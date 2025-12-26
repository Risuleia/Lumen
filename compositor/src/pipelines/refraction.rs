use anyhow::Result;
use wgpu::util::DeviceExt;
use wgpu::*;

pub struct RefractionPipeline {
    device: Device,
    queue: Queue,
    sampler: Sampler,
    pipeline: RenderPipeline,
    quad: Buffer,

    output: Texture,
    pub output_view: TextureView,
}

impl RefractionPipeline {
    pub fn new(device: &Device, queue: &Queue, size: (u32, u32)) -> Result<Self> {
        let device = device.clone();
        let queue = queue.clone();

        let shader = device.create_shader_module(include_wgsl!("../../shaders/refraction.wgsl"));

        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("refraction_bind_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("refraction_pipeline_layout"),
            bind_group_layouts: &[&bind_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("refraction_pipeline"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip, // ⭐ ADD THIS
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let output = device.create_texture(&TextureDescriptor {
            label: Some("refraction-output"),
            size: Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let output_view = output.create_view(&Default::default());

        let quad_data: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

        let quad = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("refraction-quad"),
            contents: bytemuck::cast_slice(&quad_data),
            usage: BufferUsages::VERTEX,
        });

        Ok(Self {
            device,
            queue,
            sampler,
            pipeline,
            quad,
            output,
            output_view,
        })
    }

    pub fn run(&self, blurred: &TextureView) -> Result<&TextureView> {
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("refraction-encoder"),
            });

        let bind = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("refraction_bind"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(blurred),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("refraction_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.output_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.quad.slice(..));
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..4, 0..1);
        drop(pass);

        self.queue.submit(Some(encoder.finish()));

        Ok(&self.output_view)
    }
}
