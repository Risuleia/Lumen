use anyhow::Result;
use winit::window::Window;

use crate::{compositor::RegionParams, motion::MotionDriver};

mod capture;
mod compositor;
mod gpu;
mod motion;
mod pipelines;
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

    pub blur: pipelines::BlurPipeline,
    pub refraction: pipelines::RefractionPipeline,
    pub glow: pipelines::GlowPipeline,
    pub shadow: pipelines::ShadowPipeline,

    pub motion: motion::MotionDriver,
}

impl<'w> LiquidGlassEngine<'w> {
    pub async fn new(config: LiquidGlassConfig, window: &'w Window) -> Result<Self> {
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

        let blur = pipelines::BlurPipeline::new(&gpu.device, &gpu.queue, size)?;
        let refraction = pipelines::RefractionPipeline::new(&gpu.device, &gpu.queue, size)?;
        let glow = pipelines::GlowPipeline::new(&gpu.device, &gpu.queue, size)?;
        let shadow = pipelines::ShadowPipeline::new(&gpu.device, &gpu.queue, size)?;

        let motion = MotionDriver::new();

        Ok(Self {
            gpu,
            capture,
            compositor,
            blur,
            refraction,
            glow,
            shadow,
            motion,
        })
    }

    pub fn tick(&mut self) {
        self.motion.update();

        let tex_opt = self.capture.latest_frame.lock().unwrap().take();

        if let Some(frame) = tex_opt {
            let view = self.capture.to_wgpu_view(&self.gpu.device, &frame);

            // let blurred = self.blur.run(&view).unwrap();
            let refracted = self.refraction.run(&view).unwrap();
            let glow = self.glow.run().unwrap();
            let shadow = self.shadow.run().unwrap();

            let m = &self.motion.island;

            let sz = self.capture.item.Size().unwrap();

            // Get window position + size from winit
            let inner = self.compositor.window.inner_size();
            let pos = self.compositor.window.outer_position().unwrap();

            let scale_factor = self.compositor.window.scale_factor() as f32;

            self.compositor.set_region(RegionParams {
                window_pos: [(pos.x as f32), (pos.y as f32)],
                window_size: [
                    (inner.width as f32),
                    (inner.height as f32),
                ],
                capture_size: [sz.Width as f32, sz.Height as f32],
                _pad: [0.0, 0.0],
            });

            self.compositor
                .draw(
                    shadow,
                    glow,
                    refracted,
                    m.scale.value,
                    m.radius.value,
                    m.glow.value,
                    m.shadow.value,
                )
                .unwrap();
        }
    }
}
