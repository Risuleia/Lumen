#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{cell::RefCell, rc::Rc, time::Duration};

use anyhow::{Result, anyhow};
use i_slint_backend_winit::WinitWindowAccessor;
use lumen_compositor::{LiquidGlassConfig, LiquidGlassEngine};
use raw_window_handle::HasWindowHandle;
use single_instance::SingleInstance;
use slint::{ComponentHandle, Timer, TimerMode};
use windows::Win32::{
    Foundation::{COLORREF, HWND},
    Graphics::Gdi::{
        CreateRoundRectRgn, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow, SetWindowRgn
    },
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongW, HWND_TOPMOST, LWA_ALPHA, SWP_FRAMECHANGED, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowLongW, SetWindowPos, WDA_EXCLUDEFROMCAPTURE, WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_LAYERED, WS_EX_STATICEDGE, WS_THICKFRAME
    },
};

slint::include_modules!();

fn main() -> Result<()> {
    let instance = SingleInstance::new("io.risuleia.lumen").unwrap();
    if !instance.is_single() {
        return Err(anyhow!("Already running"));
    }

    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new().unwrap()))?;

    let ui = LumenOverlay::new().unwrap();
    let weak = ui.as_weak();

    let engine_cell: Rc<RefCell<Option<LiquidGlassEngine>>> = Rc::new(RefCell::new(None));
    let engine_setup_ref = engine_cell.clone();

    slint::Timer::single_shot(Duration::from_millis(60), move || {
        if let Some(ui) = weak.upgrade() {
            let window = ui.window();

            window.with_winit_window(|w| {
                if let Ok(handle) = w.window_handle() {
                    if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                        let hwnd =
                            windows::Win32::Foundation::HWND(h.hwnd.get() as isize as *mut _);

                        unsafe {
                            let width = 300;
                            let height = 60;

                            setup_window_style(hwnd, width, height);

                            let mut mi = MONITORINFO::default();
                            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

                            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
                            let _ = GetMonitorInfoW(mon, &mut mi);

                            let screen_width = mi.rcWork.right - mi.rcWork.left;

                            let x = (screen_width / 2) - (width / 2);
                            let y = 200;

                            let _ = SetWindowPos(
                                hwnd,
                                Some(HWND_TOPMOST),
                                x,
                                y,
                                width,
                                height,
                                SWP_FRAMECHANGED | SWP_NOZORDER,
                            );

                            let window_static: &'static _ =
                                std::mem::transmute::<&_, &'static _>(w);

                            let mut slot = engine_setup_ref.borrow_mut();
                            *slot = Some(futures::executor::block_on(async {
                                LiquidGlassEngine::new(LiquidGlassConfig::default(), window_static)
                                    .await
                                    .unwrap()
                            }))
                        }
                    }
                } else {
                    eprintln!("FAILED: no window_handle()");
                }
            });
        } else {
            eprintln!("TIMER: ui already gone");
        }
    });

    let tick_engine_ref = engine_cell.clone();
    let tick_timer = Rc::new(RefCell::new(Timer::default()));

    let t = tick_timer.clone();
    t.borrow_mut().start(
        TimerMode::Repeated,
        Duration::from_millis(16), // ~60 FPS
        move || {
            if let Some(engine) = tick_engine_ref.borrow_mut().as_mut() {
                engine.tick();
            }
        },
    );

    eprintln!("MAIN: running UI");
    ui.run()?;
    Ok(())
}

unsafe fn setup_window_style(hwnd: HWND, width: i32, height: i32) {
    let mut style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    style &= !(WS_CAPTION.0 | WS_THICKFRAME.0);
    unsafe { SetWindowLongW(hwnd, GWL_STYLE, style as i32) };

    let mut ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    ex &= !(WS_EX_DLGMODALFRAME.0 | WS_EX_CLIENTEDGE.0 | WS_EX_STATICEDGE.0);
    ex |= WS_EX_LAYERED.0;
    unsafe { SetWindowLongW(hwnd, GWL_EXSTYLE, ex as i32) };

    let h_rgn = unsafe { CreateRoundRectRgn(0, 0, width, height, 26 * 2, 26 * 2) };
    if !h_rgn.is_invalid() {
        unsafe { SetWindowRgn(hwnd, Some(h_rgn), true) };
    }

    let _ = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };
    let _ = unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) };
}
