use std::{sync::{Arc, Mutex}, time::Duration};

use i_slint_backend_winit::WinitWindowAccessor;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::ComponentHandle;
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, LWA_ALPHA, SM_CXSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE
    },
    UI::HiDpi::GetDpiForWindow
};

use crate::{geometry::SHELL_WIDTH, platform::{clickthrough::WINDOW_HWND, cursor::{cursor_position, point_inside_pill}, set_clickthrough}, state::IslandState};

pub fn initialize_window<T>(
    component: &T, 
    width: i32, 
    height: i32,
    state: Arc<Mutex<IslandState>>
)
where
    T: ComponentHandle + 'static,
{
    let weak = component.as_weak();

    slint::Timer::single_shot(Duration::from_millis(50), move || {
        if let Some(component) = weak.upgrade() {
            with_hwnd(&component, |hwnd| unsafe {
                configure_window(hwnd);
                position_top_center(hwnd, width, height);
                
                WINDOW_HWND.set(hwnd.0 as isize).ok();
                set_clickthrough(hwnd, true);
                start_clickthrough_loop(hwnd, state.clone());
            });
        }
    });
}

fn with_hwnd<T>(component: &T, f: impl FnOnce(HWND))
where
    T: ComponentHandle,
{
    component.window().with_winit_window(|w| {
        if let Ok(handle) = w.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                let hwnd = HWND(h.hwnd.get() as *mut _);
                f(hwnd);
            }
        }
    });
}

unsafe fn configure_window(hwnd: HWND) {
    let style = WS_POPUP.0 as isize | WS_VISIBLE.0 as isize;

    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);

        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

        ex_style &= !(WS_EX_APPWINDOW.0 as isize);
        ex_style |= WS_EX_TOOLWINDOW.0 as isize;
        ex_style |= WS_EX_LAYERED.0 as isize;

        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);

        let _ = SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            255,
            LWA_ALPHA,
        );

        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .ok();
    }
}

unsafe fn position_top_center(hwnd: HWND, width: i32, height: i32) {
    unsafe {
        let screen_width = GetSystemMetrics(SM_CXSCREEN);

        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            (screen_width - width) / 2,
            0,
            width,
            height,
            SWP_NOACTIVATE,
        )
        .ok();
    }
}

unsafe fn start_clickthrough_loop(hwnd: HWND, state: Arc<Mutex<IslandState>>) {
    let timer = Box::leak(Box::new(slint::Timer::default()));
    
    let mut clickthrough_enabled = true;
    
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let (mx, my) = cursor_position();
            
            let mut rect = RECT::default();
            
            unsafe { GetWindowRect(hwnd, &mut rect).ok(); }

            let logical = {
                let state = state.lock().unwrap();
                state.clone().bounds()
            };
            
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let scale_factor = dpi as f64 / 96.0;
            let bounds = logical.physical(scale_factor);
            
            let island_x = (SHELL_WIDTH - bounds.width) / 2;
            
            let island_left = rect.left + island_x;
            let island_top = rect.top + bounds.y;

            let px = mx - island_left;
            let py = my - island_top;
            
            let inside = point_inside_pill(
                px, 
                py, 
                bounds.width, 
                bounds.height, 
                bounds.radius
            );
                
            unsafe {
                if inside && clickthrough_enabled {
                    set_clickthrough(hwnd, false);
                    clickthrough_enabled = false;
                }

                if !inside && !clickthrough_enabled {
                    set_clickthrough(hwnd, true);
                    clickthrough_enabled = true;
                }
            }
        },
    );
}