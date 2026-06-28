use fika_core::{ViewRect, ViewSize};
use winit::dpi::PhysicalSize;

use crate::shell::prewarm::icon_raster_miss_budget_for_frame;
use crate::shell::render::gpu::VertexBufferUploadStats;
use crate::shell::render::quad::QuadVertex;
use crate::{
    IconFrameBuilder, IconFrameStats, IconRenderer, ShellScene, TextFrameBuilder, TextFrameStats,
    TextRenderer,
};

pub(crate) struct SceneFrame {
    pub(crate) vertices: Vec<QuadVertex>,
    pub(crate) overlay_vertices: Vec<QuadVertex>,
    pub(crate) visible_items: usize,
    pub(crate) thumbnail_candidates: usize,
    pub(crate) folder_preview_candidates: usize,
    pub(crate) quad_count: usize,
    pub(crate) content_size: ViewSize,
    pub(crate) content_scrollbar_visible: bool,
    pub(crate) first_item_rect: Option<ViewRect>,
    pub(crate) layout_us: u128,
    pub(crate) text_stats: TextFrameStats,
    pub(crate) icon_stats: IconFrameStats,
    pub(crate) vertex_upload_stats: VertexBufferUploadStats,
}

pub(crate) fn prepare_scene_frame(
    text_renderer: &mut TextRenderer,
    overlay_text_renderer: Option<&mut TextRenderer>,
    icon_renderer: &mut IconRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &mut ShellScene,
    size: PhysicalSize<u32>,
    reason: &str,
) -> SceneFrame {
    text_renderer.label_cache.begin_frame();
    text_renderer.metrics_cache.begin_frame();
    icon_renderer.raster_cache.begin_frame();
    icon_renderer.role_raster_cache.begin_frame();

    if let Some(overlay_text_renderer) = overlay_text_renderer {
        overlay_text_renderer.label_cache.begin_frame();
        overlay_text_renderer.metrics_cache.begin_frame();
        let (mut scene_frame, mut text_frame, mut overlay_text_frame, mut icon_frame) = {
            let text_pixels = text_renderer.take_staging_pixels();
            let overlay_text_pixels = overlay_text_renderer.take_staging_pixels();
            let mut text_builder = TextFrameBuilder::new(
                &mut text_renderer.font_system,
                &mut text_renderer.swash_cache,
                &mut text_renderer.text_buffer,
                &mut text_renderer.label_cache,
                &mut text_renderer.metrics_cache,
                &mut text_renderer.atlas_cache,
                size,
                scene.ui_scale(),
                text_pixels,
            );
            let mut overlay_text_builder = TextFrameBuilder::new(
                &mut overlay_text_renderer.font_system,
                &mut overlay_text_renderer.swash_cache,
                &mut overlay_text_renderer.text_buffer,
                &mut overlay_text_renderer.label_cache,
                &mut overlay_text_renderer.metrics_cache,
                &mut overlay_text_renderer.atlas_cache,
                size,
                scene.ui_scale(),
                overlay_text_pixels,
            );
            let mut icon_builder = IconFrameBuilder::new(
                &mut icon_renderer.resolver,
                &mut icon_renderer.thumbnails,
                &mut icon_renderer.icon_rasters,
                &mut icon_renderer.raster_cache,
                &mut icon_renderer.role_raster_cache,
                size,
                icon_raster_miss_budget_for_frame(reason),
                scene.folder_preview_roles.borrow().ready_len(),
                scene.folder_preview_roles.borrow().ready_bytes(),
            );
            let scene_frame = scene.build_frame(
                size,
                &mut text_builder,
                &mut icon_builder,
                Some(&mut overlay_text_builder),
            );
            let text_frame = text_builder.finish();
            let overlay_text_frame = overlay_text_builder.finish();
            let icon_frame = icon_builder.finish();
            (scene_frame, text_frame, overlay_text_frame, icon_frame)
        };

        let mut vertex_upload_stats = VertexBufferUploadStats::default();
        vertex_upload_stats.merge(icon_renderer.upload(device, queue, &mut icon_frame));
        vertex_upload_stats.merge(text_renderer.upload(device, queue, &mut text_frame));
        vertex_upload_stats.merge(overlay_text_renderer.upload(
            device,
            queue,
            &mut overlay_text_frame,
        ));
        let (text_swash_images, text_swash_outlines, text_swash_reset) =
            text_renderer.trim_text_engine_caches();
        let (overlay_swash_images, overlay_swash_outlines, overlay_swash_reset) =
            overlay_text_renderer.trim_text_engine_caches();
        scene_frame.icon_stats = icon_frame.stats;
        scene_frame.text_stats = text_frame.stats.merged(overlay_text_frame.stats);
        scene_frame.text_stats.swash_image_entries = text_swash_images.max(overlay_swash_images);
        scene_frame.text_stats.swash_outline_entries =
            text_swash_outlines.max(overlay_swash_outlines);
        scene_frame.text_stats.swash_resets =
            usize::from(text_swash_reset) + usize::from(overlay_swash_reset);
        scene_frame.vertex_upload_stats = vertex_upload_stats;
        scene_frame
    } else {
        let (mut scene_frame, mut text_frame, mut icon_frame) = {
            let text_pixels = text_renderer.take_staging_pixels();
            let mut text_builder = TextFrameBuilder::new(
                &mut text_renderer.font_system,
                &mut text_renderer.swash_cache,
                &mut text_renderer.text_buffer,
                &mut text_renderer.label_cache,
                &mut text_renderer.metrics_cache,
                &mut text_renderer.atlas_cache,
                size,
                scene.ui_scale(),
                text_pixels,
            );
            let mut icon_builder = IconFrameBuilder::new(
                &mut icon_renderer.resolver,
                &mut icon_renderer.thumbnails,
                &mut icon_renderer.icon_rasters,
                &mut icon_renderer.raster_cache,
                &mut icon_renderer.role_raster_cache,
                size,
                icon_raster_miss_budget_for_frame(reason),
                scene.folder_preview_roles.borrow().ready_len(),
                scene.folder_preview_roles.borrow().ready_bytes(),
            );
            let scene_frame = scene.build_frame(size, &mut text_builder, &mut icon_builder, None);
            let text_frame = text_builder.finish();
            let icon_frame = icon_builder.finish();
            (scene_frame, text_frame, icon_frame)
        };

        let mut vertex_upload_stats = VertexBufferUploadStats::default();
        vertex_upload_stats.merge(icon_renderer.upload(device, queue, &mut icon_frame));
        vertex_upload_stats.merge(text_renderer.upload(device, queue, &mut text_frame));
        let (text_swash_images, text_swash_outlines, text_swash_reset) =
            text_renderer.trim_text_engine_caches();
        scene_frame.icon_stats = icon_frame.stats;
        scene_frame.text_stats = text_frame.stats;
        scene_frame.text_stats.swash_image_entries = text_swash_images;
        scene_frame.text_stats.swash_outline_entries = text_swash_outlines;
        scene_frame.text_stats.swash_resets = usize::from(text_swash_reset);
        scene_frame.vertex_upload_stats = vertex_upload_stats;
        scene_frame
    }
}
