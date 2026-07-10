#[cfg(test)]
use std::fs;
#[cfg(all(test, unix))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use winit::data_transfer::{
    DataTransferId, DataTransferSendBuilder, SendData, TypeHint, TypedData,
};
use winit::dpi::PhysicalPosition;
use winit::event_loop::{ActiveEventLoop, AsyncRequestSerial, DndAction, DragIcon};
use winit::icon::RgbaIcon;

use super::outcome::ShellActionOutcome;
use crate::shell::drop_menu::ShellDropTarget;
use crate::shell::icon_roles::{
    FileIconKind, FileIconPathCacheKey, FileIconRoleCacheKey, NamedIconFallback,
    file_icon_path_cache_key, icon_cache_size,
};
use crate::shell::tasks::ShellTaskStatus;
use crate::{
    FikaWgpuApp, FolderPreviewReady, IconRaster, IconRasterCacheKey, IncomingDndTransfer,
    ItemPixmapLayout, OutgoingDndTransfer, ShellInternalDragPreviewSource, ShellViewMode, ViewRect,
    decode_file_clipboard_text, entry_path_for_thumbnail, folder_preview_role_draw_rect,
    icon_emblem_kinds_for_path, icon_emblem_rects, path_uri_from_path, rasterize_icon,
    thumbnail_request_may_have_preview, view_point_from_physical_position,
};

const ACCEPTED_DND_ACTIONS: [DndAction; 3] = [DndAction::Ask, DndAction::Move, DndAction::Copy];
const DEEPIN_DND_ICON_SIZE: f32 = 128.0;
const DEEPIN_DND_ICON_OUTLINE: f32 = 30.0;
const DEEPIN_DND_ICON_MAX: usize = 4;
const DEEPIN_DND_ICON_MAX_COUNT: usize = 99;
const DEEPIN_DND_ICON_ROTATE: f32 = 10.0;
const DEEPIN_DND_ICON_OPACITY: f32 = 0.1;

#[derive(Clone, Debug)]
struct OutgoingDndPayload {
    uris: Vec<String>,
    text: String,
}

#[derive(Clone, Debug)]
struct OutgoingDndPreviewRaster {
    icon: IconRaster,
}

impl FikaWgpuApp {
    pub(crate) fn reset_outgoing_drag_tracking(&mut self) {
        self.outgoing_dnd_transfer = None;
        self.outgoing_dnd_start_failed = false;
    }

