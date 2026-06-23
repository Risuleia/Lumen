#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{
    _mm_storeu_ps, _mm256_castps256_ps128, _mm256_hadd_ps, _mm256_loadu_ps, _mm256_mul_ps,
    _mm256_setzero_ps, _mm256_sqrt_ps, _mm256_storeu_ps, _mm256_unpackhi_ps, _mm256_unpacklo_ps,
};
use std::{collections::VecDeque, f32::consts::PI, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use rustfft::{FftPlanner, num_complex::Complex};
use windows::Win32::{
    Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
        eConsole, eRender,
    },
    System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
};

use crate::{bus::EventSender, runtime::RuntimeState, services::Service};

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 24;

pub struct AudioSpectrumService;

#[async_trait]
impl Service for AudioSpectrumService {
    fn new() -> Self {
        Self
    }

    async fn run(self, _tx: EventSender, runtime: Arc<RuntimeState>) {
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            if let Err(e) = run_loopback_timer_driven(runtime) {
                eprintln!("[AudioSpectrum] Fatal error: {e}");
            }
        });
    }
}

struct BandRange {
    start: usize,
    end: usize,
}

fn run_loopback_timer_driven(runtime: Arc<RuntimeState>) -> Result<()> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
    let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };

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
    }

    let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService()? };
    unsafe {
        audio_client.Start()?;
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    let window_coefficients: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE as f32)).cos()))
        .collect();

    let mut band_ranges = Vec::with_capacity(NUM_BANDS);
    let nyquist = format.nSamplesPerSec as f32 / 2.0;
    let log_min = 40.0f32.ln();
    let log_max = nyquist.ln();
    let bins_len = FFT_SIZE / 2;

    for i in 0..NUM_BANDS {
        let f0 = (log_min + (i as f32 / NUM_BANDS as f32) * (log_max - log_min)).exp();
        let f1 = (log_min + ((i + 1) as f32 / NUM_BANDS as f32) * (log_max - log_min)).exp();

        let index_start = ((f0 / nyquist) * bins_len as f32) as usize;
        let mut index_end = ((f1 / nyquist) * bins_len as f32) as usize;

        if index_end <= index_start {
            index_end = index_start + 1;
        }

        band_ranges
            .push(BandRange { start: index_start.min(bins_len - 1), end: index_end.min(bins_len) });
    }

    let mut sample_ring_buffer = VecDeque::<f32>::with_capacity(FFT_SIZE * 2);
    let mut fft_input_buffer = vec![Complex { re: 0.0f32, im: 0.0f32 }; FFT_SIZE];
    let mut magnitude_bins = vec![0.0f32; bins_len];
    let mut band_smoothing_cache = [0.0f32; NUM_BANDS];

    loop {
        std::thread::sleep(sleep_duration);

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
                    sample_ring_buffer.push_back(sum / channels as f32);
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
                    sample_ring_buffer.push_back(sum / channels as f32);
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

        while sample_ring_buffer.len() >= FFT_SIZE {
            let (front_slice, _) = sample_ring_buffer.as_slices();

            unsafe {
                simd_window_and_cast(front_slice, &window_coefficients, &mut fft_input_buffer);
            }

            fft.process(&mut fft_input_buffer);

            unsafe {
                simd_extract_magnitudes(&fft_input_buffer, &mut magnitude_bins);
            }

            for idx in 0..NUM_BANDS {
                let range = &band_ranges[idx];
                let mut energy = 0.0f32;
                let mut count = 0;

                for j in range.start..range.end {
                    energy += magnitude_bins[j];
                    count += 1;
                }

                let v = if count > 0 { energy / count as f32 } else { 0.0 };
                let db = 20.0 * v.max(1e-4).log10();
                let target_value = ((db + 45.0) / 42.0).clamp(0.0, 1.0);

                if target_value > band_smoothing_cache[idx] {
                    band_smoothing_cache[idx] =
                        band_smoothing_cache[idx] * 0.65 + target_value * 0.35;
                } else {
                    band_smoothing_cache[idx] = band_smoothing_cache[idx] * 0.985;
                }
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

#[inline(always)]
#[cfg(target_arch = "x86_64")]
pub unsafe fn simd_window_and_cast(
    samples_source: &[f32],
    window_coeffs: &[f32],
    fft_input_buffer: &mut [Complex<f32>],
) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            let mut idx = 0;
            while idx < 2048 {
                let audio_vector = _mm256_loadu_ps(samples_source.as_ptr().add(idx));
                let window_vector = _mm256_loadu_ps(window_coeffs.as_ptr().add(idx));
                let windowed_real = _mm256_mul_ps(audio_vector, window_vector);
                let imag_zeros = _mm256_setzero_ps();

                let complex_low = _mm256_unpacklo_ps(windowed_real, imag_zeros);
                let complex_high = _mm256_unpackhi_ps(windowed_real, imag_zeros);

                let dest_ptr = fft_input_buffer.as_mut_ptr().add(idx) as *mut f32;
                _mm256_storeu_ps(dest_ptr, complex_low);
                _mm256_storeu_ps(dest_ptr.add(8), complex_high);

                idx += 8;
            }
        }
    } else {
        for idx in 0..2048 {
            fft_input_buffer[idx] =
                Complex { re: samples_source[idx] * window_coeffs[idx], im: 0.0 };
        }
    }
}

#[inline(always)]
#[cfg(target_arch = "x86_64")]
pub unsafe fn simd_extract_magnitudes(fft_output: &[Complex<f32>], magnitude_bins: &mut [f32]) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            let source_ptr = fft_output.as_ptr() as *const f32;
            let mut idx = 0;

            while idx < 1024 {
                let complex_vector = _mm256_loadu_ps(source_ptr.add(idx * 2));
                let squared_elements = _mm256_mul_ps(complex_vector, complex_vector);

                let horizontal_sums = _mm256_hadd_ps(squared_elements, squared_elements);
                let computed_magnitudes = _mm256_sqrt_ps(horizontal_sums);

                let clean_low_register = _mm256_castps256_ps128(computed_magnitudes);
                _mm_storeu_ps(magnitude_bins.as_mut_ptr().add(idx), clean_low_register);

                idx += 4;
            }
        }
    } else {
        for idx in 0..1024 {
            magnitude_bins[idx] = fft_output[idx].norm();
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn simd_window_and_cast(s: &[f32], w: &[f32], b: &mut [Complex<f32>]) {
    for idx in 0..2048 {
        b[idx] = Complex { re: s[idx] * w[idx], im: 0.0 };
    }
}
#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn simd_extract_magnitudes(f: &[Complex<f32>], m: &mut [f32]) {
    for idx in 0..1024 {
        m[idx] = f[idx].norm();
    }
}
