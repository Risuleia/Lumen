use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};

pub fn cursor_position() -> (i32, i32) {
    let mut point = POINT::default();

    unsafe {
        GetCursorPos(&mut point).ok();
    }

    (point.x, point.y)
}

pub fn point_inside_pill(
    px: i32,
    py: i32,
    width: i32,
    height: i32,
    radius: i32
) -> bool {
    if px < 0 || py < 0 {
        return false;
    }
    if px > width || py > height {
        return false;
    }

    if px >= radius && px < width - radius {
        return true;
    }
    if py < height - radius {
        return true;
    }

    // bottom left
    let dx = px - radius;
    let dy = py - (height - radius - 1);

    if dx * dx + dy * dy <= radius * radius {
        return true;
    }

    // bottom right
    let dx = px - (width - radius - 1);
    let dy = py - (height - radius - 1);

    dx * dx + dy * dy <= radius * radius
}