    pub(crate) fn start_outgoing_drag_if_needed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.outgoing_dnd_transfer.is_some()
            || self.outgoing_dnd_start_failed
            || !self.scene.internal_drag_active()
        {
            return;
        }
        let Some(window_id) = self.window.as_ref().map(|window| window.id()) else {
            return;
        };
        let Some(paths) = self.scene.active_internal_drag_paths() else {
            return;
        };
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let payload = outgoing_dnd_payload(&paths);
        let scale = self.scene.ui_scale();
        let preview_source = self.scene.active_internal_drag_preview_source(size);
        let preview_icon_size = outgoing_dnd_preview_icon_size(preview_source.as_ref(), scale);
        let preview_raster = self.outgoing_dnd_preview_raster(
            preview_source.as_ref(),
            &paths,
            preview_icon_size,
            scale,
        );
        let drag_icon =
            outgoing_dnd_drag_icon(&paths, preview_icon_size, scale, preview_raster.as_ref());
        let send_data = DataTransferSendBuilder::new(payload)
            .with_type(TypeHint::UriList, |payload, _| {
                Some(SendData::Uris(payload.uris.clone()))
            })
            .with_type(TypeHint::Plaintext, |payload, _| Some(payload.text.clone()))
            .build();
        match event_loop.start_drag(window_id, send_data, &ACCEPTED_DND_ACTIONS, drag_icon) {
            Ok(id) => {
                fika_log!(
                    "[fika-wgpu] outgoing-dnd start id={} sources={}",
                    id.into_raw(),
                    paths.len()
                );
                self.outgoing_dnd_transfer = Some(OutgoingDndTransfer { id, paths });
            }
            Err(error) => {
                self.outgoing_dnd_start_failed = true;
                fika_log!("[fika-wgpu] outgoing-dnd-unavailable {error}");
            }
        }
    }

    fn outgoing_dnd_preview_raster(
        &mut self,
        source: Option<&ShellInternalDragPreviewSource>,
        paths: &[PathBuf],
        icon_size: u32,
        scale: f32,
    ) -> Option<OutgoingDndPreviewRaster> {
        let renderer = self.renderer.as_mut()?;
        let icon_size_px = icon_cache_size(icon_size as f32);
        let icon = match source {
            Some(ShellInternalDragPreviewSource::PaneItem {
                directory,
                entry,
                folder_preview,
                ..
            }) => {
                let icon_path = directory.join(entry.name.as_ref());
                let base = if entry.is_dir {
                    let resolved = renderer.icon_renderer.resolver.resolve_entry_visible_fast(
                        directory,
                        entry,
                        icon_size as f32,
                    );
                    let base = rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?;
                    apply_folder_preview_to_drag_icon(base, folder_preview.as_ref())
                } else if let Some(raster) = ready_drag_thumbnail(
                    &mut renderer.icon_renderer.raster_cache,
                    &mut renderer.icon_renderer.thumbnails,
                    directory,
                    entry,
                    icon_size_px,
                ) {
                    raster
                } else {
                    let resolved = renderer.icon_renderer.resolver.resolve_entry_visible_fast(
                        directory,
                        entry,
                        icon_size as f32,
                    );
                    rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?
                };
                apply_drag_emblems(renderer, base, &icon_path, scale)
            }
            Some(ShellInternalDragPreviewSource::Place { icon_name, .. }) => {
                rasterize_named_drag_icon(renderer, icon_name, icon_size_px)?
            }
            None => {
                let path = paths.first()?;
                let key =
                    file_icon_path_cache_key(path, path.is_dir(), None, true, icon_size as f32);
                let resolved = renderer
                    .icon_renderer
                    .resolver
                    .resolve_path_cache_key_fast(key);
                let base = rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?;
                apply_drag_emblems(renderer, base, path, scale)
            }
        };
        Some(OutgoingDndPreviewRaster { icon })
    }

    pub(crate) fn external_drag_entered(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: DataTransferId,
        position: Option<PhysicalPosition<f64>>,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        let mut transfer = IncomingDndTransfer::new(id, position);
        let supports_uri_list = event_loop
            .data_transfer(id)
            .map(|data| data.has_type(&TypeHint::UriList))
            .unwrap_or_else(|error| {
                fika_log!(
                    "[fika-wgpu] external-dnd data-transfer-error id={} {error}",
                    id.into_raw()
                );
                false
            });

        if !supports_uri_list {
            self.set_valid_dnd_actions(event_loop, id, false);
            self.incoming_dnd_transfer = None;
            let changed = self.scene.clear_external_drag();
            fika_log!(
                "[fika-wgpu] external-dnd reject id={} reason=missing-uri-list",
                id.into_raw()
            );
            return ShellActionOutcome::redraw_if(changed);
        }

        match event_loop.fetch_data_transfer(id, &TypeHint::UriList) {
            Ok(serial) => {
                transfer.fetch_serial = Some(serial);
            }
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] external-dnd fetch-error id={} {error}",
                    id.into_raw()
                );
                self.set_valid_dnd_actions(event_loop, id, false);
                self.incoming_dnd_transfer = None;
                let changed = self.scene.clear_external_drag();
                return ShellActionOutcome::redraw_if(changed);
            }
        }

        let changed = position
            .map(|position| {
                let point = view_point_from_physical_position(position);
                self.scene.begin_external_drag(Vec::new(), point, size)
            })
            .unwrap_or(false);
        self.incoming_dnd_transfer = Some(transfer);
        self.sync_external_dnd_actions(event_loop, id);
        fika_log!(
            "[fika-wgpu] external-dnd enter id={} target={}",
            id.into_raw(),
            self.scene
                .dnd_hover_target
                .as_ref()
                .map(ShellDropTarget::kind)
                .unwrap_or("none")
        );
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn external_drag_position(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: DataTransferId,
        position: PhysicalPosition<f64>,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        let Some(transfer) = self
            .incoming_dnd_transfer
            .as_mut()
            .filter(|transfer| transfer.id == id)
        else {
            return ShellActionOutcome::None;
        };
        transfer.last_position = Some(position);
        let point = view_point_from_physical_position(position);
        let changed = if let Some(paths) = transfer.paths.clone() {
            if self.scene.external_drag.is_some() {
                self.scene.update_external_drag(point, size)
            } else {
                self.scene.begin_external_drag(paths, point, size)
            }
        } else {
            false
        };
        self.sync_external_dnd_actions(event_loop, id);
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn external_drag_dropped(&mut self, id: DataTransferId) -> ShellActionOutcome {
        self.finish_external_drag_if_ready(id)
    }

    pub(crate) fn external_drag_left(&mut self, id: DataTransferId) -> ShellActionOutcome {
        let changed = if self
            .incoming_dnd_transfer
            .as_ref()
            .is_some_and(|transfer| transfer.id == id)
        {
            self.incoming_dnd_transfer = None;
            self.scene.clear_external_drag()
        } else {
            false
        };
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn external_drag_data_received(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: DataTransferId,
        serial: AsyncRequestSerial,
        value: Arc<dyn TypedData>,
    ) -> ShellActionOutcome {
        let Some(transfer) = self
            .incoming_dnd_transfer
            .as_mut()
            .filter(|transfer| transfer.id == id)
        else {
            return ShellActionOutcome::None;
        };
        if transfer
            .fetch_serial
            .is_some_and(|fetch_serial| fetch_serial != serial)
        {
            return ShellActionOutcome::None;
        }

        let paths = match external_drag_paths_from_typed_data(value.as_ref()) {
            Ok(paths) => paths,
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] external-dnd data-error id={} {error}",
                    id.into_raw()
                );
                self.set_valid_dnd_actions(event_loop, id, false);
                self.incoming_dnd_transfer = None;
                let changed = self.scene.clear_external_drag();
                return ShellActionOutcome::redraw_if(changed);
            }
        };
        if paths.is_empty() {
            self.set_valid_dnd_actions(event_loop, id, false);
            self.incoming_dnd_transfer = None;
            let changed = self.scene.clear_external_drag();
            fika_log!(
                "[fika-wgpu] external-dnd reject id={} reason=empty-uri-list",
                id.into_raw()
            );
            return ShellActionOutcome::redraw_if(changed);
        }

        transfer.paths = Some(paths.clone());
        let changed = if let (Some(position), Some(size)) = (
            transfer.last_position,
            self.renderer.as_ref().map(|renderer| renderer.size),
        ) {
            let point = view_point_from_physical_position(position);
            self.scene.begin_external_drag(paths, point, size)
        } else {
            false
        };
        let drop_pending = transfer.drop_pending;
        self.sync_external_dnd_actions(event_loop, id);
        fika_log!(
            "[fika-wgpu] external-dnd data id={} sources={}",
            id.into_raw(),
            self.incoming_dnd_transfer
                .as_ref()
                .and_then(|transfer| transfer.paths.as_ref())
                .map(Vec::len)
                .unwrap_or(0)
        );
        if drop_pending {
            return self.finish_external_drag_if_ready(id);
        }
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn outgoing_drag_dropped(
        &mut self,
        id: DataTransferId,
        action: Option<DndAction>,
    ) -> ShellActionOutcome {
        if !self
            .outgoing_dnd_transfer
            .as_ref()
            .is_some_and(|transfer| transfer.id == id)
        {
            return ShellActionOutcome::None;
        }
        let source_count = self
            .outgoing_dnd_transfer
            .as_ref()
            .map(|transfer| transfer.paths.len())
            .unwrap_or(0);
        self.outgoing_dnd_transfer = None;
        self.outgoing_dnd_start_failed = false;
        let changed = self.scene.clear_internal_drag();
        fika_log!(
            "[fika-wgpu] outgoing-dnd drop id={} action={:?} sources={}",
            id.into_raw(),
            action,
            source_count
        );
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn outgoing_drag_canceled(&mut self, id: DataTransferId) -> ShellActionOutcome {
        if !self
            .outgoing_dnd_transfer
            .as_ref()
            .is_some_and(|transfer| transfer.id == id)
        {
            return ShellActionOutcome::None;
        }
        let source_count = self
            .outgoing_dnd_transfer
            .as_ref()
            .map(|transfer| transfer.paths.len())
            .unwrap_or(0);
        self.outgoing_dnd_transfer = None;
        self.outgoing_dnd_start_failed = false;
        let changed = self.scene.clear_internal_drag();
        fika_log!(
            "[fika-wgpu] outgoing-dnd cancel id={} sources={}",
            id.into_raw(),
            source_count
        );
        ShellActionOutcome::redraw_if(changed)
    }

    fn finish_external_drag_if_ready(&mut self, id: DataTransferId) -> ShellActionOutcome {
        let Some(transfer) = self
            .incoming_dnd_transfer
            .as_mut()
            .filter(|transfer| transfer.id == id)
        else {
            return ShellActionOutcome::None;
        };
        transfer.drop_pending = true;
        let Some(paths) = transfer.paths.clone() else {
            return ShellActionOutcome::None;
        };
        let Some(position) = transfer.last_position else {
            return ShellActionOutcome::None;
        };
        self.incoming_dnd_transfer = None;
        self.finish_external_drag_paths(paths, position)
    }

    fn finish_external_drag_paths(
        &mut self,
        paths: Vec<PathBuf>,
        position: PhysicalPosition<f64>,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        let point = view_point_from_physical_position(position);
        let sources = external_drag_drop_sources(paths, self.scene.external_drag_sources());
        match self.scene.finish_external_drag(sources, point, size) {
            Ok(changed) => {
                fika_log!(
                    "[fika-wgpu] external-dnd drop menu={} target={}",
                    self.scene.drop_menu.is_some() as u8,
                    self.scene
                        .drop_menu
                        .as_ref()
                        .map(|menu| menu.target.kind())
                        .unwrap_or("none")
                );
                ShellActionOutcome::redraw_if(changed)
            }
            Err(error) => {
                fika_log!("[fika-wgpu] external-dnd-error {error}");
                self.scene
                    .record_task_status(ShellTaskStatus::failed("Drop failed", error, false));
                ShellActionOutcome::Redraw
            }
        }
    }

    fn sync_external_dnd_actions(&self, event_loop: &dyn ActiveEventLoop, id: DataTransferId) {
        let accepted = self
            .incoming_dnd_transfer
            .as_ref()
            .filter(|transfer| transfer.id == id)
            .is_some_and(|transfer| {
                if transfer.paths.is_none() || transfer.last_position.is_none() {
                    return true;
                }
                self.scene.dnd_hover_target.is_some()
            });
        self.set_valid_dnd_actions(event_loop, id, accepted);
    }

    fn set_valid_dnd_actions(
        &self,
        event_loop: &dyn ActiveEventLoop,
        id: DataTransferId,
        accepted: bool,
    ) {
        let actions = if accepted {
            ACCEPTED_DND_ACTIONS.as_slice()
        } else {
            &[]
        };
        if let Err(error) = event_loop.set_valid_dnd_actions(id, actions) {
            fika_log!(
                "[fika-wgpu] dnd-actions-error id={} accepted={} {error}",
                id.into_raw(),
                accepted as u8
            );
        }
    }
}

