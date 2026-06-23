use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::watch;
use windows::Win32::{
    Foundation::{HANDLE, WIN32_ERROR},
    System::{
        Registry::{
            HKEY, HKEY_CURRENT_USER, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
            RegCloseKey, RegNotifyChangeKeyValue,
        },
        Threading::{CreateEventW, ResetEvent},
    },
};
use winreg::RegKey;

use crate::{CoreEvent, bus::EventSender, runtime::RuntimeState, services::Service};

pub struct CameraService {
    active: bool,
}

#[async_trait]
impl Service for CameraService {
    fn new() -> Self {
        Self { active: false }
    }

    async fn run(mut self, tx: EventSender, runtime: Arc<RuntimeState>) {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build camera runtime");

            rt.block_on(async move {
                if let Err(e) = run_event_driven(&mut self, tx, runtime).await {
                    eprintln!("[CameraService] Fatal error: {e:?}");
                }
            })
        });
    }
}

struct SessionHandlers {
    hkcu_root: HKEY,
    event_handle: HANDLE,
}

unsafe impl Send for SessionHandlers {}
unsafe impl Sync for SessionHandlers {}

impl SessionHandlers {
    fn new(notify_tx: watch::Sender<()>) -> Option<Self> {
        unsafe {
            let path = windows::core::w!(
                "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam"
            );
            let mut hkcu_root = HKEY::default();

            let status = windows::Win32::System::Registry::RegOpenKeyExW(
                HKEY_CURRENT_USER,
                path,
                Some(0),
                windows::Win32::System::Registry::KEY_NOTIFY,
                &mut hkcu_root,
            );

            if status != WIN32_ERROR(0) {
                return None;
            }

            let event_handle = CreateEventW(None, true, false, None).ok()?;

            let _ = RegNotifyChangeKeyValue(
                hkcu_root,
                true,
                REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET,
                Some(event_handle),
                true,
            );

            let loop_root_raw = hkcu_root.0 as usize;
            let loop_handle_raw = event_handle.0 as usize;

            tokio::task::spawn_blocking(move || {
                let thread_root = HKEY(loop_root_raw as *mut std::ffi::c_void);
                let thread_handle = HANDLE(loop_handle_raw as *mut std::ffi::c_void);

                loop {
                    let wait_result = windows::Win32::System::Threading::WaitForSingleObject(
                        thread_handle,
                        windows::Win32::System::Threading::INFINITE,
                    );

                    if wait_result.0 != 0 {
                        break;
                    }

                    let _ = notify_tx.send(());

                    let _ = ResetEvent(thread_handle);

                    let _ = RegNotifyChangeKeyValue(
                        thread_root,
                        true,
                        REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET,
                        Some(thread_handle),
                        true,
                    );
                }
            });

            Some(Self { hkcu_root, event_handle })
        }
    }
}

impl Drop for SessionHandlers {
    fn drop(&mut self) {
        unsafe {
            if !self.event_handle.is_invalid() {
                let _ = windows::Win32::Foundation::CloseHandle(self.event_handle);
            }
            if !self.hkcu_root.is_invalid() {
                let _ = RegCloseKey(self.hkcu_root);
            }
        }
    }
}

async fn run_event_driven(
    service: &mut CameraService,
    tx: EventSender,
    runtime: Arc<RuntimeState>,
) -> anyhow::Result<()> {
    let (notify_tx, mut notify_rx) = watch::channel(());

    let _handlers = SessionHandlers::new(notify_tx);

    let hkcu = RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let mut cached_app_paths = prebuild_registry_cache(&hkcu);

    let initial_state = evaluate_camera_state(&cached_app_paths);
    service.active = initial_state;
    runtime.camera.store(initial_state, std::sync::atomic::Ordering::Relaxed);
    let _ =
        tx.send(if initial_state { CoreEvent::CameraActive } else { CoreEvent::CameraInactive });

    loop {
        notify_rx.changed().await.ok();

        tokio::time::sleep(Duration::from_millis(80)).await;
        while notify_rx.has_changed().unwrap_or(false) {
            notify_rx.mark_unchanged();
        }

        let mut current = evaluate_camera_state(&cached_app_paths);

        if !current {
            cached_app_paths = prebuild_registry_cache(&hkcu);
            current = evaluate_camera_state(&cached_app_paths);
        }

        if current != service.active {
            service.active = current;
            runtime.camera.store(current, std::sync::atomic::Ordering::Relaxed);
            let _ =
                tx.send(if current { CoreEvent::CameraActive } else { CoreEvent::CameraInactive });
        }
    }
}

struct CachedKey {
    key: RegKey,
}

fn prebuild_registry_cache(hkcu: &RegKey) -> Vec<CachedKey> {
    let mut cache = Vec::with_capacity(32);
    let bases = [
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam\\NonPackaged",
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam\\Packaged",
    ];

    for base_path in bases {
        if let Ok(root_key) = hkcu.open_subkey(base_path) {
            for entry in root_key.enum_keys().flatten() {
                if let Ok(app_key) = root_key.open_subkey(&entry) {
                    cache.push(CachedKey { key: app_key });
                }
            }
        }
    }
    cache
}

#[inline(always)]
fn evaluate_camera_state(cached_paths: &[CachedKey]) -> bool {
    for target in cached_paths {
        if let Ok(stop) = target.key.get_value::<u64, _>("LastUsedTimeStop") {
            if stop == 0 {
                return true;
            }
        }
    }
    false
}
