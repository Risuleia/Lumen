pub struct VisualEnvelope {
    value: f32
}

impl VisualEnvelope {
    pub fn new() -> Self {
        Self { value: 0.0 }
    }

    pub fn push(&mut self, amp: f32) -> f32 {
        let attack = 0.35;
        let release = 0.15;

        if amp > self.value {
            self.value = self.value + (amp - self.value) * attack;
        } else {
            self.value = self.value + (amp - self.value) * release
        }

        self.value
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let mut sum = 0.0;
    for &s in samples {
        sum += s * s;
    }

    (sum / samples.len() as f32).sqrt()
        .clamp(0.0, 1.0)
}