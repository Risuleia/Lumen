use std::{path::PathBuf, sync::{Arc, RwLock}};

use lumen_core::cache_dir;
use serde::{Deserialize, Serialize};

mod notification;
mod island;

pub use notification::{NotificationConfig, ToastSuppression};
pub use island::IslandConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    island: IslandConfig,
    notifications: NotificationConfig,
}

fn config_path() -> PathBuf {
    cache_dir().join("config.toml")
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        std::fs::create_dir_all(cache_dir()).ok();

        let Ok(contents) = std::fs::read_to_string(&path) else {
            let default = Config::default();
            default.write_default(&path);
            return default;
        };

        match toml::from_str::<Config>(&contents) {
            Ok(config) => config.sanitize(),
            Err(e) => {
                eprintln!("[Config] Failed to parse config.toml: {e}. Using defaults...");
                Config::default()
            }
        }
    }

    fn write_default(&self, path: &PathBuf) {
        std::fs::create_dir_all(cache_dir()).ok();
        if let Ok(toml_str) = toml::to_string_pretty(self) {
            std::fs::write(path, toml_str).ok();
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
    fn set(&self, config: Config) {
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