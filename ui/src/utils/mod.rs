use windows::Win32::{
    Foundation::{COLORREF, HWND},
    Graphics::Gdi::{
        CreateRoundRectRgn, SetWindowRgn,
    },
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowLongW,
        WDA_EXCLUDEFROMCAPTURE, WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME,
        WS_EX_LAYERED, WS_EX_STATICEDGE, WS_THICKFRAME,
    },
};

pub unsafe fn setup_window_style(hwnd: HWND, width: i32, height: i32) {
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