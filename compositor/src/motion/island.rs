use crate::motion::value::SpringValue;

pub struct IslandMotionState {
    pub scale: SpringValue,
    pub radius: SpringValue,
    pub glow: SpringValue,
    pub shadow: SpringValue
}

impl IslandMotionState {
    pub fn new() -> Self {
        Self {
            scale: SpringValue::new(1.0),
            radius: SpringValue::new(1.0),
            glow: SpringValue::new(0.6),
            shadow: SpringValue::new(0.5)
        }
    }

    pub fn set_expanded(&mut self) {
        self.scale.set(1.25);
        self.radius.set(0.75);
        self.glow.set(1.0);
        self.shadow.set(0.9);
    }

    pub fn set_idle(&mut self) {
        self.scale.set(1.0);
        self.radius.set(1.0);
        self.glow.set(0.6);
        self.shadow.set(0.5);
    }

    pub fn update(&mut self, dt: f32) {
        self.scale.update(dt);
        self.radius.update(dt);
        self.glow.update(dt);
        self.shadow.update(dt);
    }
}