#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutgoingDndPreviewKind {
    DolphinItem,
    DolphinGrid {
        columns: usize,
        item_count: usize,
        stride: i32,
    },
}

const DND_FALLBACK_LABEL_WIDTH: f32 = 160.0;
const DND_FALLBACK_LABEL_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct OutgoingDndPreviewMetrics {
    canvas_width: u32,
    canvas_height: u32,
    /// Physical pixel size used when painting the drag surface.
    icon_size: u32,
    /// Scene/logical icon size used for thumbnail and theme-icon cache keys.
    /// Matches the live item view so ready thumbnails are reused (Dolphin's
    /// `iconPixmap` path) instead of falling back to a MIME icon.
    cache_icon_size: f32,
    buffer_scale: i32,
    icon_rect: PixelRect,
    label_rect: Option<PixelRect>,
    background_rect: Option<PixelRect>,
    background_radius: i32,
    label_style: Option<DragPreviewLabelStyle>,
    /// Hotspot in buffer pixels (for debugging / scale checks).
    hotspot_x: i32,
    hotspot_y: i32,
    /// Drag-icon offset in logical surface coordinates (negated hotspot).
    /// Matches `wayland-client-runtime::DndIcon` / `wl_surface.offset`.
    hotspot_logical_x: i32,
    hotspot_logical_y: i32,
    kind: OutgoingDndPreviewKind,
    background_color: [u8; 4],
}

#[derive(Clone, Debug)]
struct OutgoingDndPreviewLabelRaster {
    alpha: Arc<[u8]>,
    width: u32,
    height: u32,
}

impl OutgoingDndPreviewMetrics {
    fn visible_icon_count(self) -> usize {
        match self.kind {
            OutgoingDndPreviewKind::DolphinItem => 1,
            OutgoingDndPreviewKind::DolphinGrid { item_count, .. } => item_count,
        }
    }

    fn icon_rect_at(self, index: usize) -> Option<PixelRect> {
        match self.kind {
            OutgoingDndPreviewKind::DolphinItem if index == 0 => Some(self.icon_rect),
            OutgoingDndPreviewKind::DolphinGrid {
                columns,
                item_count,
                stride,
            } if index < item_count => {
                let column = index % columns;
                let row = index / columns;
                Some(PixelRect::new(
                    column as i32 * stride,
                    row as i32 * stride,
                    self.icon_size as i32,
                    self.icon_size as i32,
                ))
            }
            _ => None,
        }
    }
}

fn outgoing_dnd_preview_metrics_for_layout(
    layout: SingleDragPreviewLayout,
    scale: f32,
) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let factor = buffer_scale as f32 / logical_scale;
    let mut canvas_width = (layout.bounds.width * factor).round().max(1.0) as u32;
    let mut canvas_height = (layout.bounds.height * factor).round().max(1.0) as u32;
    canvas_width = align_preview_dimension(canvas_width, buffer_scale);
    canvas_height = align_preview_dimension(canvas_height, buffer_scale);
    let icon_rect = map_preview_rect(layout.icon, factor);
    let label_rect = layout
        .label
        .map(|label| map_preview_rect(label.rect, factor));
    let background_rect = Some(map_preview_rect(layout.background, factor));
    // Pane items: Dolphin `setHotSpot((pixmap.width()/dpr)/2, 0)` after the
    // final pixmap exists. Places: Qt `QAbstractItemView` press-relative hotspot
    // (`pressedPosition - rect.topLeft()`), converted to surface-local units.
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        if layout.view_mode.is_some() {
            dolphin_pane_hotspot(canvas_width, buffer_scale)
        } else {
            places_press_hotspot(layout.hotspot, logical_scale, buffer_scale)
        };
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height,
        icon_size: icon_rect.width.max(icon_rect.height) as u32,
        // Layout icon size is already in scene units (same as the live item).
        cache_icon_size: layout.icon.width.max(layout.icon.height).clamp(16.0, 256.0),
        buffer_scale,
        icon_rect,
        label_rect,
        background_rect,
        background_radius: (layout.radius * factor).round() as i32,
        label_style: layout.label.map(|label| label.style),
        hotspot_x,
        hotspot_y,
        hotspot_logical_x,
        hotspot_logical_y,
        kind: OutgoingDndPreviewKind::DolphinItem,
        background_color: [0, 0, 0, 0],
    }
}

