use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem},
};
use windows::{
    Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    core::PCSTR,
};

use crate::platform::updater::{
    UpdateState, download_and_apply_update, force_check_for_update, start_update_check,
};

pub fn initialize_tray() -> (TrayIcon, slint::Timer) {
    if let Ok(uxtheme) = unsafe { LoadLibraryW(windows_core::w!("uxtheme.dll")) } {
        unsafe {
            if let Some(proc_addr) = GetProcAddress(uxtheme, PCSTR(135 as *const u8)) {
                let set_mode: extern "system" fn(i32) -> i32 = std::mem::transmute(proc_addr);
                set_mode(2);
            }
        }
    }

    let menu = Menu::new();
    let (tray_img, menu_img) = load_icon();

    let header = IconMenuItem::new("Lumen", true, Some(menu_img), None);
    let check_updates = MenuItem::new("Check for Updates", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit Lumen", true, None);

    let check_updates_id = check_updates.id().clone();
    let quit_id = quit.id().clone();

    menu.append(&header).unwrap();
    menu.append(&separator).unwrap();
    menu.append(&check_updates).unwrap();
    menu.append(&separator).unwrap();
    menu.append(&quit).unwrap();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Lumen")
        .with_icon(tray_img)
        .build()
        .unwrap();

    start_update_check();

    let state = Arc::new(Mutex::new(UpdateState::Idle));
    let state_clone = state.clone();

    let mut last_rendered_state = None;

    let poll_timer = slint::Timer::default();
    poll_timer.start(slint::TimerMode::Repeated, Duration::from_millis(100), move || {
        let current_state = {
            if let Ok(lock) = state.lock() {
                lock.clone()
            } else {
                return;
            }
        };

        if Some(current_state.clone()) != last_rendered_state {
            match &current_state {
                UpdateState::Idle | UpdateState::Failed => {
                    check_updates.set_text("Check for Updates");
                    check_updates.set_enabled(true);
                }
                UpdateState::Checking => {
                    check_updates.set_text("Checking...");
                    check_updates.set_enabled(false);
                }
                UpdateState::NotAvailable => {
                    check_updates.set_text("No update available");
                    check_updates.set_enabled(false);
                }
                UpdateState::Available(ver) => {
                    let mut text = String::with_capacity(24);
                    text.push_str("Update to v");
                    text.push_str(ver);
                    check_updates.set_text(text);
                    check_updates.set_enabled(true);
                }
                UpdateState::Downloading => {
                    check_updates.set_text("Updating...");
                    check_updates.set_enabled(false);
                }
            };
            last_rendered_state = Some(current_state.clone());
        }

        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == quit_id {
                let _ = slint::quit_event_loop();
            } else if event.id == check_updates_id {
                match current_state {
                    UpdateState::Idle | UpdateState::Failed => {
                        if let Ok(mut lock) = state.lock() {
                            *lock = UpdateState::Checking;
                        }

                        let state = state_clone.clone();
                        std::thread::spawn(move || match force_check_for_update() {
                            Some(ver) => {
                                if let Ok(mut lock) = state.lock() {
                                    *lock = UpdateState::Available(ver);
                                }
                            }
                            None => {
                                if let Ok(mut lock) = state.lock() {
                                    *lock = UpdateState::NotAvailable;
                                }
                                std::thread::sleep(Duration::from_secs(2));
                                if let Ok(mut lock) = state.lock() {
                                    *lock = UpdateState::Idle;
                                }
                            }
                        });
                    }
                    UpdateState::Available(_) => {
                        if let Ok(mut lock) = state.lock() {
                            *lock = UpdateState::Downloading;
                        }

                        let state = state_clone.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = download_and_apply_update() {
                                eprintln!("[Updater] Update application failed: {e}");
                                if let Ok(mut lock) = state.lock() {
                                    *lock = UpdateState::Failed;
                                }
                            }
                        });
                    }
                    _ => {}
                }
            }
        }
    });

    (tray, poll_timer)
}

fn load_icon() -> (tray_icon::Icon, tray_icon::menu::Icon) {
    let bytes = include_bytes!("../../../assets/lumen.ico");

    let decoded_image = image::load_from_memory(bytes)
        .expect("Failed to parse embedded lumen.ico asset")
        .to_rgba8();

    let (width, height) = decoded_image.dimensions();
    let raw_rgba_pixels = decoded_image.into_raw();

    (
        tray_icon::Icon::from_rgba(raw_rgba_pixels.clone(), width, height).unwrap(),
        tray_icon::menu::Icon::from_rgba(raw_rgba_pixels, width, height).unwrap(),
    )
}
