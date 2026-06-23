#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Result, anyhow};
use single_instance::SingleInstance;

use crate::{
    app::Lumen, platform::{initialize_tray, initialize_window}
};

mod app;
mod geometry;
mod platform;
mod state;
mod sync;
mod config;

slint::include_modules!();

pub const AUMID: &str = "io.risuleia.lumen";

fn main() -> Result<()> {
    let instance = SingleInstance::new(AUMID).unwrap();
    if !instance.is_single() {
        return Err(anyhow!("One instance of Lumen is already running."));
    }

    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new().unwrap()))
        .unwrap();

    let mut app = Lumen::new();

    let state = app.state().clone();
    let shell = Shell::new().unwrap();
    let weak = shell.as_weak();

    let (_tray, _tray_timer) = initialize_tray();

    initialize_window(
        &shell, 
        state.clone(), 
        app.config(), 
        move || weak
            .upgrade()
            .map(|s| s.global::<IslandData>().get_collapsed())
            .unwrap_or(false)
    );

    app.start(&shell)?;

    Ok(())
}
