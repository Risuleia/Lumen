use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

use tokio::sync::broadcast;
use windows::Win32::{Media::Audio::{AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications, eConsole}, System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx}};
use windows_core::Interface;

use crate::{CoreEvent, audio::{loopback::Loopback, mic::MicCapture, smoothing::{VisualEnvelope, rms}}};

mod loopback;
mod smoothing;
mod mic;

pub fn start_loopback_thread(tx: broadcast::Sender<CoreEvent>) {
    std::thread::spawn(move || {
        let Ok(loopback) = Loopback::new() else {
            eprintln!("Loopback init failed");
            return;
        };

        let mut env = VisualEnvelope::new();

        loop {
            if let Ok(Some(samples)) = loopback.poll() {
                let raw = rms(&samples);
                let smoothed = env.push(raw);

                let _ = tx.send(CoreEvent::VisualizerFrame(smoothed));
            }

            std::thread::sleep(Duration::from_millis(30));
        }
    });
}

pub fn start_mic_thread(tx: broadcast::Sender<CoreEvent>) {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED).ok();

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();

        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eCommunications)
            .or_else(|_| enumerator.GetDefaultAudioEndpoint(eCapture, eConsole))
            .unwrap();

        let session_manager: IAudioSessionManager2 =
            device.Activate(CLSCTX_ALL, None).unwrap();

        let sessions = session_manager.GetSessionEnumerator().unwrap();

        let mut mic_active = false;

        let running = Arc::new(AtomicBool::new(false));
        let mut capture_thread = None;

        loop {
            let mut any_active = false;
            let count = sessions.GetCount().unwrap();

            for i in 0..count {
                let session = sessions.GetSession(i).unwrap();
                let ctrl: IAudioSessionControl2 = session.cast().unwrap();

                if ctrl.IsSystemSoundsSession().is_ok() {
                    continue;
                }

                if ctrl.GetState().unwrap() == AudioSessionStateActive {
                    any_active = true;
                    break;
                }
            }

            // -------- activate --------
            if any_active && !mic_active {
                mic_active = true;
                let _ = tx.send(CoreEvent::MicActive(0.0));

                let tx_clone = tx.clone();
                let run = running.clone();
                run.store(true, Ordering::Relaxed);

                capture_thread = Some(std::thread::spawn(move || {
                    if let Ok(cap) = MicCapture::new() {
                        while run.load(Ordering::Relaxed) {
                            if let Ok(Some(buf)) = cap.poll() {
                                let level = rms(&buf);
                                let _ = tx_clone.send(CoreEvent::MicActive(level));
                            }
                            std::thread::sleep(Duration::from_millis(12));
                        }
                    }
                }));
            }

            // -------- deactivate --------
            if !any_active && mic_active {
                mic_active = false;
                running.store(false, Ordering::Relaxed);

                if let Some(t) = capture_thread.take() {
                    let _ = t.join();
                }

                let _ = tx.send(CoreEvent::MicIdle);
            }

            std::thread::sleep(Duration::from_millis(250));
        }
    });
}