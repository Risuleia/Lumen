use std::time::Duration;

use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem},
};
use windows::{
    Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    core::PCSTR,
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
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit Lumen", true, None);

    let quit_id = quit.id().clone();

    menu.append(&header).unwrap();
    menu.append(&separator).unwrap();
    menu.append(&quit).unwrap();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Lumen")
        .with_icon(tray_img)
        .build()
        .unwrap();

    let poll_timer = slint::Timer::default();
    poll_timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if event.id == quit_id {
                    slint::quit_event_loop().unwrap();
                }
            }
        },
    );

    (tray, poll_timer)
}

fn load_icon() -> (tray_icon::Icon, tray_icon::menu::Icon) {
    let bytes = include_bytes!("../../assets/lumen.ico");
    let img = image::load_from_memory(bytes).unwrap().to_rgba8();
    let (w, h) = img.dimensions();

    (
        tray_icon::Icon::from_rgba(img.clone().into_raw(), w, h).unwrap(),
        tray_icon::menu::Icon::from_rgba(img.into_raw(), w, h).unwrap(),
    )
}
