use anyhow::Result;
use wgpu::{
    Adapter, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferDescriptor,
    BufferUsages, Color, ColorTargetState, ColorWrites, CommandEncoderDescriptor, Device,
    FragmentState, LoadOp, MultisampleState, Operations, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    ShaderStages, Surface, SurfaceConfiguration, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension, VertexBufferLayout, VertexState, VertexStepMode,
    include_wgsl, util::DeviceExt, vertex_attr_array, wgt::SamplerDescriptor,
};
use winit::window::Window;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IslandParams {
    pub scale: f32,
    pub radius: f32,
    pub glow_power: f32,
    pub shadow_power: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RegionParams {
    pub window_pos: [f32; 2],
    pub window_size: [f32; 2],
    pub capture_size: [f32; 2],
    pub _pad: [f32; 2],
}

pub struct Compositor<'w> {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'w>,
    pub config: SurfaceConfiguration,

    pub sampler: Sampler,
    pub pipeline: RenderPipeline,

    pub vertex_buffer: Buffer,
    pub param_buffer: Buffer,
    pub region_buffer: Buffer,
    pub bind_layout: BindGroupLayout,

    pub window: &'w Window
}

impl<'w> Compositor<'w> {
    pub fn new(
        surface: Surface<'w>,
        adapter: Adapter,
        device: &Device,
        queue: &Queue,
        window: &'w Window,
        size: (u32, u32),
    ) -> Result<Self> {
        let device = device.clone();
        let queue = queue.clone();

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_capabilities(&adapter).formats[0],
            width: size.0,
            height: size.1,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };

        surface.configure(&device, &config);

        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let vertices: [f32; 16] = [
            -1.0, -1.0, 0.0, 1.0,
             1.0, -1.0, 1.0, 1.0,
            -1.0,  1.0, 0.0, 0.0,
             1.0,  1.0, 1.0, 0.0,
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad V8"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });

        let bind_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("compositor_bind_layout"),
            entries: &[
                // refracted
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // glow
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // shadow
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // sampler
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // Island uniform
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Region uniform
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(include_wgsl!("../shaders/liquid_mask.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("compositor_layout"),
            bind_group_layouts: &[&bind_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("compositor_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[VertexBufferLayout {
                    array_stride: 4 * 4,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                }],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let param_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Island Params"),
            size: std::mem::size_of::<IslandParams>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let region_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Region Params"),
            size: std::mem::size_of::<RegionParams>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            sampler,
            pipeline,
            vertex_buffer,
            param_buffer,
            region_buffer,
            bind_layout,
            window
        })
    }

    pub fn set_region(&self, params: RegionParams) {
        self.queue.write_buffer(
            &self.region_buffer,
            0,
            bytemuck::bytes_of(&params),
        );
    }

    pub fn draw(
        &self,
        shadow: &TextureView,
        glow: &TextureView,
        refracted: &TextureView,
        scale: f32,
        radius: f32,
        glow_power: f32,
        shadow_power: f32,
    ) -> Result<()> {
        let params = IslandParams {
            scale,
            radius,
            glow_power,
            shadow_power,
        };

        self.queue.write_buffer(
            &self.param_buffer,
            0,
            bytemuck::bytes_of(&params),
        );

        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&TextureViewDescriptor::default());

        let bind = self.device.create_bind_group(&BindGroupDescriptor {
            layout: &self.bind_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(refracted)},
                BindGroupEntry { binding: 1, resource: BindingResource::TextureView(glow)},
                BindGroupEntry { binding: 2, resource: BindingResource::TextureView(shadow)},
                BindGroupEntry { binding: 3, resource: BindingResource::Sampler(&self.sampler)},
                BindGroupEntry { binding: 4, resource: self.param_buffer.as_entire_binding()},
                BindGroupEntry { binding: 5, resource: self.region_buffer.as_entire_binding()},
            ],
            label: None,
        });

        let mut encoder = self.device.create_command_encoder(
            &CommandEncoderDescriptor { label: Some("compositor_encoder") }
        );

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("compositor_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..4, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();

        Ok(())
    }
}