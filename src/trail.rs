//! Trail of the workspace disc while it slides to another slot, drawn with a wgpu shader.
//! Without wgpu (iced's software fallback) nothing is drawn and the disc slides alone.

use std::borrow::Cow;

use iced::widget::shader::{self, Viewport};
use iced::{Color, Rectangle, mouse, wgpu};

// Half the streak's thickness at the tail and at the head (the disc is 18 px across).
const THIN: f32 = 2.0;
const THICK: f32 = 8.0;
const GLOW: f32 = 4.0;

/// One frame of the trail; positions are x centers in the widget, in logical pixels.
pub struct Trail {
    pub tail: f32,
    pub head: f32,
    pub fade: f32,
    pub color: Color,
}

impl<Message> shader::Program<Message> for Trail {
    type State = ();
    type Primitive = Primitive;

    fn draw(&self, _: &(), _: mouse::Cursor, bounds: Rectangle) -> Primitive {
        Primitive {
            uniforms: [
                bounds.width,
                bounds.height,
                self.tail,
                self.head,
                self.fade,
                THIN,
                THICK,
                GLOW,
                self.color.r,
                self.color.g,
                self.color.b,
                self.color.a,
            ],
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    // Matches `Uniforms` in trail.wgsl: eight f32 and a vec4, 48 bytes.
    uniforms: [f32; 12],
}

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(&self, pipeline: &mut Pipeline, _: &wgpu::Device, queue: &wgpu::Queue, _: &Rectangle, _: &Viewport) {
        let bytes: Vec<u8> = self.uniforms.iter().flat_map(|value| value.to_ne_bytes()).collect();
        queue.write_buffer(&pipeline.uniforms, 0, &bytes);
    }

    fn draw(&self, pipeline: &Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        true
    }
}

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zehll trail"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("trail.wgsl"))),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zehll trail uniforms"),
            size: std::mem::size_of::<[f32; 12]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zehll trail layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zehll trail bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zehll trail pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zehll trail pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self { pipeline, uniforms, bind_group }
    }
}
