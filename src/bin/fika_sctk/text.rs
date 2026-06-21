use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{
    Align, Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache,
    Wrap,
};
use fika_core::ViewRect;

const TEXT_ATLAS_WIDTH: u32 = 2048;
const TEXT_PADDING: u32 = 2;

const TEXT_SHADER: &str = r#"
struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var text_texture: texture_2d<f32>;
@group(0) @binding(1) var text_sampler: sampler;

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(text_texture, text_sampler, input.uv);
}
"#;

#[derive(Default)]
pub(crate) struct TextBatch {
    labels: Vec<TextLabel>,
}

impl TextBatch {
    pub(crate) fn push(
        &mut self,
        text: impl Into<String>,
        rect: ViewRect,
        clip: ViewRect,
        size: f32,
        line_height: f32,
        color: [u8; 4],
    ) {
        self.push_with_style(
            text,
            rect,
            clip,
            size,
            line_height,
            color,
            TextAlign::Start,
            TextWrap::WordOrGlyph,
        );
    }

    pub(crate) fn push_centered(
        &mut self,
        text: impl Into<String>,
        rect: ViewRect,
        clip: ViewRect,
        size: f32,
        line_height: f32,
        color: [u8; 4],
    ) {
        self.push_with_style(
            text,
            rect,
            clip,
            size,
            line_height,
            color,
            TextAlign::Center,
            TextWrap::WordOrGlyph,
        );
    }

    pub(crate) fn push_no_wrap(
        &mut self,
        text: impl Into<String>,
        rect: ViewRect,
        clip: ViewRect,
        size: f32,
        line_height: f32,
        color: [u8; 4],
    ) {
        self.push_with_style(
            text,
            rect,
            clip,
            size,
            line_height,
            color,
            TextAlign::Start,
            TextWrap::None,
        );
    }

    fn push_with_style(
        &mut self,
        text: impl Into<String>,
        rect: ViewRect,
        clip: ViewRect,
        size: f32,
        line_height: f32,
        color: [u8; 4],
        align: TextAlign,
        wrap: TextWrap,
    ) {
        let text = text.into();
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        self.labels.push(TextLabel {
            text,
            rect,
            clip,
            size,
            line_height,
            color,
            align,
            wrap,
        });
    }

    pub(crate) fn len(&self) -> usize {
        self.labels.len()
    }

    fn labels(&self) -> &[TextLabel] {
        &self.labels
    }
}

struct TextLabel {
    text: String,
    rect: ViewRect,
    clip: ViewRect,
    size: f32,
    line_height: f32,
    color: [u8; 4],
    align: TextAlign,
    wrap: TextWrap,
}

#[derive(Clone, Copy)]
enum TextAlign {
    Start,
    Center,
}

impl TextAlign {
    fn cosmic(self) -> Align {
        match self {
            Self::Start => Align::Left,
            Self::Center => Align::Center,
        }
    }
}

#[derive(Clone, Copy)]
enum TextWrap {
    None,
    WordOrGlyph,
}

