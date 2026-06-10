#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]


use anyhow::{Result, anyhow};
use single_instance::SingleInstance;

use crate::{
    app::Lumen,
    geometry::{SHELL_HEIGHT, SHELL_WIDTH},
    platform::initialize_window,
};

mod app;
mod geometry;
mod platform;
mod state;
mod sync;

slint::include_modules!();

fn main() -> Result<()> {
    let instance = SingleInstance::new("io.risuleia.lumen").unwrap();
    if !instance.is_single() {
        return Err(anyhow!("One instance of Lumen is already running."));
    }

    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new().unwrap()))
        .unwrap();

    let mut app = Lumen::new();

    let state = app.state().clone();

    let shell = Shell::new().unwrap();

    initialize_window(&shell, SHELL_WIDTH, SHELL_HEIGHT, state.clone());

    app.start(&shell)?;

    Ok(())
}
