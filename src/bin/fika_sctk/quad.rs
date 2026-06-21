use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use fika_core::{ViewPoint, ViewRect};

const QUAD_SHADER: &str = r#"
struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

pub(crate) struct QuadBatch {
    vertices: Vec<QuadVertex>,
    scale: f32,
}

impl Default for QuadBatch {
    fn default() -> Self {
        Self::with_scale(1.0)
    }
}

impl QuadBatch {
    pub(crate) fn with_scale(scale: f32) -> Self {
        Self {
            vertices: Vec::new(),
            scale: scale.max(1.0),
        }
    }

    pub(crate) fn vertices(&self) -> &[QuadVertex] {
        &self.vertices
    }

    pub(crate) fn len(&self) -> usize {
        self.vertices.len() / 6
    }

    pub(crate) fn push_rect(
        &mut self,
        rect: ViewRect,
        color: [f32; 4],
        surface_width: u32,
        surface_height: u32,
    ) {
        let surface_width = scaled_extent(surface_width, self.scale);
        let surface_height = scaled_extent(surface_height, self.scale);
        push_rect(
            &mut self.vertices,
            scale_rect(rect, self.scale),
            color,
            surface_width,
            surface_height,
        );
    }

    pub(crate) fn push_clipped_rect(
        &mut self,
        rect: ViewRect,
        clip: ViewRect,
        color: [f32; 4],
        surface_width: u32,
        surface_height: u32,
    ) {
        let surface_width = scaled_extent(surface_width, self.scale);
        let surface_height = scaled_extent(surface_height, self.scale);
        push_clipped_rect_unscaled(
            &mut self.vertices,
            scale_rect(rect, self.scale),
            scale_rect(clip, self.scale),
            color,
            surface_width,
            surface_height,
        );
    }

    pub(crate) fn push_clipped_rounded_rect(
        &mut self,
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        color: [f32; 4],
        surface_width: u32,
        surface_height: u32,
    ) {
        let surface_width = scaled_extent(surface_width, self.scale);
        let surface_height = scaled_extent(surface_height, self.scale);
        let rect = scale_rect(rect, self.scale);
        let clip = scale_rect(clip, self.scale);
        let radius = radius * self.scale;
        if rect.width <= 0.0 || rect.height <= 0.0 || color[3] <= 0.0 {
            return;
        }
        let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
        if radius <= 1.0 {
            push_clipped_rect_unscaled(
                &mut self.vertices,
                rect,
                clip,
                color,
                surface_width,
                surface_height,
            );
            return;
        }

        let middle_height = (rect.height - radius * 2.0).max(0.0);
        if middle_height > 0.0 {
            push_clipped_rect_unscaled(
                &mut self.vertices,
                ViewRect {
                    x: rect.x,
                    y: rect.y + radius,
                    width: rect.width,
                    height: middle_height,
                },
                clip,
                color,
                surface_width,
                surface_height,
            );
        }

        let steps = radius.ceil().clamp(4.0, 16.0) as usize;
        let step_height = radius / steps as f32;
        for step in 0..steps {
            let y = rect.y + step as f32 * step_height;
            let midpoint_y = y + step_height / 2.0;
            let dy = rect.y + radius - midpoint_y;
            let inset = radius - (radius * radius - dy * dy).max(0.0).sqrt();
            let strip_width = rect.width - inset * 2.0;
            if strip_width <= 0.0 {
                continue;
            }
            let top = ViewRect {
                x: rect.x + inset,
                y,
                width: strip_width,
                height: step_height,
            };
            let bottom = ViewRect {
                x: rect.x + inset,
                y: rect.bottom() - (step + 1) as f32 * step_height,
                width: strip_width,
                height: step_height,
            };
            push_clipped_rect_unscaled(
                &mut self.vertices,
                top,
                clip,
                color,
                surface_width,
                surface_height,
            );
            push_clipped_rect_unscaled(
                &mut self.vertices,
                bottom,
                clip,
                color,
                surface_width,
                surface_height,
            );
        }
    }
}

pub(crate) struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
}

impl QuadRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-sctk-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-sctk-quad-layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-sctk-quad-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[QuadVertex::layout()],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_capacity = 6;
        let vertex_buffer = create_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
        }
    }

    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[QuadVertex],
    ) {
        if vertices.len() > self.vertex_capacity {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = create_vertex_buffer(device, self.vertex_capacity);
        }
        self.vertex_count = vertices.len();
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    pub(crate) fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct QuadVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl QuadVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

fn create_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-sctk-quad-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<QuadVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn push_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    color: [f32; 4],
    surface_width: u32,
    surface_height: u32,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let width = surface_width.max(1) as f32;
    let height = surface_height.max(1) as f32;
    let left = rect.x / width * 2.0 - 1.0;
    let right = rect.right() / width * 2.0 - 1.0;
    let top = 1.0 - rect.y / height * 2.0;
    let bottom = 1.0 - rect.bottom() / height * 2.0;

    vertices.extend_from_slice(&[
        QuadVertex {
            position: [left, top],
            color,
        },
        QuadVertex {
            position: [left, bottom],
            color,
        },
        QuadVertex {
            position: [right, bottom],
            color,
        },
        QuadVertex {
            position: [left, top],
            color,
        },
        QuadVertex {
            position: [right, bottom],
            color,
        },
        QuadVertex {
            position: [right, top],
            color,
        },
    ]);
}

fn intersect_rect(a: ViewRect, b: ViewRect) -> Option<ViewRect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    let width = right - x;
    let height = bottom - y;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x,
        y,
        width,
        height,
    })
}

fn push_clipped_rect_unscaled(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    color: [f32; 4],
    surface_width: u32,
    surface_height: u32,
) {
    if let Some(rect) = intersect_rect(rect, clip) {
        push_rect(vertices, rect, color, surface_width, surface_height);
    }
}

fn scale_rect(rect: ViewRect, scale: f32) -> ViewRect {
    ViewRect {
        x: rect.x * scale,
        y: rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

fn scaled_extent(value: u32, scale: f32) -> u32 {
    ((value.max(1) as f32) * scale.max(1.0)).ceil().max(1.0) as u32
}

pub(crate) fn inset(rect: ViewRect, by: f32) -> ViewRect {
    ViewRect {
        x: rect.x + by,
        y: rect.y + by,
        width: (rect.width - by * 2.0).max(0.0),
        height: (rect.height - by * 2.0).max(0.0),
    }
}

pub(crate) fn point(x: f64, y: f64) -> ViewPoint {
    ViewPoint {
        x: x as f32,
        y: y as f32,
    }
}
