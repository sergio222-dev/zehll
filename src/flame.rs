//! Flame burst around the media button when playback starts or stops, drawn with a wgpu shader.
//! Without wgpu (iced's software fallback) nothing is drawn and the button looks as before.

use std::borrow::Cow;
use std::time::Duration;

use iced::widget::shader::{self, Viewport};
use iced::{Rectangle, mouse, wgpu};

/// How long one burst lasts.
pub const DURATION: Duration = Duration::from_millis(700);

/// A burst `elapsed` into its run; `playing` is the state playback changed to.
pub struct Flare {
    pub elapsed: Duration,
    pub playing: bool,
    /// Half the size of the button the flames surround.
    pub half_extent: f32,
}

impl<Message> shader::Program<Message> for Flare {
    type State = ();
    type Primitive = Primitive;

    fn draw(&self, _: &(), _: mouse::Cursor, bounds: Rectangle) -> Primitive {
        let reach = (bounds.width.min(bounds.height) / 2.0 - self.half_extent).max(0.0);
        Primitive {
            uniforms: [
                bounds.width,
                bounds.height,
                self.half_extent,
                self.half_extent,
                progress(self.elapsed),
                self.elapsed.as_secs_f32(),
                if self.playing { 1.0 } else { 0.0 },
                reach,
            ],
        }
    }
}

/// 0 to 1 over `DURATION`.
pub fn progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

#[derive(Debug)]
pub struct Primitive {
    // Matches `Uniforms` in flame.wgsl: eight f32, 32 bytes.
    uniforms: [f32; 8],
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
            label: Some("zehll flame"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("flame.wgsl"))),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zehll flame uniforms"),
            size: std::mem::size_of::<[f32; 8]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zehll flame layout"),
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
            label: Some("zehll flame bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zehll flame pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zehll flame pipeline"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_runs_from_zero_to_one_over_the_burst() {
        assert_eq!(progress(Duration::ZERO), 0.0);
        assert!((progress(DURATION / 2) - 0.5).abs() < 1e-6);
        assert_eq!(progress(DURATION * 3), 1.0);
    }
}
