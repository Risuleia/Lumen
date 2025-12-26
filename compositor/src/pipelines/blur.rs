use std::num::NonZero;

use anyhow::Result;
use wgpu::{util::DeviceExt, *};

pub struct BlurPipeline {
    device: Device,
    queue: Queue,
    sampler: Sampler,

    down_pipeline: RenderPipeline,
    up_pipeline: RenderPipeline,

    quad_vb: Buffer,

    targets_a: Vec<Texture>,
    views_a: Vec<TextureView>,

    targets_b: Vec<Texture>,
    views_b: Vec<TextureView>,

    output: Texture,
    output_view: TextureView,
}

impl BlurPipeline {
    pub fn new(device: &Device, queue: &Queue, size: (u32, u32)) -> Result<Self> {
        let device = device.clone();
        let queue = queue.clone();

        let shader = device.create_shader_module(include_wgsl!("../../shaders/blur.wgsl"));

        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("blur_bind_layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("blur_pipeline"),
            bind_group_layouts: &[&bind_layout],
            immediate_size: 0,
        });

        let make_pipeline = |entry: &str| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
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
            })
        };

        let down_pipeline = make_pipeline("downsample");
        let up_pipeline = make_pipeline("upsample");

        let mut targets_a = Vec::new();
        let mut views_a = Vec::new();

        let mut targets_b = Vec::new();
        let mut views_b = Vec::new();

        let mut w = size.0;
        let mut h = size.1;

        for _ in 0..2 {
            w = (w / 2).max(1);
            h = (h / 2).max(1);

            let make_tex = |label| {
                device.create_texture(&TextureDescriptor {
                    label: Some(label),
                    size: Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Bgra8Unorm,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };

            let a = make_tex("blur_a");
            let b = make_tex("blur_b");

            views_a.push(a.create_view(&Default::default()));
            views_b.push(b.create_view(&Default::default()));

            targets_a.push(a);
            targets_b.push(b);
        }

        let quad_data: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

        let quad_vb = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("blur-quad"),
            contents: bytemuck::cast_slice(&quad_data),
            usage: BufferUsages::VERTEX,
        });

        let output = device.create_texture(&TextureDescriptor {
            label: Some("blur-output"),
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

        Ok(Self {
            device,
            queue,
            sampler,
            down_pipeline,
            up_pipeline,
            quad_vb,
            targets_a,
            views_a,
            targets_b,
            views_b,
            output,
            output_view,
        })
    }

    pub fn run(&self, input: &TextureView) -> Result<&TextureView> {
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("blur-encoder"),
            });

        let mut last = input;

        for view in &self.views_a {
            self.pass(&mut encoder, last, view, &self.down_pipeline);
            last = view;
        }

        let mut up_src = self.views_a.last().unwrap();

        for i in (0..self.views_b.len() - 1).rev() {
            let dst = &self.views_b[i];
            self.pass(&mut encoder, &up_src, dst, &self.up_pipeline);
            up_src = &dst;
        }

        self.pass(&mut encoder, &up_src, &self.output_view, &self.up_pipeline);

        self.queue.submit(Some(encoder.finish()));

        Ok(&self.output_view)
    }

    // pub fn run(&self, input: &TextureView) -> Result<&TextureView> {
    //     let mut encoder = self
    //         .device
    //         .create_command_encoder(&CommandEncoderDescriptor {
    //             label: Some("blur-encoder"),
    //         });

    //     // Just do ONE pass directly to output, skip all the downsampling/upsampling
    //     self.pass(&mut encoder, input, &self.output_view, &self.down_pipeline);

    //     self.queue.submit(Some(encoder.finish()));

    //     Ok(&self.output_view)
    // }

    fn pass(
        &self,
        encoder: &mut CommandEncoder,
        src: &TextureView,
        dst: &TextureView,
        pipeline: &RenderPipeline,
    ) {
        let bind = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(src),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: dst,
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

        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, self.quad_vb.slice(..));
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..4, 0..1);
    }
}
