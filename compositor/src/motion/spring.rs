#[derive(Clone, Copy)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            stiffness: 280.0,
            damping: 28.0
        }
    }
}

impl Spring {
    pub fn step(
        &self,
        value: &mut f32,
        velocity: &mut f32,
        target: f32,
        dt: f32
    ) {
        let x = *value - target;
        let acceleration = -self.stiffness * x - self.damping * *velocity;
        *velocity += acceleration * dt;
        *value += *velocity * dt
    }
}