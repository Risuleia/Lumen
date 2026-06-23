#[derive(Debug, Clone, Copy)]
pub struct IslandBounds {
    pub y: i32,

    pub width: i32,
    pub height: i32,

    pub radius: i32,
}

impl IslandBounds {
    pub fn physical(self, scale_factor: f64) -> PhysicalBounds {
        PhysicalBounds {
            width: (self.width as f64 * scale_factor).round() as i32,
            height: (self.height as f64 * scale_factor).round() as i32,
            radius: (self.radius as f64 * scale_factor).round() as i32,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PhysicalBounds {
    pub width: i32,
    pub height: i32,

    pub radius: i32,
}
