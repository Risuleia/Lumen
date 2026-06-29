use std::{
    collections::VecDeque,
    f32::consts::PI,
    sync::{Arc, mpsc},
    time::Duration,
};

use anyhow::{Result, bail};
use async_trait::async_trait;
use rustfft::{FftPlanner, num_complex::Complex};
use windows::Win32::{
    Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        Endpoints::IAudioEndpointVolume, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, WAVEFORMATEX,
        eConsole, eRender,
    },
    System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    },
};
use windows_core::implement;

use crate::{
    bus::EventSender,
    runtime::RuntimeState,
    services::Service,
    utils::simd_audio::{simd_apply_spatial_filter, simd_extract_magnitudes, simd_window_and_cast},
};

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 24;

const STIFFNESS: f32 = 260.0;
const DAMPNESS: f32 = 4.0;

pub struct AudioSpectrumService {
    filterbank: ConstantQFilterBank,
    kinetic_bands: [KineticBand; NUM_BANDS],
}

#[async_trait]
impl Service for AudioSpectrumService {
    fn new() -> Self {
        let mut target_sample_rate = 44100.0f32;

        unsafe {
            if let Ok(enumerator) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            {
                if let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    if let Ok(audio_client) = device.Activate::<IAudioClient>(CLSCTX_ALL, None) {
                        if let Ok(pwfx_ptr) = audio_client.GetMixFormat() {
                            target_sample_rate = (*pwfx_ptr).nSamplesPerSec as f32;
                            CoTaskMemFree(Some(pwfx_ptr as *mut _));
                        }
                    }
                }
            }
        }