impl TextWrap {
    fn cosmic(self) -> Wrap {
        match self {
            Self::None => Wrap::None,
            Self::WordOrGlyph => Wrap::WordOrGlyph,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TextFrameStats {
    pub(crate) labels: usize,
    pub(crate) quads: usize,
    pub(crate) atlas_width: u32,
    pub(crate) atlas_height: u32,
    pub(crate) atlas_bytes: usize,
}

struct TextFrame {
    vertices: Vec<TextVertex>,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    stats: TextFrameStats,
}

pub(crate) struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    texture_width: u32,
    texture_height: u32,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
    stats: TextFrameStats,
}

impl TextRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-sctk-text-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXT_SHADER)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-sctk-text-bind-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-sctk-text-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-sctk-text-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[TextVertex::layout()],
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-sctk-text-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let texture = create_text_texture(device, 1, 1);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = create_bind_group(device, &bind_group_layout, &texture_view, &sampler);
        let vertex_capacity = 6;
        let vertex_buffer = create_text_vertex_buffer(device, vertex_capacity);
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            texture_view,
            bind_group,
            texture_width: 1,
            texture_height: 1,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            stats: TextFrameStats::default(),
        }
    }

    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &TextBatch,
        surface_width: u32,
        surface_height: u32,
        scale: f32,
    ) {
        let frame = self.build_frame(batch, surface_width, surface_height, scale);
        if frame.width != self.texture_width || frame.height != self.texture_height {
            self.texture = create_text_texture(device, frame.width, frame.height);
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.bind_group = create_bind_group(
                device,
                &self.bind_group_layout,
                &self.texture_view,
                &self.sampler,
            );
            self.texture_width = frame.width;
            self.texture_height = frame.height;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
        if frame.vertices.len() > self.vertex_capacity {
            self.vertex_capacity = frame.vertices.len().next_power_of_two();
            self.vertex_buffer = create_text_vertex_buffer(device, self.vertex_capacity);
        }
        self.vertex_count = frame.vertices.len();
        if !frame.vertices.is_empty() {
            queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&frame.vertices),
            );
        }
        self.stats = frame.stats;
    }

    pub(crate) fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    pub(crate) fn stats(&self) -> TextFrameStats {
        self.stats
    }

    fn build_frame(
        &mut self,
        batch: &TextBatch,
        surface_width: u32,
        surface_height: u32,
        scale: f32,
    ) -> TextFrame {
        let scale = scale.max(1.0);
        let mut rasters = Vec::new();
        let mut cursor_x = TEXT_PADDING;
        let mut cursor_y = TEXT_PADDING;
        let mut row_height = 0u32;
        let mut atlas_height = TEXT_PADDING;

        for label in batch.labels() {
            let clipped = match intersect_rect(label.rect, label.clip) {
                Some(rect) => rect,
                None => continue,
            };
            let raster = self.raster_label(label, clipped, scale);
            if raster.width == 0 || raster.height == 0 {
                continue;
            }
            if cursor_x + raster.width + TEXT_PADDING > TEXT_ATLAS_WIDTH {
                cursor_x = TEXT_PADDING;
                cursor_y += row_height + TEXT_PADDING;
                row_height = 0;
            }
            let atlas = AtlasPlacement {
                x: cursor_x,
                y: cursor_y,
                width: raster.width,
                height: raster.height,
                screen: scale_rect(clipped, scale),
            };
            cursor_x += raster.width + TEXT_PADDING;
            row_height = row_height.max(raster.height);
            atlas_height = atlas_height.max(atlas.y + atlas.height + TEXT_PADDING);
            rasters.push((raster, atlas));
        }

        let atlas_height = atlas_height.max(1);
        let mut pixels = vec![0; (TEXT_ATLAS_WIDTH * atlas_height * 4) as usize];
        let mut vertices = Vec::with_capacity(rasters.len() * 6);
        for (raster, atlas) in &rasters {
            blit_raster(&mut pixels, TEXT_ATLAS_WIDTH, raster, atlas.x, atlas.y);
            push_textured_rect(
                &mut vertices,
                atlas.screen,
                *atlas,
                TEXT_ATLAS_WIDTH,
                atlas_height,
                surface_width,
                surface_height,
            );
        }

        TextFrame {
            vertices,
            pixels,
            width: TEXT_ATLAS_WIDTH,
            height: atlas_height,
            stats: TextFrameStats {
                labels: batch.len(),
                quads: rasters.len(),
                atlas_width: TEXT_ATLAS_WIDTH,
                atlas_height,
                atlas_bytes: (TEXT_ATLAS_WIDTH * atlas_height * 4) as usize,
            },
        }
    }

    fn raster_label(&mut self, label: &TextLabel, clipped: ViewRect, scale: f32) -> LabelRaster {
        let width = (clipped.width * scale)
            .ceil()
            .clamp(1.0, TEXT_ATLAS_WIDTH as f32) as u32;
        let height = (clipped.height * scale)
            .ceil()
            .max((label.line_height * scale).ceil())
            .max(1.0) as u32;
        let mut pixels = vec![0; (width * height * 4) as usize];
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(label.size * scale, label.line_height * scale),
        );
        buffer.set_wrap(label.wrap.cosmic());
        buffer.set_size(Some(width as f32), Some(height as f32));
        buffer.set_text(
            &label.text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            Some(label.align.cosmic()),
        );
        let color = TextColor::rgba(
            label.color[0],
            label.color[1],
            label.color[2],
            label.color[3],
        );
        let dx = ((label.rect.x - clipped.x) * scale).round() as i32;
        let dy = ((label.rect.y - clipped.y) * scale).round() as i32;
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            color,
            |x, y, w, h, color| {
                let [r, g, b, a] = color.as_rgba();
                for yy in 0..h as i32 {
                    for xx in 0..w as i32 {
                        let px = x + xx + dx;
                        let py = y + yy + dy;
                        if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                            continue;
                        }
                        let offset = ((py as u32 * width + px as u32) * 4) as usize;
                        pixels[offset] = r;
                        pixels[offset + 1] = g;
                        pixels[offset + 2] = b;
                        pixels[offset + 3] = a;
                    }
                }
            },
        );
        LabelRaster {
            pixels,
            width,
            height,
        }
    }
}