fn outgoing_dnd_payload(paths: &[PathBuf]) -> OutgoingDndPayload {
    let uris = paths
        .iter()
        .map(|path| path_uri_from_path(path))
        .collect::<Vec<_>>();
    let text = uris.join("\n");
    OutgoingDndPayload { uris, text }
}

fn ready_drag_thumbnail(
    raster_cache: &mut crate::IconRasterCache,
    thumbnails: &mut crate::ThumbnailRasterResolver,
    directory: &Path,
    entry: &crate::Entry,
    size_px: u16,
) -> Option<IconRaster> {
    let path = entry_path_for_thumbnail(directory, entry);
    let modified_secs = entry.modified_secs?;
    if !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref()) {
        return None;
    }
    let key = IconRasterCacheKey::thumbnail(path, size_px, modified_secs);
    if let Some(raster) = raster_cache.get(&key) {
        return Some(raster);
    }
    thumbnails.drain_results();
    let entry = thumbnails.ready.get_mut(&key)?;
    thumbnails.ready_frame = thumbnails.ready_frame.wrapping_add(1);
    entry.last_used_frame = thumbnails.ready_frame;
    Some(raster_cache.insert(key, entry.raster.clone()))
}

fn rasterize_resolved_drag_icon(
    renderer: &mut crate::WgpuState,
    icon_path: Option<PathBuf>,
    size_px: u16,
) -> Option<IconRaster> {
    let icon_path = icon_path?;
    let key = IconRasterCacheKey::icon(icon_path, size_px);
    if let Some(raster) = renderer.icon_renderer.raster_cache.get(&key) {
        return Some(raster);
    }
    let raster = rasterize_icon(&key.path, size_px as u32)?;
    Some(renderer.icon_renderer.raster_cache.insert(key, raster))
}

