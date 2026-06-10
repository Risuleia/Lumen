use std::sync::OnceLock;

use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_TRANSPARENT}};

pub static WINDOW_HWND: OnceLock<isize> = OnceLock::new();

pub unsafe fn set_clickthrough(hwnd: HWND, enabled: bool) {
    unsafe {
        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    
        if enabled {
            ex_style |= WS_EX_TRANSPARENT.0 as isize;
        } else {
            ex_style &= !(WS_EX_TRANSPARENT.0 as isize);
        }
    
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
    }
}