use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use lumen_core::config_dir;
use serde::{Deserialize, Serialize};

mod island;
mod notification;

pub use island::IslandConfig;
pub use notification::{NotificationConfig, ToastSuppression};

const DEFAULT_CONFIG: &str = include_str!("../../../assets/default_config.toml");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    island: IslandConfig,
    notifications: NotificationConfig,
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        std::fs::create_dir_all(config_dir()).ok();

        let Ok(contents) = std::fs::read_to_string(&path) else {
            std::fs::write(&path, DEFAULT_CONFIG).ok();
            return Config::default();
        };

        match toml::from_str::<Config>(&contents) {
            Ok(config) => config.sanitize(),
            Err(e) => {
                eprintln!("[Config] Failed to parse config.toml: {e}. Using defaults...");
                Config::default()
            }
        }
    }

    fn sanitize(mut self) -> Self {
        self.island.sanitize();
        self.notifications.sanitize();

        self
    }
}

#[derive(Clone)]
pub struct ConfigHandle {
    inner: Arc<RwLock<Config>>,
}

impl ConfigHandle {
    fn _set(&self, config: Config) {
        *self.inner.write().unwrap() = config;
    }

    pub fn island(&self) -> IslandConfig {
        self.inner.read().unwrap().island.clone()
    }

    pub fn notifications(&self) -> NotificationConfig {
        self.inner.read().unwrap().notifications.clone()
    }
}

pub fn init_config() -> ConfigHandle {
    let initial = Config::load();
    let handle = ConfigHandle { inner: Arc::new(RwLock::new(initial)) };

    handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_matches_defaults() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(toml::to_string(&parsed).unwrap(), toml::to_string(&Config::default()).unwrap());
    }
}