fn rasterize_named_drag_icon(
    renderer: &mut crate::WgpuState,
    icon_name: &str,
    size_px: u16,
) -> Option<IconRaster> {
    let key = FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: FileIconKind::Named {
                icon_name: icon_name.to_string(),
                fallback: NamedIconFallback::Service,
            },
        },
        size_px,
    };
    let resolved = renderer
        .icon_renderer
        .resolver
        .resolve_path_cache_key_fast(key);
    rasterize_resolved_drag_icon(renderer, resolved.path, size_px)
}

fn rasterize_named_drag_icon_exact(
    renderer: &mut crate::WgpuState,
    icon_name: &str,
    size_px: u16,
) -> Option<IconRaster> {
    let path = renderer
        .icon_renderer
        .resolver
        .resolve_named_exact_fast(icon_name, size_px as f32)?;
    rasterize_resolved_drag_icon(renderer, Some(path), size_px)
}

fn apply_folder_preview_to_drag_icon(
    base: IconRaster,
    folder_preview: Option<&FolderPreviewReady>,
) -> IconRaster {
    let Some(folder_preview) = folder_preview else {
        return base;
    };
    let layout = ItemPixmapLayout {
        view_mode: ShellViewMode::Icons,
        icon_rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: base.width as f32,
            height: base.height as f32,
        },
        text_rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: base.width as f32,
            height: base.height as f32,
        },
        text_midline_shift: 0.0,
    };
    let draw_rect = folder_preview_role_draw_rect(layout, &folder_preview.raster);
    let rect = PixelRect::new(
        draw_rect.x.round() as i32,
        draw_rect.y.round() as i32,
        draw_rect.width.round().max(1.0) as i32,
        draw_rect.height.round().max(1.0) as i32,
    );
    let mut pixels = base.pixels.to_vec();
    draw_raster_scaled(&mut pixels, base.width, &folder_preview.raster, rect, 1.0);
    IconRaster {
        pixels: Arc::from(pixels),
        width: base.width,
        height: base.height,
    }
}

fn apply_drag_emblems(
    renderer: &mut crate::WgpuState,
    base: IconRaster,
    path: &Path,
    scale: f32,
) -> IconRaster {
    let emblems = icon_emblem_kinds_for_path(path);
    if emblems.is_empty() {
        return base;
    }
    let rects = drag_emblem_pixel_rects(base.width, scale);
    let mut pixels = base.pixels.to_vec();
    for (index, emblem) in emblems.into_iter().take(rects.len()).enumerate() {
        let rect = rects[index];
        let size_px = icon_cache_size(rect.width.max(rect.height) as f32);
        for icon_name in emblem.theme_names() {
            if let Some(raster) = rasterize_named_drag_icon_exact(renderer, icon_name, size_px) {
                draw_raster_scaled(&mut pixels, base.width, &raster, rect, 1.0);
                break;
            }
        }
    }
    IconRaster {
        pixels: Arc::from(pixels),
        width: base.width,
        height: base.height,
    }
}

fn drag_emblem_pixel_rects(icon_size: u32, scale: f32) -> [PixelRect; 4] {
    let paint_area = ViewRect {
        x: 0.0,
        y: 0.0,
        width: icon_size as f32,
        height: icon_size as f32,
    };
    icon_emblem_rects(paint_area, scale).map(|rect| {
        PixelRect::new(
            rect.x.round() as i32,
            rect.y.round() as i32,
            rect.width.round().max(1.0) as i32,
            rect.height.round().max(1.0) as i32,
        )
    })
}

fn outgoing_dnd_drag_icon(
    paths: &[PathBuf],
    icon_size: u32,
    scale: f32,
    raster: Option<&OutgoingDndPreviewRaster>,
) -> Option<DragIcon> {
    let metrics = outgoing_dnd_preview_metrics(icon_size, scale);
    let pixels = outgoing_dnd_preview_pixels(paths, metrics, raster);
    let icon = RgbaIcon::new(pixels, metrics.canvas_size, metrics.canvas_size)
        .ok()?
        .into();
    Some(DragIcon {
        icon,
        offset_x: -(metrics.canvas_size as i32) / 2,
        offset_y: -(metrics.canvas_size as i32) / 2,
    })
}

#[derive(Clone, Copy, Debug)]
struct OutgoingDndPreviewMetrics {
    canvas_size: u32,
    icon_size: u32,
    outline: i32,
    unit: f32,
}

fn outgoing_dnd_preview_icon_size(
    source: Option<&ShellInternalDragPreviewSource>,
    scale: f32,
) -> u32 {
    let unit = scale.clamp(1.0, 2.0);
    match source {
        Some(ShellInternalDragPreviewSource::PaneItem { icon_size, .. })
        | Some(ShellInternalDragPreviewSource::Place { icon_size, .. }) => {
            icon_size.round().clamp(16.0 * unit, 256.0 * unit) as u32
        }
        None => (DEEPIN_DND_ICON_SIZE * unit).round() as u32,
    }
}

fn outgoing_dnd_preview_metrics(icon_size: u32, scale: f32) -> OutgoingDndPreviewMetrics {
    let unit = scale.clamp(1.0, 2.0);
    let outline = (DEEPIN_DND_ICON_OUTLINE * unit).round() as i32;
    OutgoingDndPreviewMetrics {
        canvas_size: icon_size + outline.max(0) as u32 * 2,
        icon_size,
        outline,
        unit,
    }
}

