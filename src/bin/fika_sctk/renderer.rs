use std::error::Error;
use std::ptr::NonNull;

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::shell::{WaylandSurface, xdg::window::Window};
use wayland_client::{Connection, Proxy};

use super::quad::QuadRenderer;
use super::scene::SceneFrame;
use super::text::TextRenderer;

pub(crate) struct WgpuRenderer {
    adapter: wgpu::Adapter,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: Option<wgpu::TextureFormat>,
    quad_renderer: Option<QuadRenderer>,
    text_renderer: Option<TextRenderer>,
    frame_count: u64,
}

impl WgpuRenderer {
    pub(crate) fn new(conn: &Connection, window: &Window) -> Result<Self, Box<dyn Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(conn.backend().display_ptr() as *mut _)
                .ok_or("Wayland display pointer is null")?,
        ));
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(window.wl_surface().id().as_ptr() as *mut _)
                .ok_or("Wayland surface pointer is null")?,
        ));
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })?
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))?;
        let adapter_info = adapter.get_info();
        eprintln!(
            "[fika-sctk] adapter name={:?} backend={:?}",
            adapter_info.name, adapter_info.backend
        );

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("fika-sctk-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            }))?;

        Ok(Self {
            adapter,
            surface,
            device,
            queue,
            format: None,
            quad_renderer: None,
            text_renderer: None,
            frame_count: 0,
        })
    }

    pub(crate) fn configure_surface(
        &mut self,
        width: u32,
        height: u32,
        scale: f32,
    ) -> wgpu::SurfaceConfiguration {
        let capabilities = self.surface.get_capabilities(&self.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .expect("surface must expose at least one format");
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Mailbox)
            .or_else(|| {
                capabilities
                    .present_modes
                    .iter()
                    .copied()
                    .find(|mode| *mode == wgpu::PresentMode::Fifo)
            })
            .unwrap_or(capabilities.present_modes[0]);
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let physical_width = surface_extent(width, scale);
        let physical_height = surface_extent(height, scale);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical_width,
            height: physical_height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        self.surface.configure(&self.device, &config);
        if self.format != Some(format) {
            self.quad_renderer = Some(QuadRenderer::new(&self.device, format));
            self.text_renderer = Some(TextRenderer::new(&self.device, format));
            self.format = Some(format);
        }
        config
    }

    pub(crate) fn render_scene_frame(&mut self, frame_scene: SceneFrame, reason: &str) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("[fika-sctk] surface validation error");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fika-sctk-scene-frame"),
            });
        if let Some(quad_renderer) = self.quad_renderer.as_mut() {
            quad_renderer.upload(&self.device, &self.queue, frame_scene.batch.vertices());
        }
        let text_stats = if let Some(text_renderer) = self.text_renderer.as_mut() {
            text_renderer.upload(
                &self.device,
                &self.queue,
                &frame_scene.text,
                frame.texture.width(),
                frame.texture.height(),
                frame_scene.scale,
            );
            text_renderer.stats()
        } else {
            Default::default()
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fika-sctk-scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.93,
                            g: 0.95,
                            b: 0.97,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(quad_renderer) = self.quad_renderer.as_ref() {
                quad_renderer.draw(&mut pass);
            }
            if let Some(text_renderer) = self.text_renderer.as_ref() {
                text_renderer.draw(&mut pass);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.frame_count = self.frame_count.wrapping_add(1);
        eprintln!(
            "[fika-sctk] frame={} reason={} scale={:.2} quads={} visible={} selected={} selected_count={} hover={} split_pane={} active_pane={} text_labels={} text_quads={} text_atlas={}x{}:{}b scroll_x={:.1} scroll_y={:.1}",
            self.frame_count,
            reason,
            frame_scene.scale,
            frame_scene.quads,
            frame_scene.visible_items,
            frame_scene
                .selected
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-1".to_string()),
            frame_scene.selected_count,
            frame_scene
                .hover
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-1".to_string()),
            frame_scene.split_pane as u8,
            frame_scene.active_pane,
            text_stats.labels,
            text_stats.quads,
            text_stats.atlas_width,
            text_stats.atlas_height,
            text_stats.atlas_bytes,
            frame_scene.scroll_x,
            frame_scene.scroll_y,
        );
    }
}

pub(crate) fn surface_extent(value: u32, scale: f32) -> u32 {
    ((value.max(1) as f32) * scale.max(1.0)).ceil().max(1.0) as u32
}
