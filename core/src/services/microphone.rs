use std::{sync::{Arc, atomic::Ordering}, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use windows::Win32::{Media::Audio::{AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications}, System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx}};
use windows_core::Interface;

use crate::{CoreEvent, bus::EventSender, runtime::RuntimeState, services::Service};

pub struct MicrophoneService {
    active: bool
}

#[async_trait]
impl Service for MicrophoneService {
    fn new() -> Self {
        Self { active: false }
    }

    async fn run(
        mut self,
        tx: EventSender,
        runtime: Arc<RuntimeState>
    ) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        
        loop {
            let current = microphone_active();

            if current != self.active {
                self.active = current;

                runtime.mic.store(current, Ordering::Relaxed);

                let _ = tx.send(
                    if current {
                        CoreEvent::MicrophoneActive
                    } else {
                        CoreEvent::MicrophoneInactive
                    }
                );
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

fn microphone_active() -> bool {
    let detector = MicrophoneDetector::new()
        .expect("Failed to create detector");

    detector.active().unwrap_or(false)
}

pub struct MicrophoneDetector {
    enumerator: IMMDeviceEnumerator
}

impl MicrophoneDetector {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(
                &MMDeviceEnumerator, 
                None, 
                CLSCTX_ALL
            )?
        };

        Ok(Self { enumerator })
    }

    pub fn active(&self) -> Result<bool> {
        unsafe {
            let device = self.enumerator.GetDefaultAudioEndpoint(
                eCapture, 
                eCommunications
            )?;

            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;

            let session_enum = manager.GetSessionEnumerator()?;

            let count = session_enum.GetCount()?;

            for i in 0..count {
                let control = session_enum.GetSession(i)?;

                let control2: IAudioSessionControl2 = control.cast()?;

                let state = control.GetState()?;

                if state == AudioSessionStateActive {
                    let pid = control2.GetProcessId()?;

                    if pid != 0 {
                        return Ok(true);
                    }
                }
            }

            Ok(false)
        }
    }
}