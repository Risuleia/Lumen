use crate::motion::spring::Spring;

pub struct SpringValue {
    pub value: f32,
    pub target: f32,
    pub velocity: f32,
    pub spring: Spring
}

impl SpringValue {
    pub fn new(v: f32) -> Self {
        Self {
            value: v,
            target: v,
            velocity: 0.0,
            spring: Spring::default()
        }
    }

    pub fn set(&mut self, t: f32) {
        self.target = t;
    }

    pub fn update(&mut self, dt: f32) {
        self.spring.step(&mut self.value, &mut self.velocity, self.target, dt);
    }
}