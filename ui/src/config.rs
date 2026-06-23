use std::{path::PathBuf, sync::{Arc, RwLock}};

use lumen_core::cache_dir;
use notify::Watcher;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IslandConfig {
    pub scale: f64,
    pub y_offset: u64
}

impl Default for IslandConfig {
    fn default() -> Self {
        Self { scale: 1.0, y_offset: 8 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub timeout_ms: u64,
    pub suppress_native_toasts: bool
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { timeout_ms: 3000, suppress_native_toasts: false }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub island: IslandConfig,
    pub notifications: NotificationConfig
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

        match toml::from_str(&contents) {
            Ok(config) => config,
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
}

#[derive(Clone)]
pub struct ConfigHandle {
    inner: Arc<RwLock<Config>>
}

impl ConfigHandle {
    pub fn get(&self) -> Config {
        self.inner.read().unwrap().clone()
    }

    fn set(&self, config: Config) {
        *self.inner.write().unwrap() = config;
    }
}

pub fn init_config() -> ConfigHandle {
    let initial = Config::load();
    let handle = ConfigHandle {
        inner: Arc::new(RwLock::new(initial))
    };

    start_watcher(handle.clone());

    handle
}

fn start_watcher(handle: ConfigHandle) {
    std::thread::spawn(move || {
        let path = config_path();

        let Some(parent) = path.parent().map(|p| p.to_path_buf()) else {
            return;
        };

        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[Config] Failed to create watcher: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&parent, notify::RecursiveMode::NonRecursive) {
            eprintln!("[Config] Failed to watch config directory: {e}");
            return;
        }

        for res in rx {
            match res {
                Ok(ev) => {
                    if ev.paths.iter().any(|p| p == &path) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        let new_config = Config::load();
                        handle.set(new_config);
                    }
                }
                Err(e) => eprintln!("[Config] Watch error: {e}")
            }
        }
    });
}