fn outgoing_dnd_preview_pixels(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    raster: Option<&OutgoingDndPreviewRaster>,
) -> Vec<u8> {
    let mut pixels = vec![0; (metrics.canvas_size * metrics.canvas_size * 4) as usize];
    let count = paths.len().max(1);
    let is_dir = paths.first().is_some_and(|path| path.is_dir());
    let fallback;
    let raster = match raster {
        Some(raster) => &raster.icon,
        None => {
            fallback = fallback_drag_icon_raster(is_dir, metrics.icon_size);
            &fallback
        }
    };
    let icon_rect = PixelRect::new(
        metrics.outline,
        metrics.outline,
        metrics.icon_size as i32,
        metrics.icon_size as i32,
    );
    let ghost_count = count.saturating_sub(1).min(DEEPIN_DND_ICON_MAX - 1);
    for index in (0..ghost_count).rev() {
        let opacity = 1.0 - (index as f32 + 5.0) * DEEPIN_DND_ICON_OPACITY;
        draw_raster_rotated(
            &mut pixels,
            metrics.canvas_size,
            raster,
            icon_rect,
            deepin_drag_icon_rotation(index),
            opacity,
        );
    }
    draw_raster_rotated(
        &mut pixels,
        metrics.canvas_size,
        raster,
        icon_rect,
        0.0,
        0.8,
    );
    if count > 1 {
        draw_count_badge(&mut pixels, metrics, count);
    }
    pixels
}

fn deepin_drag_icon_rotation(index: usize) -> f32 {
    let magnitude = (((index as f32 + 1.0) / 2.0).round() / 2.0) + 1.0;
    let direction = if index % 2 == 1 { -1.0 } else { 1.0 };
    DEEPIN_DND_ICON_ROTATE * magnitude * direction
}

fn fallback_drag_icon_raster(is_dir: bool, size: u32) -> IconRaster {
    let mut pixels = vec![0; (size * size * 4) as usize];
    let unit = size as f32 / DEEPIN_DND_ICON_SIZE;
    let rect = PixelRect::new(
        (18.0 * unit).round() as i32,
        (16.0 * unit).round() as i32,
        (92.0 * unit).round() as i32,
        (96.0 * unit).round() as i32,
    );
    if is_dir {
        draw_folder_card(&mut pixels, size, rect, 255);
    } else {
        draw_file_card(&mut pixels, size, rect, 255);
    }
    IconRaster {
        pixels: Arc::from(pixels),
        width: size,
        height: size,
    }
}

fn draw_file_card(pixels: &mut [u8], size: u32, rect: PixelRect, alpha: u8) {
    draw_rounded_rect(pixels, size, rect, 8, [245, 248, 250, alpha]);
    let fold = rect.width / 4;
    let fold_rect = PixelRect::new(rect.right() - fold, rect.y, fold, fold);
    draw_triangle(
        pixels,
        size,
        fold_rect.x,
        fold_rect.y,
        fold_rect.right(),
        fold_rect.y,
        fold_rect.right(),
        fold_rect.bottom(),
        [191, 212, 233, alpha],
    );
    let line_color = [78, 122, 165, alpha.saturating_mul(7) / 10];
    for i in 0..4 {
        let y = rect.y + rect.height / 3 + i * rect.height / 8;
        draw_rounded_rect(
            pixels,
            size,
            PixelRect::new(rect.x + rect.width / 5, y, rect.width * 3 / 5, 3),
            1,
            line_color,
        );
    }
    draw_rounded_rect_outline(pixels, size, rect, 8, [78, 122, 165, alpha / 4]);
}

fn draw_folder_card(pixels: &mut [u8], size: u32, rect: PixelRect, alpha: u8) {
    let tab = PixelRect::new(
        rect.x + rect.width / 9,
        rect.y + rect.height / 7,
        rect.width * 2 / 5,
        rect.height / 5,
    );
    draw_rounded_rect(pixels, size, tab, 5, [91, 157, 222, alpha]);
    let body = PixelRect::new(
        rect.x,
        rect.y + rect.height / 4,
        rect.width,
        rect.height * 3 / 4,
    );
    draw_rounded_rect(pixels, size, body, 9, [61, 133, 211, alpha]);
    let front = PixelRect::new(
        rect.x + rect.width / 12,
        rect.y + rect.height / 3,
        rect.width * 5 / 6,
        rect.height / 2,
    );
    draw_rounded_rect(pixels, size, front, 7, [95, 169, 235, alpha]);
    draw_rounded_rect_outline(pixels, size, body, 9, [25, 77, 136, alpha / 3]);
}

fn draw_count_badge(pixels: &mut [u8], metrics: OutgoingDndPreviewMetrics, count: usize) {
    let length = if count > DEEPIN_DND_ICON_MAX_COUNT {
        28.0
    } else {
        24.0
    };
    let radius = (length * metrics.unit / 2.0).round() as i32;
    let inset = (10.0 * metrics.unit).round() as i32;
    let center_x = metrics.outline + metrics.icon_size as i32 - inset;
    let center_y = metrics.outline + metrics.icon_size as i32 - inset;
    draw_circle(
        pixels,
        metrics.canvas_size,
        center_x,
        center_y,
        radius,
        [244, 74, 74, 255],
    );
    draw_circle_outline(
        pixels,
        metrics.canvas_size,
        center_x,
        center_y,
        radius,
        [255, 255, 255, 230],
    );
    let label = if count > DEEPIN_DND_ICON_MAX_COUNT {
        format!("{DEEPIN_DND_ICON_MAX_COUNT}+")
    } else {
        count.to_string()
    };
    let digit_scale = if label.len() > 2 { 2.0 } else { 3.0 };
    draw_digit_label(
        pixels,
        metrics.canvas_size,
        &label,
        center_x,
        center_y,
        (digit_scale * metrics.unit).round().max(2.0) as i32,
        [255, 255, 255, 255],
    );
}

