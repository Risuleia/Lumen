use std::time::Instant;

use crate::motion::island::IslandMotionState;

pub struct MotionDriver {
    pub island: IslandMotionState,
    last_time: Instant
}

impl MotionDriver {
    pub fn new() -> Self {
        Self {
            island: IslandMotionState::new(),
            last_time: Instant::now()
        }
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_time).as_secs_f32();
        self.last_time = now;

        let dt = dt.clamp(0.0001, 0.033);

        self.island.update(dt);
    }
}