fn outgoing_dnd_preview_metrics_for_multi_layout(
    layout: MultiDragPreviewLayout,
    scale: f32,
) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let factor = buffer_scale as f32 / logical_scale;
    let mut canvas_width = (layout.bounds.width * factor).round().max(1.0) as u32;
    let mut canvas_height = (layout.bounds.height * factor).round().max(1.0) as u32;
    canvas_width = align_preview_dimension(canvas_width, buffer_scale);
    canvas_height = align_preview_dimension(canvas_height, buffer_scale);
    let icon_rect = layout
        .cell_rect(0)
        .map(|cell| map_preview_rect(cell, factor))
        .unwrap_or_else(|| PixelRect::new(0, 0, buffer_scale, buffer_scale));
    let stride = ((layout.icon_size + layout.gap) * factor).round().max(1.0) as i32;
    // Multi-item path also uses Dolphin top-centre on the final grid pixmap.
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        dolphin_pane_hotspot(canvas_width, buffer_scale);
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height,
        icon_size: icon_rect.width.max(icon_rect.height) as u32,
        // Multi-item grid keeps Dolphin's fixed logical cell size for cache keys.
        cache_icon_size: layout.icon_size.clamp(16.0, 256.0),
        buffer_scale,
        icon_rect,
        label_rect: None,
        background_rect: None,
        background_radius: 0,
        label_style: None,
        hotspot_x,
        hotspot_y,
        hotspot_logical_x,
        hotspot_logical_y,
        kind: OutgoingDndPreviewKind::DolphinGrid {
            columns: layout.columns,
            item_count: layout.item_count,
            stride,
        },
        background_color: [0, 0, 0, 0],
    }
}

/// Dolphin `KItemListController::startDragging`:
/// `hotSpot = QPoint((pixmap.width() / dpr) / 2, 0)`.
///
/// `pixmap.width` is buffer pixels; `dpr` is the pixmap device pixel ratio
/// (`buffer_scale` here). Result is surface-local logical coords; Qt Wayland
/// then applies `addAttachOffset(-hotSpot)`.
fn dolphin_pane_hotspot(canvas_width: u32, buffer_scale: i32) -> (i32, i32, i32, i32) {
    let scale = buffer_scale.max(1);
    let logical_width = canvas_width as i32 / scale;
    let hotspot_logical_x = logical_width / 2;
    let hotspot_x = hotspot_logical_x * scale;
    (hotspot_x, 0, hotspot_logical_x, 0)
}

/// Places / `QAbstractItemView::startDrag`:
/// `setHotSpot(pressedPosition - rect.topLeft())` in widget (logical) pixels.
///
/// Our place row layout is in scene/window-physical units, so convert with
/// `ui_scale` into surface-local logical coords for `wl_surface.offset`.
fn places_press_hotspot(
    hotspot: fika_core::ViewPoint,
    logical_scale: f32,
    buffer_scale: i32,
) -> (i32, i32, i32, i32) {
    let scale = buffer_scale.max(1) as f32;
    let factor = scale / logical_scale.max(1.0);
    let hotspot_logical_x = (hotspot.x / logical_scale.max(1.0)).round() as i32;
    let hotspot_logical_y = (hotspot.y / logical_scale.max(1.0)).round() as i32;
    let hotspot_x = (hotspot.x * factor).round() as i32;
    let hotspot_y = (hotspot.y * factor).round() as i32;
    (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y)
}

fn map_preview_rect(rect: fika_core::ViewRect, factor: f32) -> PixelRect {
    let x = (rect.x * factor).round() as i32;
    let y = (rect.y * factor).round() as i32;
    let right = (rect.right() * factor).round() as i32;
    let bottom = (rect.bottom() * factor).round() as i32;
    PixelRect::new(x, y, (right - x).max(1), (bottom - y).max(1))
}