#[derive(Clone, Copy, Debug)]
struct PixelRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl PixelRect {
    fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width: width.max(1),
            height: height.max(1),
        }
    }

    fn right(self) -> i32 {
        self.x + self.width
    }

    fn bottom(self) -> i32 {
        self.y + self.height
    }
}

fn draw_raster_rotated(
    pixels: &mut [u8],
    canvas_size: u32,
    raster: &IconRaster,
    rect: PixelRect,
    angle_degrees: f32,
    opacity: f32,
) {
    if opacity <= 0.0 || raster.width == 0 || raster.height == 0 {
        return;
    }
    let half_width = rect.width as f32 / 2.0;
    let half_height = rect.height as f32 / 2.0;
    let center_x = rect.x as f32 + half_width;
    let center_y = rect.y as f32 + half_height;
    let radius = (half_width.hypot(half_height)).ceil() as i32 + 1;
    let radians = angle_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let min_x = (center_x as i32 - radius).max(0);
    let max_x = (center_x as i32 + radius + 1).min(canvas_size as i32);
    let min_y = (center_y as i32 - radius).max(0);
    let max_y = (center_y as i32 + radius + 1).min(canvas_size as i32);
    for y in min_y..max_y {
        for x in min_x..max_x {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let local_x = cos * dx + sin * dy + half_width;
            let local_y = -sin * dx + cos * dy + half_height;
            if local_x < 0.0
                || local_y < 0.0
                || local_x >= rect.width as f32
                || local_y >= rect.height as f32
            {
                continue;
            }
            let source_x = local_x / rect.width as f32 * (raster.width as f32 - 1.0);
            let source_y = local_y / rect.height as f32 * (raster.height as f32 - 1.0);
            if let Some(color) = sample_raster_bilinear(raster, source_x, source_y, opacity) {
                blend_pixel(pixels, canvas_size, x, y, color);
            }
        }
    }
}

fn draw_raster_scaled(
    pixels: &mut [u8],
    canvas_size: u32,
    raster: &IconRaster,
    rect: PixelRect,
    opacity: f32,
) {
    if opacity <= 0.0 || raster.width == 0 || raster.height == 0 {
        return;
    }
    let min_x = rect.x.max(0);
    let max_x = rect.right().min(canvas_size as i32);
    let min_y = rect.y.max(0);
    let max_y = rect.bottom().min(canvas_size as i32);
    for y in min_y..max_y {
        for x in min_x..max_x {
            let local_x = (x - rect.x) as f32 + 0.5;
            let local_y = (y - rect.y) as f32 + 0.5;
            let source_x = local_x / rect.width as f32 * (raster.width as f32 - 1.0);
            let source_y = local_y / rect.height as f32 * (raster.height as f32 - 1.0);
            if let Some(color) = sample_raster_bilinear(raster, source_x, source_y, opacity) {
                blend_pixel(pixels, canvas_size, x, y, color);
            }
        }
    }
}

fn sample_raster_bilinear(raster: &IconRaster, x: f32, y: f32, opacity: f32) -> Option<[u8; 4]> {
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let x1 = (x0 + 1).min(raster.width - 1);
    let y1 = (y0 + 1).min(raster.height - 1);
    let tx = (x - x0 as f32).clamp(0.0, 1.0);
    let ty = (y - y0 as f32).clamp(0.0, 1.0);
    let p00 = raster_pixel(raster, x0, y0);
    let p10 = raster_pixel(raster, x1, y0);
    let p01 = raster_pixel(raster, x0, y1);
    let p11 = raster_pixel(raster, x1, y1);
    let mut out = [0u8; 4];
    for channel in 0..4 {
        let top = lerp(p00[channel] as f32, p10[channel] as f32, tx);
        let bottom = lerp(p01[channel] as f32, p11[channel] as f32, tx);
        let value = lerp(top, bottom, ty);
        out[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out[3] as f32 * opacity.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8;
    (out[3] > 0).then_some(out)
}

fn raster_pixel(raster: &IconRaster, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * raster.width + x) * 4) as usize;
    [
        raster.pixels[offset],
        raster.pixels[offset + 1],
        raster.pixels[offset + 2],
        raster.pixels[offset + 3],
    ]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn draw_rounded_rect(pixels: &mut [u8], size: u32, rect: PixelRect, radius: i32, color: [u8; 4]) {
    if color[3] == 0 {
        return;
    }
    let radius = radius.max(0).min(rect.width / 2).min(rect.height / 2);
    let min_x = rect.x.max(0);
    let max_x = rect.right().min(size as i32);
    let min_y = rect.y.max(0);
    let max_y = rect.bottom().min(size as i32);
    for y in min_y..max_y {
        for x in min_x..max_x {
            if rounded_rect_contains(rect, radius, x, y) {
                blend_pixel(pixels, size, x, y, color);
            }
        }
    }
}

fn draw_rounded_rect_outline(
    pixels: &mut [u8],
    size: u32,
    rect: PixelRect,
    radius: i32,
    color: [u8; 4],
) {
    let inner = PixelRect::new(rect.x + 2, rect.y + 2, rect.width - 4, rect.height - 4);
    let min_x = rect.x.max(0);
    let max_x = rect.right().min(size as i32);
    let min_y = rect.y.max(0);
    let max_y = rect.bottom().min(size as i32);
    for y in min_y..max_y {
        for x in min_x..max_x {
            if rounded_rect_contains(rect, radius, x, y)
                && !rounded_rect_contains(inner, radius.saturating_sub(2), x, y)
            {
                blend_pixel(pixels, size, x, y, color);
            }
        }
    }
}

fn rounded_rect_contains(rect: PixelRect, radius: i32, x: i32, y: i32) -> bool {
    if radius <= 0 {
        return x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom();
    }
    let left = rect.x + radius;
    let right = rect.right() - radius - 1;
    let top = rect.y + radius;
    let bottom = rect.bottom() - radius - 1;
    let corner_x = if x < left {
        left
    } else if x > right {
        right
    } else {
        x
    };
    let corner_y = if y < top {
        top
    } else if y > bottom {
        bottom
    } else {
        y
    };
    let dx = x - corner_x;
    let dy = y - corner_y;
    dx * dx + dy * dy <= radius * radius
}

fn draw_circle(pixels: &mut [u8], size: u32, cx: i32, cy: i32, radius: i32, color: [u8; 4]) {
    let r2 = radius * radius;
    for y in (cy - radius).max(0)..(cy + radius + 1).min(size as i32) {
        for x in (cx - radius).max(0)..(cx + radius + 1).min(size as i32) {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                blend_pixel(pixels, size, x, y, color);
            }
        }
    }
}

