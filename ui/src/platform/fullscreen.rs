use std::{cell::RefCell, collections::HashMap};

use windows::Win32::{
    Foundation::{CloseHandle, HWND, RECT},
    Graphics::{
        Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    },
    System::{
        ProcessStatus::GetProcessImageFileNameW,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
    UI::WindowsAndMessaging::{
        GWL_STYLE, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, IsZoomed, RealGetWindowClassW, WS_CAPTION, WS_POPUP,
    },
};

struct OwnedHandle(windows::Win32::Foundation::HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct CachedWindow {
    should_exclude: bool,
}

thread_local! {
    static WINDOW_EXCLUSION_CACHE: RefCell<HashMap<usize, CachedWindow>> = RefCell::new(HashMap::with_capacity(16));
}

pub fn is_foreground_fullscreen(app_hwnd: HWND) -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null()
            || hwnd == app_hwnd
            || IsIconic(hwnd).as_bool()
            || !IsWindowVisible(hwnd).as_bool()
        {
            return false;
        }

        let hwnd_raw = hwnd.0 as usize;

        let is_cached_exclusion = WINDOW_EXCLUSION_CACHE
            .with(|cache| cache.borrow().get(&hwnd_raw).map(|w| w.should_exclude));

        if let Some(true) = is_cached_exclusion {
            return false;
        }

        let mut cloaked: u32 = 0;
        let dwm_res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if dwm_res.is_ok() && cloaked != 0 {
            return false;
        }

        if is_cached_exclusion.is_none() {
            let mut should_exclude = false;

            let mut process_id: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
            if process_id != 0 {
                if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
                    let _handle = OwnedHandle(h);
                    let mut image_name = [0u16; 512];
                    let len = GetProcessImageFileNameW(h, &mut image_name);
                    if len > 0 {
                        let path_str =
                            String::from_utf16_lossy(&image_name[..len as usize]).to_lowercase();
                        if path_str.contains("startmenuexperiencehost.exe")
                            || path_str.contains("shellexperiencehost.exe")
                        {
                            should_exclude = true;
                        }
                    }
                }
            }

            if !should_exclude {
                let mut class_name = [0u16; 256];
                let len = RealGetWindowClassW(hwnd, &mut class_name);
                if len > 0 {
                    let class_str = String::from_utf16_lossy(&class_name[..len as usize]);
                    if class_str == "Progman"
                        || class_str == "WorkerW"
                        || class_str == "Shell_TrayWnd"
                        || class_str == "ForegroundStaging"
                        || class_str == "XAMLContextMenu"
                        || class_str == "Windows.Internal.Shell.Experience.ContextMenu.WindowClass"
                        || class_str == "#32771"
                    {
                        should_exclude = true;
                    }
                }
            }

            WINDOW_EXCLUSION_CACHE.with(|cache| {
                let mut cache_mut = cache.borrow_mut();

                if cache_mut.len() > 64 {
                    cache_mut.clear();
                }

                cache_mut.insert(hwnd_raw, CachedWindow { should_exclude });
            });

            if should_exclude {
                return false;
            }
        }

        let window_style = GetWindowLongW(hwnd, GWL_STYLE) as u32;

        if IsZoomed(hwnd).as_bool() {
            if (window_style & WS_CAPTION.0) == WS_CAPTION.0 {
                return false;
            }
        }

        let mut window_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_err() {
            return false;
        }

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);

        let mut mi =
            MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };

        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return false;
        }

        let monitor_rect = mi.rcMonitor;

        let fills_screen = window_rect.left <= monitor_rect.left
            && window_rect.top <= monitor_rect.top
            && window_rect.right >= monitor_rect.right
            && window_rect.bottom >= monitor_rect.bottom;

        if fills_screen {
            let is_popup = (window_style & WS_POPUP.0) == WS_POPUP.0;
            let has_caption = (window_style & WS_CAPTION.0) == WS_CAPTION.0;

            if is_popup || !has_caption {
                return true;
            }
        }

        false
    }
}
