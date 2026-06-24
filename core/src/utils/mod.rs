use std::path::PathBuf;

pub mod artwork;
pub mod icon;
pub mod name;
pub mod simd_audio;

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap().join("Lumen")
}
pub fn artwork_dir() -> PathBuf {
    cache_dir().join("artwork")
}
pub fn icons_dir() -> PathBuf {
    cache_dir().join("icons")
}