fn draw_circle_outline(
    pixels: &mut [u8],
    size: u32,
    cx: i32,
    cy: i32,
    radius: i32,
    color: [u8; 4],
) {
    let outer = radius * radius;
    let inner = (radius - 3).max(0) * (radius - 3).max(0);
    for y in (cy - radius).max(0)..(cy + radius + 1).min(size as i32) {
        for x in (cx - radius).max(0)..(cx + radius + 1).min(size as i32) {
            let dx = x - cx;
            let dy = y - cy;
            let d2 = dx * dx + dy * dy;
            if d2 <= outer && d2 >= inner {
                blend_pixel(pixels, size, x, y, color);
            }
        }
    }
}

fn draw_triangle(
    pixels: &mut [u8],
    size: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: [u8; 4],
) {
    let min_x = x0.min(x1).min(x2).max(0);
    let max_x = x0.max(x1).max(x2).min(size as i32 - 1);
    let min_y = y0.min(y1).min(y2).max(0);
    let max_y = y0.max(y1).max(y2).min(size as i32 - 1);
    let area = edge(x0, y0, x1, y1, x2, y2);
    if area == 0 {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let w0 = edge(x1, y1, x2, y2, x, y);
            let w1 = edge(x2, y2, x0, y0, x, y);
            let w2 = edge(x0, y0, x1, y1, x, y);
            if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                blend_pixel(pixels, size, x, y, color);
            }
        }
    }
}

fn edge(x0: i32, y0: i32, x1: i32, y1: i32, x: i32, y: i32) -> i32 {
    (x - x0) * (y1 - y0) - (y - y0) * (x1 - x0)
}

fn draw_digit_label(
    pixels: &mut [u8],
    size: u32,
    label: &str,
    center_x: i32,
    center_y: i32,
    scale: i32,
    color: [u8; 4],
) {
    let char_width = 3 * scale;
    let gap = scale;
    let total_width = label.chars().count() as i32 * char_width
        + label.chars().count().saturating_sub(1) as i32 * gap;
    let start_x = center_x - total_width / 2;
    let start_y = center_y - (5 * scale) / 2;
    for (index, ch) in label.chars().enumerate() {
        let x = start_x + index as i32 * (char_width + gap);
        draw_digit_char(pixels, size, ch, x, start_y, scale, color);
    }
}

fn draw_digit_char(
    pixels: &mut [u8],
    size: u32,
    ch: char,
    x: i32,
    y: i32,
    scale: i32,
    color: [u8; 4],
) {
    let Some(pattern) = digit_pattern(ch) else {
        return;
    };
    for (row, bits) in pattern.iter().enumerate() {
        for (col, byte) in bits.as_bytes().iter().enumerate() {
            if *byte == b'1' {
                draw_rounded_rect(
                    pixels,
                    size,
                    PixelRect::new(x + col as i32 * scale, y + row as i32 * scale, scale, scale),
                    1,
                    color,
                );
            }
        }
    }
}

fn digit_pattern(ch: char) -> Option<[&'static str; 5]> {
    match ch {
        '0' => Some(["111", "101", "101", "101", "111"]),
        '1' => Some(["010", "110", "010", "010", "111"]),
        '2' => Some(["111", "001", "111", "100", "111"]),
        '3' => Some(["111", "001", "111", "001", "111"]),
        '4' => Some(["101", "101", "111", "001", "001"]),
        '5' => Some(["111", "100", "111", "001", "111"]),
        '6' => Some(["111", "100", "111", "101", "111"]),
        '7' => Some(["111", "001", "010", "010", "010"]),
        '8' => Some(["111", "101", "111", "101", "111"]),
        '9' => Some(["111", "101", "111", "001", "111"]),
        '+' => Some(["010", "010", "111", "010", "010"]),
        _ => None,
    }
}

fn blend_pixel(pixels: &mut [u8], size: u32, x: i32, y: i32, src: [u8; 4]) {
    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 || src[3] == 0 {
        return;
    }
    let offset = ((y as u32 * size + x as u32) * 4) as usize;
    let src_a = src[3] as u16;
    let inv_a = 255 - src_a;
    let dst_a = pixels[offset + 3] as u16;
    for channel in 0..3 {
        let dst = pixels[offset + channel] as u16;
        pixels[offset + channel] = ((src[channel] as u16 * src_a + dst * inv_a) / 255) as u8;
    }
    pixels[offset + 3] = (src_a + dst_a * inv_a / 255).min(255) as u8;
}

