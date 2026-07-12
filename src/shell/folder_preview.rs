use std::path::Path;
use std::sync::Arc;

use crate::IconRaster;
use crate::shell::metrics::DOLPHIN_FOLDER_PREVIEW_MAX_IMAGES;
use crate::shell::render::gpu::hash_bytes_with_len;

pub(crate) const FOLDER_PREVIEW_LAYOUT_VERSION: u64 = 2;

#[derive(Clone, Debug)]
struct PreparedFolderPreviewThumbnail {
    image: image::RgbaImage,
}

pub(crate) fn folder_preview_thumbnail_raster_from_children(
    rasters: &[IconRaster],
    target_size: u32,
    seed: u64,
) -> Option<IconRaster> {
    let target_size = target_size.clamp(16, 256);
    if rasters.is_empty() {
        return None;
    }
    let layout = DolphinDirectoryPreviewLayout::new(target_size)?;
    let thumbnails = rasters
        .iter()
        .filter_map(prepare_folder_preview_thumbnail)
        .collect::<Vec<_>>();
    if thumbnails.is_empty() {
        return None;
    }
    let mut pixels = vec![0; (target_size * target_size * 4) as usize];
    let slots = folder_preview_thumbnail_slots(thumbnails.len(), layout);
    for (index, (thumbnail, slot)) in thumbnails.iter().zip(slots.iter()).enumerate() {
        paint_dolphin_directory_subthumbnail(
            thumbnail,
            *slot,
            &mut pixels,
            target_size,
            layout.border_stroke_width,
            folder_preview_thumbnail_angle(seed, index),
        );
    }
    Some(IconRaster {
        pixels: Arc::from(pixels),
        width: target_size,
        height: target_size,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DolphinDirectoryPreviewLayout {
    pub(crate) folder_size: u32,
    pub(crate) top_margin: u32,
    pub(crate) bottom_margin: u32,
    pub(crate) left_margin: u32,
    pub(crate) right_margin: u32,
    pub(crate) spacing: u32,
    pub(crate) segment_width: u32,
    pub(crate) segment_height: u32,
    pub(crate) border_stroke_width: u32,
}

impl DolphinDirectoryPreviewLayout {
    pub(crate) fn new(folder_size: u32) -> Option<Self> {
        let folder_size = folder_size.clamp(16, 256);
        let spacing = 1;
        let tiles = 2;
        let top_margin = folder_size * 30 / 100;
        let bottom_margin = folder_size / 6;
        let left_margin = folder_size / 13;
        let right_margin = left_margin;
        let segment_width = (folder_size - left_margin - right_margin + spacing) / tiles - spacing;
        let segment_height = (folder_size - top_margin - bottom_margin + spacing) / tiles - spacing;
        if segment_width < 5 || segment_height < 5 {
            return None;
        }
        let border_stroke_width = ((folder_size as f32 / 170.0) + 0.5).floor() as u32;
        Some(Self {
            folder_size,
            top_margin,
            bottom_margin,
            left_margin,
            right_margin,
            spacing,
            segment_width,
            segment_height,
            border_stroke_width,
        })
    }

    pub(crate) fn one_tile_slot(self) -> FolderPreviewThumbnailSlot {
        FolderPreviewThumbnailSlot {
            x: self.left_margin,
            y: self.top_margin,
            width: self
                .folder_size
                .saturating_sub(self.left_margin + self.right_margin),
            height: self
                .folder_size
                .saturating_sub(self.top_margin + self.bottom_margin),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FolderPreviewThumbnailSlot {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn folder_preview_thumbnail_slots(
    count: usize,
    layout: DolphinDirectoryPreviewLayout,
) -> Vec<FolderPreviewThumbnailSlot> {
    let count = count.min(DOLPHIN_FOLDER_PREVIEW_MAX_IMAGES);
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![layout.one_tile_slot()];
    }

    let row2_y = layout.top_margin + layout.segment_height + layout.spacing;
    if count == 3 {
        let available_width = layout
            .folder_size
            .saturating_sub(layout.left_margin + layout.right_margin);
        let centered_x =
            layout.left_margin + available_width.saturating_sub(layout.segment_width) / 2;
        return vec![
            FolderPreviewThumbnailSlot {
                x: layout.left_margin,
                y: layout.top_margin,
                width: layout.segment_width,
                height: layout.segment_height,
            },
            FolderPreviewThumbnailSlot {
                x: layout.left_margin + layout.segment_width + layout.spacing,
                y: layout.top_margin,
                width: layout.segment_width,
                height: layout.segment_height,
            },
            FolderPreviewThumbnailSlot {
                x: centered_x,
                y: row2_y,
                width: layout.segment_width,
                height: layout.segment_height,
            },
        ];
    }

    let mut slots = Vec::with_capacity(count);
    let mut x = layout.left_margin;
    let mut y = layout.top_margin;
    for _ in 0..count {
        slots.push(FolderPreviewThumbnailSlot {
            x,
            y,
            width: layout.segment_width,
            height: layout.segment_height,
        });
        x += layout.segment_width + layout.spacing;
        if x > layout.folder_size - layout.right_margin - layout.segment_width {
            x = layout.left_margin;
            y += layout.segment_height + layout.spacing;
        }
    }
    slots
}

fn prepare_folder_preview_thumbnail(raster: &IconRaster) -> Option<PreparedFolderPreviewThumbnail> {
    crop_icon_raster_to_alpha_bounds(raster).map(|image| PreparedFolderPreviewThumbnail { image })
}

fn paint_dolphin_directory_subthumbnail(
    thumbnail: &PreparedFolderPreviewThumbnail,
    slot: FolderPreviewThumbnailSlot,
    target: &mut [u8],
    target_size: u32,
    border_stroke_width: u32,
    rotation_angle: i32,
) {
    let framed = dolphin_directory_picture_frame(
        &thumbnail.image,
        slot.width,
        slot.height,
        border_stroke_width,
    );
    let rotated = rotate_rgba_image(&framed, rotation_angle);
    let radius = border_stroke_width.max(1);
    let center_x = slot.x as i32 + slot.width as i32 / 2;
    let center_y = slot.y as i32 + slot.height as i32 / 2;
    let draw_x = center_x - rotated.width() as i32 / 2;
    let draw_y = center_y - rotated.height() as i32 / 2;
    let mut shadow = shadow_from_alpha(&rotated, radius);
    blur_alpha_shadow(&mut shadow, radius);
    blend_image_over(
        target,
        target_size,
        &shadow,
        draw_x - radius as i32 / 2,
        draw_y - radius as i32 / 2,
    );
    blend_image_over(target, target_size, &rotated, draw_x, draw_y);
}

fn crop_icon_raster_to_alpha_bounds(raster: &IconRaster) -> Option<image::RgbaImage> {
    let source =
        image::RgbaImage::from_raw(raster.width, raster.height, raster.pixels.as_ref().to_vec())?;
    let mut min_x = raster.width;
    let mut min_y = raster.height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;
    for y in 0..raster.height {
        for x in 0..raster.width {
            if source.get_pixel(x, y)[3] == 0 {
                continue;
            }
            found = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    found.then(|| {
        image::imageops::crop_imm(&source, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image()
    })
}

fn dolphin_directory_picture_frame(
    image: &image::RgbaImage,
    target_width: u32,
    target_height: u32,
    border_stroke_width: u32,
) -> image::RgbaImage {
    let width_with_frame = image.width() + border_stroke_width * 2;
    let height_with_frame = image.height() + border_stroke_width * 2;
    let scaling =
        if image.width() > image.height() && width_with_frame > target_width && target_width > 0 {
            target_width as f32 / width_with_frame as f32
        } else if height_with_frame > target_height && target_height > 0 {
            target_height as f32 / height_with_frame as f32
        } else {
            1.0
        };
    let draw_width = ((image.width() as f32 * scaling).round() as u32)
        .max(1)
        .min(target_width.max(1));
    let draw_height = ((image.height() as f32 * scaling).round() as u32)
        .max(1)
        .min(target_height.max(1));
    let resized = image::imageops::resize(
        image,
        draw_width,
        draw_height,
        image::imageops::FilterType::Lanczos3,
    );
    let frame_width = draw_width + border_stroke_width * 2;
    let frame_height = draw_height + border_stroke_width * 2;
    let mut framed = image::RgbaImage::from_pixel(
        frame_width.max(1),
        frame_height.max(1),
        image::Rgba([0, 0, 0, 0]),
    );
    if border_stroke_width > 0 && image_corners_are_opaque(image) {
        for pixel in framed.pixels_mut() {
            *pixel = image::Rgba([255, 255, 255, 255]);
        }
    }
    image::imageops::overlay(
        &mut framed,
        &resized,
        border_stroke_width as i64,
        border_stroke_width as i64,
    );
    framed
}

fn image_corners_are_opaque(image: &image::RgbaImage) -> bool {
    image.get_pixel(0, 0)[3] == 255
        && image.get_pixel(image.width() - 1, 0)[3] == 255
        && image.get_pixel(0, image.height() - 1)[3] == 255
        && image.get_pixel(image.width() - 1, image.height() - 1)[3] == 255
}

fn rotate_rgba_image(image: &image::RgbaImage, angle_degrees: i32) -> image::RgbaImage {
    if angle_degrees == 0 {
        return image.clone();
    }
    let radians = (angle_degrees as f32).to_radians();
    let sin = radians.sin();
    let cos = radians.cos();
    let src_w = image.width() as f32;
    let src_h = image.height() as f32;
    let dst_w = (src_w * cos.abs() + src_h * sin.abs()).ceil().max(1.0) as u32;
    let dst_h = (src_w * sin.abs() + src_h * cos.abs()).ceil().max(1.0) as u32;
    let src_cx = (src_w - 1.0) / 2.0;
    let src_cy = (src_h - 1.0) / 2.0;
    let dst_cx = (dst_w as f32 - 1.0) / 2.0;
    let dst_cy = (dst_h as f32 - 1.0) / 2.0;
    let mut rotated = image::RgbaImage::from_pixel(dst_w, dst_h, image::Rgba([0, 0, 0, 0]));
    for y in 0..dst_h {
        for x in 0..dst_w {
            let dx = x as f32 - dst_cx;
            let dy = y as f32 - dst_cy;
            let src_x = cos * dx + sin * dy + src_cx;
            let src_y = -sin * dx + cos * dy + src_cy;
            if src_x < 0.0 || src_y < 0.0 || src_x > src_w - 1.0 || src_y > src_h - 1.0 {
                continue;
            }
            rotated.put_pixel(x, y, sample_rgba_bilinear(image, src_x, src_y));
        }
    }
    rotated
}

fn sample_rgba_bilinear(image: &image::RgbaImage, x: f32, y: f32) -> image::Rgba<u8> {
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = image.get_pixel(x0, y0).0;
    let p10 = image.get_pixel(x1, y0).0;
    let p01 = image.get_pixel(x0, y1).0;
    let p11 = image.get_pixel(x1, y1).0;
    let mut out = [0u8; 4];
    for channel in 0..4 {
        let top = p00[channel] as f32 * (1.0 - tx) + p10[channel] as f32 * tx;
        let bottom = p01[channel] as f32 * (1.0 - tx) + p11[channel] as f32 * tx;
        out[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    image::Rgba(out)
}

fn shadow_from_alpha(image: &image::RgbaImage, radius: u32) -> image::RgbaImage {
    let mut shadow = image::RgbaImage::from_pixel(
        image.width() + radius * 2,
        image.height() + radius * 2,
        image::Rgba([0, 0, 0, 0]),
    );
    for y in 0..image.height() {
        for x in 0..image.width() {
            let alpha = image.get_pixel(x, y)[3];
            if alpha == 0 {
                continue;
            }
            shadow.put_pixel(
                x + radius,
                y + radius,
                image::Rgba([0, 0, 0, ((alpha as u16 * 128) / 255) as u8]),
            );
        }
    }
    shadow
}

fn blur_alpha_shadow(image: &mut image::RgbaImage, radius: u32) {
    let radius = radius.max(1) as i32;
    let original = image.clone();
    for y in 0..image.height() {
        for x in 0..image.width() {
            let mut total = 0u32;
            let mut count = 0u32;
            for yy in (y as i32 - radius)..=(y as i32 + radius) {
                for xx in (x as i32 - radius)..=(x as i32 + radius) {
                    if xx < 0
                        || yy < 0
                        || xx >= original.width() as i32
                        || yy >= original.height() as i32
                    {
                        continue;
                    }
                    total += original.get_pixel(xx as u32, yy as u32)[3] as u32;
                    count += 1;
                }
            }
            let alpha = total.checked_div(count).unwrap_or(0) as u8;
            image.put_pixel(x, y, image::Rgba([0, 0, 0, alpha]));
        }
    }
}

fn blend_image_over(
    target: &mut [u8],
    target_size: u32,
    image: &image::RgbaImage,
    dst_x: i32,
    dst_y: i32,
) {
    for y in 0..image.height() {
        let ty = dst_y + y as i32;
        if ty < 0 || ty >= target_size as i32 {
            continue;
        }
        for x in 0..image.width() {
            let tx = dst_x + x as i32;
            if tx < 0 || tx >= target_size as i32 {
                continue;
            }
            let src = image.get_pixel(x, y).0;
            if src[3] == 0 {
                continue;
            }
            let offset = ((ty as u32 * target_size + tx as u32) * 4) as usize;
            alpha_blend_pixel(&mut target[offset..offset + 4], src);
        }
    }
}

fn alpha_blend_pixel(dst: &mut [u8], src: [u8; 4]) {
    let src_a = src[3] as u32;
    let dst_a = dst[3] as u32;
    let out_a = src_a + dst_a * (255 - src_a) / 255;
    if out_a == 0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let src_term = src[channel] as u32 * src_a;
        let dst_term = dst[channel] as u32 * dst_a * (255 - src_a) / 255;
        dst[channel] = ((src_term + dst_term) / out_a).min(255) as u8;
    }
    dst[3] = out_a.min(255) as u8;
}

pub(crate) fn folder_preview_directory_seed(directory: &Path) -> u64 {
    hash_bytes_with_len(directory.to_string_lossy().as_bytes())
}

pub(crate) fn folder_preview_thumbnail_angle(seed: u64, index: usize) -> i32 {
    let mut value = seed ^ ((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value % 17) as i32 - 8
}
