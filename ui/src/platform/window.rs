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
            GWL_EXSTYLE, GWL_STYLE, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, LWA_ALPHA, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP
        },
    },
};

use crate::{
    config::ConfigHandle,
    platform::{
        clickthrough::set_clickthrough,
        cursor::{cursor_position, point_inside_pill},
        fullscreen::is_foreground_fullscreen,
    },
    state::{ContentState, IslandState},
};

static WINDOW_HWND: OnceLock<isize> = OnceLock::new();
static TOPMOST_REASSERT_INTERVAL: u32 = 60;

pub fn initialize_window<T>(
    component: &T,
    state: Arc<Mutex<IslandState>>,
    config: ConfigHandle,
    get_collapsed: impl Fn() -> bool + Send + 'static,
) where
    T: ComponentHandle + 'static,
{
    let weak = component.as_weak();

    slint::Timer::single_shot(Duration::from_millis(200), move || {
        if let Some(component) = weak.upgrade() {
            with_hwnd(&component, |hwnd| unsafe {
                configure_window(hwnd);
                position_fullscreen(hwnd);

                WINDOW_HWND.set(hwnd.0 as isize).ok();
                set_clickthrough(hwnd, true);

                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = UpdateWindow(hwnd);

                start_clickthrough_loop(hwnd, state.clone(), config.clone(), get_collapsed);
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

unsafe fn position_fullscreen(hwnd: HWND) {
    unsafe {
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);

        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            screen_width,
            screen_height,
            SWP_NOACTIVATE,
        )
        .ok();
    }
}

unsafe fn start_clickthrough_loop(
    hwnd: HWND,
    state: Arc<Mutex<IslandState>>,
    config: ConfigHandle,
    get_collapsed: impl Fn() -> bool + Send + 'static,
) {
    let timer = Box::leak(Box::new(slint::Timer::default()));

    let mut clickthrough_enabled = true;
    let mut hidden_for_fullscreen = false;
    let mut topmost_counter: u32 = 0;

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
        }

        topmost_counter += 1;
        if topmost_counter >= TOPMOST_REASSERT_INTERVAL {
            topmost_counter = 0;
            unsafe {
                SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
                .ok();
            }
        }

        let (mx, my) = cursor_position();

        let mut rect = RECT::default();

        unsafe {
            GetWindowRect(hwnd, &mut rect).ok();
        }

        let island_config = config.island();

        let (logical, has_active) = {
            let state = state.lock().unwrap();
            (
                state.bounds(&island_config),
                state.mic || state.camera || state.content != ContentState::Idle,
            )
        };
        let collapsed = get_collapsed();

        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let scale_factor = dpi as f64 / 96.0;
        let bounds = logical.physical(scale_factor);

        let window_width = rect.right - rect.left;

        let island_x = (window_width - bounds.width) / 2;

        let island_left = rect.left + island_x;
        let island_top = rect.top
            + if collapsed {
                ((-(logical.height - 10)) as f64 * scale_factor).round() as i32
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