fn external_drag_paths_from_typed_data(value: &dyn TypedData) -> Result<Vec<PathBuf>, String> {
    if !value
        .type_()
        .hint()
        .is_some_and(|hint| TypeHint::UriList.matches(&hint))
    {
        return Err("received non-uri-list data".to_string());
    }
    let uris = value
        .try_as_uris()
        .map_err(|error| format!("read uri-list: {error}"))?;
    Ok(external_drag_paths_from_uris(uris))
}

fn external_drag_paths_from_uris(uris: Vec<String>) -> Vec<PathBuf> {
    let text = uris.join("\n");
    decode_file_clipboard_text(&text)
        .map(|payload| payload.paths)
        .unwrap_or_default()
}

fn external_drag_drop_sources(
    event_paths: Vec<PathBuf>,
    tracked_sources: Option<Vec<PathBuf>>,
) -> Vec<PathBuf> {
    if event_paths.is_empty() {
        tracked_sources.unwrap_or_default()
    } else {
        event_paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_sources_prefer_drop_event_paths() {
        let event_paths = vec![PathBuf::from("/tmp/drop.txt")];
        let tracked = Some(vec![PathBuf::from("/tmp/enter.txt")]);

        assert_eq!(
            external_drag_drop_sources(event_paths, tracked),
            vec![PathBuf::from("/tmp/drop.txt")]
        );
    }

    #[test]
    fn drop_sources_fall_back_to_tracked_enter_paths() {
        let tracked = Some(vec![PathBuf::from("/tmp/enter.txt")]);

        assert_eq!(
            external_drag_drop_sources(Vec::new(), tracked),
            vec![PathBuf::from("/tmp/enter.txt")]
        );
    }

    #[test]
    fn uri_list_data_decodes_file_paths() {
        assert_eq!(
            external_drag_paths_from_uris(vec!["file:///tmp/a%20file.txt".to_string()]),
            vec![PathBuf::from("/tmp/a file.txt")]
        );
    }

    #[test]
    fn outgoing_payload_advertises_uri_list() {
        let payload = outgoing_dnd_payload(&[PathBuf::from("/tmp/a file.txt")]);

        assert_eq!(payload.uris, vec!["file:///tmp/a%20file.txt".to_string()]);
        assert_eq!(payload.text, "file:///tmp/a%20file.txt");
    }

    #[test]
    fn outgoing_preview_pixels_are_sized_and_nonblank() {
        let metrics = outgoing_dnd_preview_metrics(128, 1.0);
        let pixels = outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, None);

        assert_eq!(metrics.canvas_size, 188);
        assert_eq!(
            pixels.len(),
            (metrics.canvas_size * metrics.canvas_size * 4) as usize
        );
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn outgoing_preview_metrics_follow_item_icon_size() {
        let metrics = outgoing_dnd_preview_metrics(64, 1.0);

        assert_eq!(metrics.icon_size, 64);
        assert_eq!(metrics.outline, 30);
        assert_eq!(metrics.canvas_size, 124);
    }

    #[test]
    fn outgoing_preview_icon_size_preserves_scaled_source_size() {
        let source = ShellInternalDragPreviewSource::Place {
            icon_name: "folder".to_string(),
            icon_size: 512.0,
        };

        assert_eq!(outgoing_dnd_preview_icon_size(Some(&source), 2.0), 512);
    }

    #[test]
    fn outgoing_preview_pixels_add_badge_for_multiple_paths() {
        let metrics = outgoing_dnd_preview_metrics(128, 1.0);
        let single = outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, None);
        let multiple = outgoing_dnd_preview_pixels(
            &[PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
            metrics,
            None,
        );

        assert_ne!(single, multiple);
    }

    #[test]
    fn outgoing_preview_pixels_use_supplied_icon_raster() {
        let metrics = outgoing_dnd_preview_metrics(128, 1.0);
        let raster = solid_test_raster(metrics.icon_size, [210, 32, 40, 255]);
        let preview = OutgoingDndPreviewRaster { icon: raster };
        let pixels =
            outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, Some(&preview));
        let center = metrics.outline as u32 + metrics.icon_size / 2;
        let offset = ((center * metrics.canvas_size + center) * 4) as usize;

        assert!(pixels[offset] > 160);
        assert!(pixels[offset + 1] < 80);
        assert!(pixels[offset + 2] < 90);
        assert!(pixels[offset + 3] > 180);
    }

    #[cfg(unix)]
    #[test]
    fn drag_emblem_kinds_include_link_for_symlink() {
        let dir = std::env::temp_dir().join(format!("fika-dnd-link-emblem-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        fs::write(&target, "x").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(icon_emblem_kinds_for_path(&link).contains(&crate::IconEmblemKind::Link));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn drag_emblem_kinds_skip_marker_for_readable_unwritable_file() {
        let dir =
            std::env::temp_dir().join(format!("fika-dnd-readonly-emblem-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("readonly.txt");
        fs::write(&path, "x").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o444);
        fs::set_permissions(&path, permissions).unwrap();

        assert!(icon_emblem_kinds_for_path(&path).is_empty());

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&path, permissions).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn drag_emblem_kinds_prefer_locked_for_unreadable_file() {
        let dir =
            std::env::temp_dir().join(format!("fika-dnd-unreadable-emblem-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("unreadable.txt");
        fs::write(&path, "x").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o000);
        fs::set_permissions(&path, permissions).unwrap();

        let emblems = icon_emblem_kinds_for_path(&path);
        assert!(emblems.contains(&crate::IconEmblemKind::Unreadable));

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&path, permissions).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    fn solid_test_raster(size: u32, color: [u8; 4]) -> IconRaster {
        let mut pixels = vec![0; (size * size * 4) as usize];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
        IconRaster {
            pixels: Arc::from(pixels),
            width: size,
            height: size,
        }
    }
}
