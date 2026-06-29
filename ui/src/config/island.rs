use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IslandConfig {
    pub scale: f64,
    pub y_offset: u64,
    pub shadows: bool,
    pub completely_hidden: bool
}

impl IslandConfig {
    pub fn sanitize(&mut self) {
        self.scale = self.scale.clamp(0.5, 1.5);
        self.y_offset = self.y_offset.clamp(2, 20);
    }
}

impl Default for IslandConfig {
    fn default() -> Self {
        Self { 
            scale: 1.0, 
            y_offset: 8, 
            shadows: true,
            completely_hidden: false
        }
    }
}