fn outgoing_dnd_fallback_preview_metrics(scale: f32) -> OutgoingDndPreviewMetrics {
    let icon_size = (DND_FALLBACK_ICON_SIZE * normalized_scale_factor(scale).max(1.0)) as u32;
    let mut metrics = outgoing_dnd_preview_metrics(icon_size, scale);
    let label_width = scaled_preview_dimension(DND_FALLBACK_LABEL_WIDTH, metrics.buffer_scale);
    let label_height = scaled_preview_dimension(DND_FALLBACK_LABEL_HEIGHT, metrics.buffer_scale);
    metrics.canvas_width = metrics.canvas_width.max(label_width);
    metrics.canvas_width = align_preview_dimension(metrics.canvas_width, metrics.buffer_scale);
    metrics.icon_rect.x = (metrics.canvas_width as i32 - metrics.icon_size as i32) / 2;
    let icon_bottom = metrics.canvas_height;
    metrics.label_rect = Some(PixelRect::new(
        0,
        icon_bottom as i32,
        metrics.canvas_width as i32,
        label_height as i32,
    ));
    metrics.canvas_height = align_preview_dimension(
        icon_bottom.saturating_add(label_height),
        metrics.buffer_scale,
    );
    metrics.label_style = Some(DragPreviewLabelStyle::PlainSingleLine);
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        dolphin_pane_hotspot(metrics.canvas_width, metrics.buffer_scale);
    metrics.hotspot_x = hotspot_x;
    metrics.hotspot_y = hotspot_y;
    metrics.hotspot_logical_x = hotspot_logical_x;
    metrics.hotspot_logical_y = hotspot_logical_y;
    metrics
}

fn outgoing_dnd_preview_metrics(icon_size: u32, scale: f32) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let logical_icon_size = (icon_size as f32 / logical_scale).clamp(16.0, 256.0);
    let icon_size = scaled_preview_dimension(logical_icon_size, buffer_scale);
    let canvas_width = align_preview_dimension(icon_size, buffer_scale);
    let (hotspot_x, hotspot_y, hotspot_logical_x, hotspot_logical_y) =
        dolphin_pane_hotspot(canvas_width, buffer_scale);
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height: canvas_width,
        icon_size,
        cache_icon_size: logical_icon_size,
        buffer_scale,
        icon_rect: PixelRect::new(
            0,
            0,
            icon_size as i32,
            icon_size as i32,
        ),
        label_rect: None,
        background_rect: None,
        background_radius: 0,
        label_style: None,
        hotspot_x,
        hotspot_y,
        hotspot_logical_x,
        hotspot_logical_y,
        kind: OutgoingDndPreviewKind::DolphinItem,
        background_color: [0, 0, 0, 0],
    }
}

#[cfg(test)]
fn outgoing_dnd_preview_pixels(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    rasters: Option<&OutgoingDndPreviewRasters>,
) -> Vec<u8> {
    outgoing_dnd_preview_pixels_with_label(paths, metrics, rasters, None, [55, 120, 210, 230])
}

fn outgoing_dnd_preview_pixels_with_label(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    rasters: Option<&OutgoingDndPreviewRasters>,
    label: Option<&OutgoingDndPreviewLabelRaster>,
    label_color: [u8; 4],
) -> Vec<u8> {
    let mut pixels = vec![0; (metrics.canvas_width * metrics.canvas_height * 4) as usize];
    if metrics.kind == OutgoingDndPreviewKind::DolphinItem {
        if let Some(background) = metrics.background_rect {
            draw_rounded_rect_rect(
                &mut pixels,
                metrics.canvas_width,
                metrics.canvas_height,
                background,
                metrics.background_radius,
                metrics.background_color,
            );
        }
        let is_dir = paths.first().is_some_and(|path| path.is_dir());
        let fallback;
        let icon = match rasters.and_then(|rasters| rasters.icon(0)) {
            Some(raster) => raster,
            None => {
                fallback = fallback_drag_icon_raster(is_dir, metrics.icon_size.max(1));
                &fallback
            }
        };
        draw_raster_scaled_rect(
            &mut pixels,
            metrics.canvas_width,
            metrics.canvas_height,
            icon,
            metrics.icon_rect,
            1.0,
        );
        if let (Some(label_rect), Some(label)) = (metrics.label_rect, label) {
            draw_label_alpha(
                &mut pixels,
                metrics.canvas_width,
                metrics.canvas_height,
                label_rect,
                label,
                label_color,
            );
        }
        return pixels;
    }
    for index in 0..metrics.visible_icon_count() {
        let Some(icon_rect) = metrics.icon_rect_at(index) else {
            continue;
        };
        let fallback;
        let raster = match rasters.and_then(|rasters| rasters.icon(index)) {
            Some(raster) => raster,
            None => {
                let is_dir = paths.get(index).is_some_and(|path| path.is_dir());
                fallback = fallback_drag_icon_raster(is_dir, metrics.icon_size.max(1));
                &fallback
            }
        };
        draw_raster_scaled_rect(
            &mut pixels,
            metrics.canvas_width,
            metrics.canvas_height,
            raster,
            icon_rect,
            1.0,
        );
    }
    pixels
}

