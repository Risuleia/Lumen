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
    unsafe {
        let uxtheme = LoadLibraryW(windows_core::w!("uxtheme.dll")).unwrap();

        let set_mode: extern "system" fn(i32) -> i32 =
            std::mem::transmute(GetProcAddress(uxtheme, PCSTR(135 as *const u8)));

        set_mode(2);
    }

    let menu = Menu::new();

    let (tray_img, menu_img) = load_icon();

    let header = IconMenuItem::new("Lumen", true, Some(menu_img), None);
    let open_config = MenuItem::new("Open Config", true, None);
    let check_updates = MenuItem::new("Check for Updates", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit Lumen", true, None);

    let check_updates_id = check_updates.id().clone();
    let open_config_id = open_config.id().clone();
    let quit_id = quit.id().clone();

    menu.append(&header).unwrap();
    menu.append(&separator).unwrap();
    menu.append(&open_config).unwrap();
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

    let check_updates = Arc::new(check_updates);

    let poll_timer = slint::Timer::default();
    poll_timer.start(slint::TimerMode::Repeated, Duration::from_millis(100), move || {
        match &*state.lock().unwrap() {
            UpdateState::Idle | UpdateState::Failed => {
                check_updates.set_text("Check for Updates");
                check_updates.set_enabled(true);
            }
            UpdateState::Checking => {
                check_updates.set_text("Checking...");
                check_updates.set_enabled(false);
            }
            UpdateState::NotAvailable => {
                check_updates.set_text(format!("No update available"));
                check_updates.set_enabled(false);
            }
            UpdateState::Available(ver) => {
                check_updates.set_text(format!("Update to v{ver}"));
                check_updates.set_enabled(true);
            }
            UpdateState::Downloading => {
                check_updates.set_text("Updating...");
                check_updates.set_enabled(false);
            }
        };

        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == quit_id {
                slint::quit_event_loop().unwrap();
            } else if event.id == open_config_id {
                let _ = std::process::Command::new("explorer")
                    .arg(crate::config::config_path())
                    .spawn();
            } else if event.id == check_updates_id {
                let current = state.lock().unwrap().clone();

                match current {
                    UpdateState::Idle | UpdateState::Failed => {
                        *state.lock().unwrap() = UpdateState::Checking;
                        let state = state_clone.clone();

                        std::thread::spawn(move || match force_check_for_update() {
                            Some(ver) => {
                                *state.lock().unwrap() = UpdateState::Available(ver);
                            }
                            None => {
                                *state.lock().unwrap() = UpdateState::NotAvailable;
                                std::thread::sleep(Duration::from_secs(2));
                                *state.lock().unwrap() = UpdateState::Idle;
                            }
                        });
                    }
                    UpdateState::Available(_) => {
                        *state.lock().unwrap() = UpdateState::Downloading;
                        let state = state_clone.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = download_and_apply_update() {
                                eprintln!("[Updater] {e}");
                                *state.lock().unwrap() = UpdateState::Failed;
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
    let img = image::load_from_memory(bytes).unwrap().to_rgba8();
    let (w, h) = img.dimensions();

    (
        tray_icon::Icon::from_rgba(img.clone().into_raw(), w, h).unwrap(),
        tray_icon::menu::Icon::from_rgba(img.into_raw(), w, h).unwrap(),
    )
}
