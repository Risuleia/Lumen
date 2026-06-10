use std::path::PathBuf;

pub mod artwork;
pub mod icon;

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap().join("Lumen")
}