        Self {
            filterbank: ConstantQFilterBank::new(target_sample_rate),
            kinetic_bands: [KineticBand::default(); NUM_BANDS],
        }
    }

    async fn run(self, _tx: EventSender, runtime: Arc<RuntimeState>) {
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            loop {
                match run_loopback_timer_driven(
                    runtime.clone(),
                    &self.filterbank,
                    self.kinetic_bands,
                ) {
                    Ok(_) => break,
                    Err(e) => {
                        eprintln!("[AudioSpectrum] Reinitializing after: {e}");
                        if let Ok(mut lock) = runtime.spectrum.write() {
                            *lock = [0.0f32; NUM_BANDS];
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
        });
    }
}

#[derive(Clone, Copy, Default)]
pub struct KineticBand {
    pub current_height: f32,
    pub velocity: f32,
}

fn run_loopback_timer_driven(
    runtime: Arc<RuntimeState>,
    filterbank: &ConstantQFilterBank,
    mut kinetic_bands: [KineticBand; NUM_BANDS],
) -> Result<()> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };

    let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
    let volume_control: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None)? };

    let pwfx_ptr = unsafe { audio_client.GetMixFormat()? };
    let format: WAVEFORMATEX = unsafe { *pwfx_ptr };

    let is_float = format.wBitsPerSample == 32;
    let channels = format.nChannels as usize;

    let mut default_period: i64 = 0;
    let mut minimum_period: i64 = 0;
    unsafe {
        audio_client.GetDevicePeriod(Some(&mut default_period), Some(&mut minimum_period))?;
    }

    let sleep_duration = Duration::from_nanos((default_period * 100) as u64);

    unsafe {
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            default_period,
            0,
            pwfx_ptr,
            None,
        )?;

        CoTaskMemFree(Some(pwfx_ptr as *const _));
    }

    let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService()? };
    unsafe {
        audio_client.Start()?;
    }

    let (device_change_tx, device_change_rx) = mpsc::sync_channel::<()>(1);
    let notifier: IMMNotificationClient = DeviceChangeNotifier { tx: device_change_tx }.into();
    unsafe {
        enumerator.RegisterEndpointNotificationCallback(&notifier)?;
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    let window_coefficients: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE as f32)).cos()))
        .collect();

    let mut sample_ring_buffer = VecDeque::<f32>::with_capacity(FFT_SIZE * 2);
    let mut fft_input_buffer = vec![Complex { re: 0.0f32, im: 0.0f32 }; FFT_SIZE];
    let mut magnitude_bins = vec![0.0f32; FFT_SIZE / 2];
    let mut band_smoothing_cache = [0.0f32; NUM_BANDS];

    let _guard = NotifierGuard(&enumerator, &notifier);

    loop {
        std::thread::sleep(sleep_duration);

        if device_change_rx.try_recv().is_ok() {
            while device_change_rx.try_recv().is_ok() {}
            bail!("Default audio device changed");
        }

        let is_muted = unsafe { volume_control.GetMute()?.as_bool() };
        let system_volume = unsafe { volume_control.GetMasterVolumeLevelScalar()? };
        let volume_multiplier =
            if is_muted || system_volume < 0.01 { 0.0f32 } else { 1.0f32 / system_volume };

        let mut packet_size = unsafe { capture_client.GetNextPacketSize()? };
        let mut loop_fuse = 0;

        while packet_size > 0 {
            loop_fuse += 1;
            if loop_fuse > 64 {
                break;
            }

            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;

            unsafe {
                capture_client.GetBuffer(&mut data_ptr, &mut frames, &mut flags, None, None)?;
            }

            if frames == 0 {
                unsafe {
                    let _ = capture_client.ReleaseBuffer(0);
                }
                break;
            }

            let total_samples = frames as usize * channels;

            if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data_ptr.is_null() {
                sample_ring_buffer.extend(std::iter::repeat(0.0).take(frames as usize));
            } else if is_float {
                let raw_float_ptr = data_ptr as *const f32;
                let mut sample_idx = 0;

                while sample_idx < total_samples {
                    let mut sum = 0.0f32;
                    for _ in 0..channels {
                        sum += unsafe { *raw_float_ptr.add(sample_idx) };
                        sample_idx += 1;
                    }
                    sample_ring_buffer.push_back((sum / channels as f32) * volume_multiplier);
                }
            } else {
                let raw_short_ptr = data_ptr as *const i16;
                let mut sample_idx = 0;

                while sample_idx < total_samples {
                    let mut sum = 0.0f32;
                    for _ in 0..channels {
                        sum += unsafe { *raw_short_ptr.add(sample_idx) } as f32 / 32768.0;
                        sample_idx += 1;
                    }
                    sample_ring_buffer.push_back((sum / channels as f32) * volume_multiplier);
                }
            }

            unsafe {
                capture_client.ReleaseBuffer(frames)?;
            }
            packet_size = unsafe { capture_client.GetNextPacketSize()? };
        }

        if sample_ring_buffer.len() > FFT_SIZE * 2 {
            sample_ring_buffer.drain(..sample_ring_buffer.len() - FFT_SIZE);
        }

        let mut state_changed = false;

        sample_ring_buffer.make_contiguous();
        while sample_ring_buffer.len() >= FFT_SIZE {
            let (front_slice, _) = sample_ring_buffer.as_slices();

            unsafe {
                simd_window_and_cast(front_slice, &window_coefficients, &mut fft_input_buffer);
            }

            fft.process(&mut fft_input_buffer);

            unsafe {
                simd_extract_magnitudes(&fft_input_buffer, &mut magnitude_bins);
            }

            let mut raw_db_targets = [0.0f32; NUM_BANDS];
            let magnitudes_array: &[f32; FFT_SIZE / 2] =
                magnitude_bins[..FFT_SIZE / 2].try_into().unwrap();
            filterbank.compute_targets(magnitudes_array, &mut raw_db_targets);

            let dt = 1.0 / 60.0;
            for idx in 0..NUM_BANDS {
                let target = raw_db_targets[idx];
                let band = &mut kinetic_bands[idx];

                let error = target - band.current_height;
                let spring_force = (STIFFNESS * error) - (DAMPNESS * band.velocity);

                band.velocity += spring_force * dt;
                let next_height = band.current_height + band.velocity * dt;

                band.current_height = next_height.clamp(0.15, 1.0);

                if band.current_height != next_height {
                    band.velocity = 0.0;
                }
            }

            unsafe {
                simd_apply_spatial_filter(&kinetic_bands, &mut band_smoothing_cache);
            }

            state_changed = true;
            sample_ring_buffer.drain(..FFT_SIZE);
        }

        if state_changed {
            if let Ok(mut lock) = runtime.spectrum.write() {
                *lock = band_smoothing_cache;
            }
        }
    }
}

