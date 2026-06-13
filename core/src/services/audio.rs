use std::{collections::VecDeque, f32::consts::PI, ptr::null_mut, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use rustfft::{FftPlanner, num_complex::Complex, num_traits::Float};
use windows::Win32::{
    Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender
    },
    System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
};

use crate::{bus::EventSender, runtime::RuntimeState, services::Service};

const FFT_SIZE: usize = 2048;
const NUM_BANDS: usize = 24;

pub struct AudioSpectrumService;

#[async_trait]
impl Service for AudioSpectrumService {
    fn new() -> Self { Self }

    async fn run(
        self,
        _tx: EventSender,
        runtime: Arc<RuntimeState>
    ) {
        unsafe {
            if let Err(e) = run_loopback(runtime) {
                eprintln!("[AudioSpectrum] {e}");
            }
        }
    }
}


unsafe fn run_loopback(runtime: Arc<RuntimeState>) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(
            &MMDeviceEnumerator, 
            None, 
            CLSCTX_ALL
        )?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    
        let audio_client: IAudioClient = device.Activate(
            CLSCTX_ALL, 
            Some(null_mut())
        )?;
    
        let pwfx = audio_client.GetMixFormat()?;
        let format = *pwfx;
    
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            10_000_000,
            0,
            pwfx,
            None,
        )?;
    
        let capture: IAudioCaptureClient = audio_client.GetService()?;
    
        audio_client.Start()?;
    
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
    
        let mut ring = VecDeque::<f32>::new();
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE as f32)).cos())
            })
            .collect();
    
        let mut smooth = [0.0f32; NUM_BANDS];

        let mut buffer = vec![Complex { re: 0.0f32, im: 0.0f32 }; FFT_SIZE];
        let mut mags = vec![0.0f32; FFT_SIZE / 2];
    
        loop {
            let mut packet = capture.GetNextPacketSize()?;
            
            while packet > 0 {
                let mut data_ptr: *mut u8 = null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
    
                capture.GetBuffer(
                    &mut data_ptr, 
                    &mut frames, 
                    &mut flags, 
                    None, 
                    None
                )?;
    
                if flags as i32 & AUDCLNT_BUFFERFLAGS_SILENT.0 != 0 {
                    for _ in 0..frames {
                        ring.push_back(0.0);
                    }
                } else {
                    let samples = std::slice::from_raw_parts(
                        data_ptr as *const f32, 
                        frames as usize * format.nChannels as usize
                    );
    
                    for frame in samples.chunks(format.nChannels as usize) {
                        let mono = frame.iter().sum::<f32>() / frame.len() as f32;
                        ring.push_back(mono);
                    }
                }
    
                capture.ReleaseBuffer(frames)?;
                packet = capture.GetNextPacketSize()?;
            }

            while ring.len() > FFT_SIZE * 4 {
                ring.pop_front();
            }
    
            while ring.len() >= FFT_SIZE {
                for i in 0..FFT_SIZE {
                    let sample = ring.pop_front().unwrap();
                    buffer[i] = Complex { re: sample * window[i], im: 0.0 };
                }

                fft.process(&mut buffer);

                for i in 0..FFT_SIZE / 2 {
                    mags[i] = buffer[i].norm();
                }

                let bands = compute_bands(&mags, format.nSamplesPerSec as usize);

                for i in 0..NUM_BANDS {
                    let target = bands[i];

                    if target > smooth[i] {
                        smooth[i] = smooth[i] * 0.65 + target * 0.35;
                    } else {
                        smooth[i] *= 0.94;
                    }
                }

                *runtime.spectrum.write().unwrap() = smooth;
            }
    
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn compute_bands(mags: &[f32], sample_rate: usize) -> [f32; NUM_BANDS] {
    let mut bands = [0.0f32; NUM_BANDS];

    let nyquist = sample_rate as f32 / 2.0;
    let min_freq = 40.0;
    let max_freq = nyquist;

    let log_min = min_freq.ln();
    let log_max = max_freq.ln();

    for i in 0..NUM_BANDS {
        let f0 = (log_min + (i as f32 / NUM_BANDS as f32) * (log_max - log_min)).exp();
        let f1 = (log_min + ((i + 1) as f32 / NUM_BANDS as f32) * (log_max - log_min)).exp();

        let i0 = ((f0 / nyquist) * mags.len() as f32) as usize;
        let i1 = ((f1 / nyquist) * mags.len() as f32) as usize;

        let i0 = i0.min(mags.len() - 1);
        let i1 = i1.min(mags.len());

        let mut energy = 0.0;
        let mut count = 0;

        for j in i0..i1 {
            energy += mags[j];
            count += 1;
        }

        let v = if count > 0 { energy / count as f32 } else { 0.0 };

        let db = 20.0 * v.max(0.000001).log10();

        bands[i] = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    }

    bands
}
