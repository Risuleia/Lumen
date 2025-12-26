use anyhow::Result;
use wgpu::{Adapter, Device, DeviceDescriptor, ExperimentalFeatures, Features, Instance, InstanceDescriptor, Limits, Queue, RequestAdapterOptions};

pub struct GpuState {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue
}

impl GpuState {
    pub async fn new() -> Result<Self> {
        let instance = Instance::new(&InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("liquid-glass-device"),
                required_features: Features::empty(),
                required_limits: Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                experimental_features: ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off
            },
        )
        .await?;

        Ok(Self { instance, adapter, device, queue })
    }
}