struct ConstantQFilterBank {
    band_mappings: Vec<Vec<(usize, f32)>>,
}

impl ConstantQFilterBank {
    pub fn new(sample_rate: f32) -> Self {
        let mut band_mappings = vec![Vec::new(); NUM_BANDS];

        let f_min = 27.5;
        let bins_per_octave = 3.0;
        let bin_resolution = (sample_rate / 2.0) / (FFT_SIZE as f32 / 2.0);

        let q = 1.0 / (2.0_f32.powf(1.0 / bins_per_octave) - 1.0);

        for i in 0..NUM_BANDS {
            let center_freq = f_min * 2.0_f32.powf(i as f32 / bins_per_octave);

            let bandwidth = center_freq / q;
            let f_low = center_freq - (bandwidth / 2.0);
            let f_high = center_freq + (bandwidth / 2.0);

            let bin_start = (f_low / bin_resolution).floor() as usize;
            let bin_end = ((f_high / bin_resolution).ceil() as usize).min(FFT_SIZE / 2);

            let mut weight_sum = 0.0;
            let mut temp_weights = Vec::new();

            for bin in bin_start..bin_end {
                let bin_freq = bin as f32 * bin_resolution;

                let distance = (bin_freq - center_freq).abs();
                if distance < (bandwidth / 2.0) {
                    let weight = 1.0 - (distance / (bandwidth / 2.0));
                    temp_weights.push((bin, weight));
                    weight_sum += weight;
                }
            }

            if weight_sum > 0.0 {
                for (bin, weight) in temp_weights {
                    band_mappings[i].push((bin, weight / weight_sum));
                }
            }
        }

        Self { band_mappings }
    }

    pub fn compute_targets(
        &self,
        fft_magnitudes: &[f32; FFT_SIZE / 2],
        raw_db_targets: &mut [f32; NUM_BANDS],
    ) {
        let mut max_tracked_db = -100.0f32;
        let mut temp_dbs = [0.0f32; NUM_BANDS];

        for i in 0..NUM_BANDS {
            let mut energy = 0.0;
            for &(bin, weight) in &self.band_mappings[i] {
                energy += fft_magnitudes[bin] * weight;
            }

            let db = 20.0 * (energy + 1e-6).log10();
            temp_dbs[i] = db;

            if db > max_tracked_db {
                max_tracked_db = db;
            }
        }

        let dynamic_ceiling = max_tracked_db.max(-18.0);
        let dynamic_floor = dynamic_ceiling - 26.0;
        for i in 0..NUM_BANDS {
            let db = temp_dbs[i];

            raw_db_targets[i] = if db < dynamic_floor {
                0.0
            } else {
                ((db - dynamic_floor) / (dynamic_ceiling - dynamic_floor)).clamp(0.0, 1.0)
            };
        }
    }
}

#[implement(IMMNotificationClient)]
struct DeviceChangeNotifier {
    tx: mpsc::SyncSender<()>,
}

impl IMMNotificationClient_Impl for DeviceChangeNotifier_Impl {
    fn OnDefaultDeviceChanged(
        &self,
        flow: windows::Win32::Media::Audio::EDataFlow,
        _: windows::Win32::Media::Audio::ERole,
        _: &windows_core::PCWSTR,
    ) -> windows_core::Result<()> {
        if flow == eRender {
            let _ = self.tx.try_send(());
        }
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &windows_core::PCWSTR) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &windows_core::PCWSTR) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnDeviceStateChanged(
        &self,
        _: &windows_core::PCWSTR,
        _: windows::Win32::Media::Audio::DEVICE_STATE,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        _: &windows_core::PCWSTR,
        _: &windows::Win32::Foundation::PROPERTYKEY,
    ) -> windows_core::Result<()> {
        Ok(())
    }
}

struct NotifierGuard<'a> (&'a IMMDeviceEnumerator, &'a IMMNotificationClient);

impl Drop for NotifierGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.UnregisterEndpointNotificationCallback(self.1);
        }
    }
}
