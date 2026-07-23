#[derive(Clone, Copy, Debug, PartialEq)]
struct OutgoingDndPreviewMetrics {
    canvas_width: u32,
    canvas_height: u32,
    icon_size: u32,
    outline: i32,
    buffer_scale: i32,
    icon_rect: PixelRect,
    label_rect: Option<PixelRect>,
}

#[derive(Clone, Debug)]
struct OutgoingDndPreviewLabelRaster {
    alpha: Arc<[u8]>,
    width: u32,
    height: u32,
}

impl OutgoingDndPreviewMetrics {
    fn with_label(mut self, has_label: bool) -> Self {
        if !has_label {
            return self;
        }
        let minimum_width =
            scaled_preview_dimension(DND_PREVIEW_LABEL_MIN_WIDTH, self.buffer_scale);
        self.canvas_width = self.canvas_width.max(minimum_width);
        self.canvas_width = align_preview_dimension(self.canvas_width, self.buffer_scale);
        self.icon_rect.x = (self.canvas_width as i32 - self.icon_size as i32) / 2;
        let label_height =
            scaled_preview_dimension(DND_PREVIEW_LABEL_HEIGHT, self.buffer_scale);
        self.label_rect = Some(PixelRect::new(
            0,
            self.canvas_height as i32,
            self.canvas_width as i32,
            label_height as i32,
        ));
        self.canvas_height = align_preview_dimension(
            self.canvas_height.saturating_add(label_height),
            self.buffer_scale,
        );
        self
    }
}

fn outgoing_dnd_preview_icon_size(
    source: Option<&ShellInternalDragPreviewSource>,
    scale: f32,
) -> u32 {
    let unit = normalized_scale_factor(scale).max(1.0);
    match source {
        Some(ShellInternalDragPreviewSource::PaneItem { icon_size, .. })
        | Some(ShellInternalDragPreviewSource::Place { icon_size, .. }) => {
            icon_size.round().clamp(16.0 * unit, 256.0 * unit) as u32
        }
        None => (DEEPIN_DND_ICON_SIZE * unit).round() as u32,
    }
}

fn outgoing_dnd_preview_metrics(icon_size: u32, scale: f32) -> OutgoingDndPreviewMetrics {
    let logical_scale = normalized_scale_factor(scale).max(1.0);
    let buffer_scale = logical_scale.round().max(1.0) as i32;
    let logical_icon_size = (icon_size as f32 / logical_scale).clamp(16.0, 256.0);
    let icon_size = scaled_preview_dimension(logical_icon_size, buffer_scale);
    let outline = scaled_preview_dimension(DND_PREVIEW_ICON_OUTLINE, buffer_scale) as i32;
    let canvas_width = align_preview_dimension(
        icon_size.saturating_add(outline.max(0) as u32 * 2),
        buffer_scale,
    );
    OutgoingDndPreviewMetrics {
        canvas_width,
        canvas_height: canvas_width,
        icon_size,
        outline,
        buffer_scale,
        icon_rect: PixelRect::new(
            (canvas_width as i32 - icon_size as i32) / 2,
            outline,
            icon_size as i32,
            icon_size as i32,
        ),
        label_rect: None,
    }
}

#[cfg(test)]
fn outgoing_dnd_preview_pixels(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    raster: Option<&OutgoingDndPreviewRaster>,
) -> Vec<u8> {
    outgoing_dnd_preview_pixels_with_label(paths, metrics, raster, None, [55, 120, 210, 230])
}

fn outgoing_dnd_preview_pixels_with_label(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    raster: Option<&OutgoingDndPreviewRaster>,
    label: Option<&OutgoingDndPreviewLabelRaster>,
    label_color: [u8; 4],
) -> Vec<u8> {
    let mut pixels = vec![0; (metrics.canvas_width * metrics.canvas_height * 4) as usize];
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
    let icon_rect = metrics.icon_rect;
    let ghost_count = count.saturating_sub(1).min(DEEPIN_DND_ICON_MAX - 1);
    for index in (0..ghost_count).rev() {
        let opacity = 1.0 - (index as f32 + 5.0) * DEEPIN_DND_ICON_OPACITY;
        draw_raster_rotated(
            &mut pixels,
            metrics.canvas_width,
            metrics.canvas_height,
            raster,
            icon_rect,
            deepin_drag_icon_rotation(index),
            opacity,
        );
    }
    draw_raster_rotated(
        &mut pixels,
        metrics.canvas_width,
        metrics.canvas_height,
        raster,
        icon_rect,
        0.0,
        0.8,
    );
    if count > 1 {
        draw_count_badge(&mut pixels, metrics, count);
    }
    if let (Some(label_rect), Some(label)) = (metrics.label_rect, label) {
        draw_label_background(&mut pixels, metrics, label_rect, label_color);
        draw_label_alpha(&mut pixels, metrics, label_rect, label);
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

fn draw_count_badge(pixels: &mut [u8], metrics: OutgoingDndPreviewMetrics, count: usize) {
    let length = if count > DEEPIN_DND_ICON_MAX_COUNT {
        28.0
    } else {
        24.0
    };
    let scale = metrics.buffer_scale as f32;
    let radius = (length * scale / 2.0).round() as i32;
    let inset = (10.0 * scale).round() as i32;
    let center_x = metrics.icon_rect.right() - inset;
    let center_y = metrics.icon_rect.bottom() - inset;
    draw_circle(
        pixels,
        metrics.canvas_width,
        center_x,
        center_y,
        radius,
        [244, 74, 74, 255],
    );
    draw_circle_outline(
        pixels,
        metrics.canvas_width,
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
        metrics.canvas_width,
        &label,
        center_x,
        center_y,
        (digit_scale * scale).round().max(2.0) as i32,
        [255, 255, 255, 255],
    );
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

fn draw_raster_rotated(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
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
    let max_x = (center_x as i32 + radius + 1).min(canvas_width as i32);
    let min_y = (center_y as i32 - radius).max(0);
    let max_y = (center_y as i32 + radius + 1).min(canvas_height as i32);
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
                blend_pixel_rect(pixels, canvas_width, canvas_height, x, y, color);
            }
        }
    }
}

fn draw_label_background(
    pixels: &mut [u8],
    metrics: OutgoingDndPreviewMetrics,
    rect: PixelRect,
    color: [u8; 4],
) {
    draw_rounded_rect_rect(
        pixels,
        metrics.canvas_width,
        metrics.canvas_height,
        rect,
        (4 * metrics.buffer_scale).max(1),
        color,
    );
}

fn draw_label_alpha(
    pixels: &mut [u8],
    metrics: OutgoingDndPreviewMetrics,
    rect: PixelRect,
    label: &OutgoingDndPreviewLabelRaster,
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
                metrics.canvas_width,
                metrics.canvas_height,
                rect.x + x as i32,
                rect.y + y as i32,
                [255, 255, 255, alpha],
            );
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
