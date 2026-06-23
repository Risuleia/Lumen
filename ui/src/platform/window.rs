use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use i_slint_backend_winit::{
    WinitWindowAccessor,
    winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
};
use slint::ComponentHandle;
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::UpdateWindow,
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            GWL_EXSTYLE, GWL_STYLE, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
            HWND_TOPMOST, LWA_ALPHA, SM_CXSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetLayeredWindowAttributes, SetWindowLongPtrW,
            SetWindowPos, ShowWindow, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
};

use crate::{
    geometry::SHELL_WIDTH,
    platform::{
        clickthrough::set_clickthrough,
        cursor::{cursor_position, point_inside_pill},
        fullscreen::is_foreground_fullscreen,
    },
    state::{ContentState, IslandState},
};

static WINDOW_HWND: OnceLock<isize> = OnceLock::new();

pub fn initialize_window<T>(
    component: &T,
    width: i32,
    height: i32,
    state: Arc<Mutex<IslandState>>,
    get_collapsed: impl Fn() -> bool + Send + 'static,
) where
    T: ComponentHandle + 'static,
{
    let weak = component.as_weak();

    slint::Timer::single_shot(Duration::from_millis(200), move || {
        if let Some(component) = weak.upgrade() {
            with_hwnd(&component, |hwnd| unsafe {
                configure_window(hwnd);
                position_top_center(hwnd, width, height);

                WINDOW_HWND.set(hwnd.0 as isize).ok();
                set_clickthrough(hwnd, true);

                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = UpdateWindow(hwnd);

                start_clickthrough_loop(hwnd, state.clone(), get_collapsed);
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
    let style = WS_POPUP.0 as isize;

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
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
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

unsafe fn start_clickthrough_loop(
    hwnd: HWND,
    state: Arc<Mutex<IslandState>>,
    get_collapsed: impl Fn() -> bool + Send + 'static,
) {
    let timer = Box::leak(Box::new(slint::Timer::default()));

    let mut clickthrough_enabled = true;
    let mut hidden_for_fullscreen = false;

    let mut z_order_tick_counter = 0u8;

    timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
        let fullscreen = is_foreground_fullscreen(hwnd);
        if fullscreen {
            if !hidden_for_fullscreen {
                let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
                hidden_for_fullscreen = true;
            }
            return;
        }

        if hidden_for_fullscreen {
            let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
            hidden_for_fullscreen = false;
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }

        z_order_tick_counter = z_order_tick_counter.wrapping_add(1);
        if z_order_tick_counter >= 60 {
            z_order_tick_counter = 0;
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }

        let state_guard = match state.try_lock() {
            Ok(lock) => lock,
            Err(_) => return,
        };

        let logical_bounds = state_guard.bounds();
        let has_active =
            state_guard.mic || state_guard.camera || state_guard.content != ContentState::Idle;

        drop(state_guard);

        let collapsed = get_collapsed();
        let (mx, my) = cursor_position();
        let mut rect = RECT::default();

        unsafe {
            let _ = GetWindowRect(hwnd, &mut rect);
        }

        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let scale_factor = dpi as f64 / 96.0;
        let bounds = logical_bounds.physical(scale_factor);

        let island_x = (SHELL_WIDTH - bounds.width) / 2;
        let island_left = rect.left + island_x;
        let island_top = rect.top
            + if collapsed {
                ((-(logical_bounds.height - 10)) as f64 * scale_factor).round() as i32
            } else {
                0
            };

        let px = mx - island_left;
        let py = my - island_top;

        let inside = if !collapsed && !has_active {
            false
        } else {
            point_inside_pill(px, py, bounds.width, bounds.height, bounds.radius)
        };

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
    });
}