fn scaled_preview_dimension(logical: f32, buffer_scale: i32) -> u32 {
    let scale = buffer_scale.max(1) as f32;
    align_preview_dimension((logical.max(1.0) * scale).round().max(1.0) as u32, buffer_scale)
}

fn align_preview_dimension(value: u32, buffer_scale: i32) -> u32 {
    let scale = buffer_scale.max(1) as u32;
    value.max(scale).div_ceil(scale) * scale
}

fn fallback_drag_icon_raster(is_dir: bool, size: u32) -> IconRaster {
    let mut pixels = vec![0; (size * size * 4) as usize];
    let unit = size as f32 / DND_FALLBACK_ICON_SIZE;
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
        [
            [fold_rect.x, fold_rect.y],
            [fold_rect.right(), fold_rect.y],
            [fold_rect.right(), fold_rect.bottom()],
        ],
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

fn draw_label_alpha(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    rect: PixelRect,
    label: &OutgoingDndPreviewLabelRaster,
    color: [u8; 4],
) {
    let width = rect.width.min(label.width as i32).max(0) as u32;
    let height = rect.height.min(label.height as i32).max(0) as u32;
    for y in 0..height {
        for x in 0..width {
            let alpha = label.alpha[(y * label.width + x) as usize];
            if alpha == 0 {
                continue;
            }
            blend_pixel_rect(
                pixels,
                canvas_width,
                canvas_height,
                rect.x + x as i32,
                rect.y + y as i32,
                [color[0], color[1], color[2], ((alpha as u16 * color[3] as u16) / 255) as u8],
            );
        }
    }
}

fn draw_raster_scaled_rect(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    raster: &IconRaster,
    rect: PixelRect,
    opacity: f32,
) {
    if opacity <= 0.0 || raster.width == 0 || raster.height == 0 {
        return;
    }
    let min_x = rect.x.max(0);
    let max_x = rect.right().min(canvas_width as i32);
    let min_y = rect.y.max(0);
    let max_y = rect.bottom().min(canvas_height as i32);
    for y in min_y..max_y {
        for x in min_x..max_x {
            let local_x = (x - rect.x) as f32 + 0.5;
            let local_y = (y - rect.y) as f32 + 0.5;
            let source_x = local_x / rect.width as f32 * (raster.width as f32 - 1.0);
            let source_y = local_y / rect.height as f32 * (raster.height as f32 - 1.0);
            if let Some(color) = sample_raster_bilinear(raster, source_x, source_y, opacity) {
                blend_pixel_rect(pixels, canvas_width, canvas_height, x, y, color);
            }
        }
    }
}

fn draw_rounded_rect_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: PixelRect,
    radius: i32,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    let radius = radius.max(0).min(rect.width / 2).min(rect.height / 2);
    let min_x = rect.x.max(0);
    let max_x = rect.right().min(width as i32);
    let min_y = rect.y.max(0);
    let max_y = rect.bottom().min(height as i32);
    for y in min_y..max_y {
        for x in min_x..max_x {
            if rounded_rect_contains(rect, radius, x, y) {
                blend_pixel_rect(pixels, width, height, x, y, color);
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
    draw_raster_scaled_rect(pixels, canvas_size, canvas_size, raster, rect, opacity);
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

fn draw_triangle(pixels: &mut [u8], size: u32, points: [[i32; 2]; 3], color: [u8; 4]) {
    let [[x0, y0], [x1, y1], [x2, y2]] = points;
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

fn blend_pixel(pixels: &mut [u8], size: u32, x: i32, y: i32, src: [u8; 4]) {
    blend_pixel_rect(pixels, size, size, x, y, src);
}

fn blend_pixel_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    src: [u8; 4],
) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 || src[3] == 0 {
        return;
    }
    let offset = ((y as u32 * width + x as u32) * 4) as usize;
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
