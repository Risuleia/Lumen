use anyhow::Result;
use windows::Win32::{
    Media::Audio::*,
    System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx},
};

pub struct Loopback {
    _client: IAudioClient,
    capture: IAudioCaptureClient,
    channels: u32,
}

impl Loopback {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let dev_enum: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let device = dev_enum.GetDefaultAudioEndpoint(eRender, eConsole)?;

            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            let mix = client.GetMixFormat()?;
            let channels = (*mix).nChannels as u32;

            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                0,
                0,
                mix,
                None,
            )?;

            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;

            Ok(Self {
                _client: client,
                capture,
                channels,
            })
        }
    }

    pub fn poll(&self) -> Result<Option<Vec<f32>>> {
        unsafe {
            let packet = self.capture.GetNextPacketSize()?;
            if packet == 0 {
                return Ok(None);
            }

            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;

            self.capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;

            let frames_us = frames as usize;
            let channels = self.channels as usize;

            let mut mono = Vec::with_capacity(frames_us);

            // 🔥 SILENT BUFFER → return zeros
            if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                mono.resize(frames_us, 0.0);
            } else {
                // 🔥 Real audio data
                let slice = std::slice::from_raw_parts(data as *const f32, frames_us * channels);

                if channels == 2 {
                    for c in slice.chunks_exact(2) {
                        mono.push((c[0] + c[1]) * 0.5);
                    }
                } else {
                    for frame in slice.chunks_exact(channels) {
                        let mut sum = 0.0;
                        for &s in frame {
                            sum += s;
                        }
                        mono.push(sum / channels as f32);
                    }
                }
            }

            self.capture.ReleaseBuffer(frames)?;
            Ok(Some(mono))
        }
    }
}
