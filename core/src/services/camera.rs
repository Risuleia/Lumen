use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

use crate::{CoreEvent, bus::EventSender, runtime::RuntimeState, services::Service};

pub struct CameraService {
    active: bool
}

#[async_trait]
impl Service for CameraService {
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
            let current = camera_active();

            if current != self.active {
                self.active = current;

                runtime.camera.store(current, std::sync::atomic::Ordering::Relaxed);

                let _ = tx.send(
                    if current {
                        CoreEvent::CameraActive
                    } else {
                        CoreEvent::CameraInactive
                    }
                );
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

fn camera_active() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let Ok(nonpackaged) = hkcu.open_subkey(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam\\NonPackaged"
    ) else {
        return false;
    };

    for entry in nonpackaged.enum_keys().flatten() {
        let Ok(app_key) = nonpackaged.open_subkey(&entry) else {
            continue;
        };

        let last_used_stop: Result<u64, _> = app_key.get_value("LastUsedTimeStop");

        if let Ok(stop) = last_used_stop {
            if stop == 0 {
                return true;
            }
        }
    }

    return false;
}