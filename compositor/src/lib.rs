use anyhow::Result;
use winit::{dpi::{PhysicalPosition, PhysicalSize}, window::Window};

use crate::{compositor::RegionParams, motion::MotionDriver};

mod capture;
mod compositor;
mod gpu;
mod motion;
mod utils;

pub struct LiquidGlassConfig {
    pub blur_strength: f32,
    pub refraction: f32,
    pub intensity: f32,
}

impl Default for LiquidGlassConfig {
    fn default() -> Self {
        Self {
            blur_strength: 25.0,
            refraction: 0.12,
            intensity: 1.0,
        }
    }
}

pub struct LiquidGlassEngine<'w> {
    pub gpu: gpu::GpuState,
    pub capture: capture::CaptureState,

    pub compositor: compositor::Compositor<'w>,

    pub motion: motion::MotionDriver,
}

impl<'w> LiquidGlassEngine<'w> {
    pub async fn new(_config: LiquidGlassConfig, window: &'w Window) -> Result<Self> {
        let gpu = gpu::GpuState::new().await?;
        let capture = capture::CaptureState::new_primary_monitor()?;

        let size = {
            let sz = capture.item.Size()?;
            (sz.Width as u32, sz.Height as u32)
        };

        let surface = gpu.instance.create_surface(window)?;

        let compositor = compositor::Compositor::new(
            surface,
            gpu.adapter.clone(),
            &gpu.device,
            &gpu.queue,
            window,
            size,
        )?;

        let motion = MotionDriver::new();

        Ok(Self {
            gpu,
            capture,
            compositor,
            motion,
        })
    }

    pub fn tick(&mut self) {
        self.motion.update();

        let tex_opt = self.capture.latest_frame.lock().unwrap().take();

        if let Some(frame) = tex_opt {
            let view = self.capture.to_wgpu_view(&self.gpu.device, &frame);

            let m = &self.motion.island;

            let sz = self.capture.item.Size().unwrap();

            let inner = self.compositor.window.inner_size();
            let pos = self.compositor.window.outer_position().unwrap();

            self.compositor.set_region(RegionParams {
                window_pos: [pos.x as f32, pos.y as f32],
                window_size: [
                    inner.width as f32,
                    inner.height as f32,
                ],
                capture_size: [sz.Width as f32, sz.Height as f32],
                _pad: [0.0, 0.0],
            });

            self.compositor
                .draw(
                    &view,
                    m.radius.value,
                    0.02,
                    m.glow.value,
                    m.shadow.value,
                )
                .unwrap();
        }
    }

    pub fn set_island_visual(
        &mut self,
        width: u32,
        height: u32,
    ) {
        let window = &self.compositor.window;
        let pos = window.outer_position().unwrap();

        window.set_outer_position(PhysicalPosition::new(pos.x, pos.y));
        window.set_min_inner_size(Some(PhysicalSize::new(width, height)));
    }
}
