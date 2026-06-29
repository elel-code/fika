use fika_core::{Entry, ViewRect};
use winit::dpi::PhysicalSize;

use crate::shell::render::quad::{QuadVertex, push_clipped_rect, push_clipped_rounded_rect};
use crate::shell::theme::{ShellScrollbarColors, ShellTheme, UiColor};

pub(crate) fn push_scrollbar(
    vertices: &mut Vec<QuadVertex>,
    track: ViewRect,
    thumb: ViewRect,
    clip: ViewRect,
    colors: ShellScrollbarColors,
    size: PhysicalSize<u32>,
) {
    let track_radius = track.width.min(track.height) / 2.0;
    let thumb_radius = thumb.width.min(thumb.height) / 2.0;
    push_clipped_rounded_rect(vertices, track, clip, track_radius, colors.track, size);
    push_clipped_rounded_rect(vertices, thumb, clip, thumb_radius, colors.thumb, size);
}

pub(crate) fn push_fallback_file_icon(
    vertices: &mut Vec<QuadVertex>,
    entry: &Entry,
    icon_rect: ViewRect,
    content_clip: ViewRect,
    theme: ShellTheme,
    size: PhysicalSize<u32>,
) {
    let palette = FallbackIconPalette::from_shell_theme(theme);
    if entry.is_dir {
        let tab = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.12,
            y: icon_rect.y + icon_rect.height * 0.16,
            width: icon_rect.width * 0.42,
            height: icon_rect.height * 0.18,
        };
        let body = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.08,
            y: icon_rect.y + icon_rect.height * 0.28,
            width: icon_rect.width * 0.84,
            height: icon_rect.height * 0.56,
        };
        push_clipped_rect(vertices, tab, content_clip, palette.folder_tab, size);
        push_clipped_rect(vertices, body, content_clip, palette.folder_body, size);
    } else {
        let body = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.18,
            y: icon_rect.y + icon_rect.height * 0.10,
            width: icon_rect.width * 0.64,
            height: icon_rect.height * 0.78,
        };
        let stripe = ViewRect {
            x: body.x,
            y: body.y,
            width: body.width,
            height: body.height * 0.22,
        };
        push_clipped_rect(
            vertices,
            body,
            content_clip,
            fallback_file_color(entry, palette),
            size,
        );
        push_clipped_rect(vertices, stripe, content_clip, palette.file_stripe, size);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FallbackIconPalette {
    folder_tab: UiColor,
    folder_body: UiColor,
    file_stripe: UiColor,
    image_file: UiColor,
    media_file: UiColor,
    text_file: UiColor,
    generic_file: UiColor,
}

impl FallbackIconPalette {
    pub(crate) fn from_shell_theme(theme: ShellTheme) -> Self {
        if theme.is_dark() {
            Self {
                folder_tab: [0.953, 0.612, 0.071, 1.0],
                folder_body: [0.749, 0.435, 0.047, 1.0],
                file_stripe: theme.field_separator(),
                image_file: [0.302, 0.741, 0.514, 1.0],
                media_file: [0.678, 0.560, 0.871, 1.0],
                text_file: [0.376, 0.647, 0.980, 1.0],
                generic_file: [0.420, 0.466, 0.545, 1.0],
            }
        } else {
            Self {
                folder_tab: [0.960, 0.700, 0.260, 1.0],
                folder_body: [0.900, 0.580, 0.180, 1.0],
                file_stripe: [0.760, 0.800, 0.860, 1.0],
                image_file: [0.500, 0.700, 0.560, 1.0],
                media_file: [0.690, 0.550, 0.820, 1.0],
                text_file: [0.380, 0.600, 0.840, 1.0],
                generic_file: [0.550, 0.600, 0.680, 1.0],
            }
        }
    }
}

pub(crate) fn fallback_file_color(entry: &Entry, palette: FallbackIconPalette) -> UiColor {
    let mime = entry.mime_type.as_deref().unwrap_or_default();
    fallback_file_color_for_mime(mime, palette)
}

fn fallback_file_color_for_mime(mime: &str, palette: FallbackIconPalette) -> UiColor {
    if mime.starts_with("image/") {
        palette.image_file
    } else if mime.starts_with("video/") || mime.starts_with("audio/") {
        palette.media_file
    } else if mime.contains("text") || mime.contains("json") || mime.contains("xml") {
        palette.text_file
    } else {
        palette.generic_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_icon_palette_follows_shell_theme() {
        let light = FallbackIconPalette::from_shell_theme(ShellTheme::for_dark_mode(false));
        let dark_theme = ShellTheme::for_dark_mode(true);
        let dark = FallbackIconPalette::from_shell_theme(dark_theme);

        assert_eq!(light.file_stripe, [0.760, 0.800, 0.860, 1.0]);
        assert_eq!(dark.file_stripe, dark_theme.field_separator());
        assert_ne!(light.generic_file, dark.generic_file);
        assert_eq!(
            fallback_file_color_for_mime("text/plain", dark),
            dark.text_file
        );
        assert_eq!(
            fallback_file_color_for_mime("video/mp4", dark),
            dark.media_file
        );
    }
}