struct LabelRaster {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy)]
struct AtlasPlacement {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    screen: ViewRect,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl TextVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

fn create_text_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-sctk-text-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<TextVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_text_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fika-sctk-text-atlas"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fika-sctk-text-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn blit_raster(pixels: &mut [u8], atlas_width: u32, raster: &LabelRaster, x: u32, y: u32) {
    for row in 0..raster.height {
        let dst = (((y + row) * atlas_width + x) * 4) as usize;
        let src = (row * raster.width * 4) as usize;
        let len = (raster.width * 4) as usize;
        pixels[dst..dst + len].copy_from_slice(&raster.pixels[src..src + len]);
    }
}

fn push_textured_rect(
    vertices: &mut Vec<TextVertex>,
    rect: ViewRect,
    atlas: AtlasPlacement,
    atlas_width: u32,
    atlas_height: u32,
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
    let uv_left = atlas.x as f32 / atlas_width as f32;
    let uv_right = (atlas.x + atlas.width) as f32 / atlas_width as f32;
    let uv_top = atlas.y as f32 / atlas_height as f32;
    let uv_bottom = (atlas.y + atlas.height) as f32 / atlas_height as f32;

    vertices.extend_from_slice(&[
        TextVertex {
            position: [left, top],
            uv: [uv_left, uv_top],
        },
        TextVertex {
            position: [left, bottom],
            uv: [uv_left, uv_bottom],
        },
        TextVertex {
            position: [right, bottom],
            uv: [uv_right, uv_bottom],
        },
        TextVertex {
            position: [left, top],
            uv: [uv_left, uv_top],
        },
        TextVertex {
            position: [right, bottom],
            uv: [uv_right, uv_bottom],
        },
        TextVertex {
            position: [right, top],
            uv: [uv_right, uv_top],
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

fn scale_rect(rect: ViewRect, scale: f32) -> ViewRect {
    ViewRect {
        x: rect.x * scale,
        y: rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_batch_skips_empty_or_collapsed_labels() {
        let mut batch = TextBatch::default();
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        };
        batch.push("", clip, clip, 14.0, 18.0, [0, 0, 0, 255]);
        batch.push(
            "hidden",
            ViewRect { width: 0.0, ..clip },
            clip,
            14.0,
            18.0,
            [0, 0, 0, 255],
        );
        batch.push_centered("visible", clip, clip, 14.0, 18.0, [0, 0, 0, 255]);
        assert_eq!(batch.len(), 1);
    }
}
