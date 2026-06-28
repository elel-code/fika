use std::borrow::Cow;

use fika_core::{CompactLayoutOptions, Entry};

use crate::shell::metrics::{
    DOLPHIN_ICONS_MAX_TEXT_LINES, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT, TEXT_PADDING,
    normalized_scale_factor,
};

pub(crate) fn estimated_label_raster_width(label: &str, font_size: f32) -> f32 {
    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    let width = label.chars().map(estimated_name_char_width).sum::<f32>() * scale
        + TEXT_PADDING as f32 * 2.0;
    width.ceil().max(1.0)
}

fn estimated_text_width_without_padding(label: &str, font_size: f32) -> f32 {
    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    label.chars().map(estimated_name_char_width).sum::<f32>() * scale
}

pub(crate) fn dolphin_elide_wrapped_filename_to_rect(
    label: &str,
    width: f32,
    height: f32,
    font_size: f32,
) -> Cow<'_, str> {
    let line_height = (font_size / TEXT_FONT_SIZE.max(1.0)) * TEXT_LINE_HEIGHT;
    let line_count = (height / line_height.max(1.0))
        .floor()
        .max(1.0)
        .min(DOLPHIN_ICONS_MAX_TEXT_LINES as f32) as usize;
    let available = dolphin_text_available_width(width);
    let Some(last_line_start) =
        dolphin_wrapped_filename_last_line_start(label, available, line_count, font_size)
    else {
        return Cow::Borrowed(label);
    };

    let mut display = String::from(&label[..last_line_start]);
    let last_line =
        dolphin_elide_filename_to_available_width(&label[last_line_start..], available, font_size);
    display.push_str(&last_line);
    Cow::Owned(display)
}

pub(crate) fn dolphin_elide_filename_to_width(
    label: &str,
    width: f32,
    font_size: f32,
) -> Cow<'_, str> {
    dolphin_elide_filename_to_available_width(label, dolphin_text_available_width(width), font_size)
}

pub(crate) fn dolphin_text_available_width(width: f32) -> f32 {
    (width - TEXT_PADDING as f32 * 2.0).max(1.0)
}

fn dolphin_elide_filename_to_available_width(
    label: &str,
    available: f32,
    font_size: f32,
) -> Cow<'_, str> {
    if estimated_text_width_without_padding(label, font_size) <= available {
        return Cow::Borrowed(label);
    }

    const ELLIPSIS: &str = "...";
    let ellipsis_width = estimated_text_width_without_padding(ELLIPSIS, font_size);
    let Some((base, extension)) = filename_base_and_extension(label) else {
        return Cow::Owned(elide_text_right(label, available, font_size));
    };

    let extension_width = estimated_text_width_without_padding(extension, font_size);
    if extension_width + ellipsis_width < available {
        let base_budget = (available - extension_width - ellipsis_width).max(1.0);
        let mut result = take_prefix_to_width(base, base_budget, font_size);
        result.push_str(ELLIPSIS);
        result.push_str(extension);
        return Cow::Owned(result);
    }

    Cow::Owned(elide_text_right(label, available, font_size))
}

fn dolphin_wrapped_filename_last_line_start(
    label: &str,
    available_width: f32,
    max_lines: usize,
    font_size: f32,
) -> Option<usize> {
    let max_lines = max_lines.max(1);
    let mut line_start = 0;
    for line_index in 0..max_lines {
        let line_end =
            dolphin_wrapped_filename_line_end(label, line_start, available_width, font_size);
        if line_end >= label.len() {
            return None;
        }
        if line_index + 1 == max_lines {
            return Some(line_start);
        }
        line_start = line_end;
    }
    Some(line_start)
}

pub(crate) fn dolphin_wrapped_filename_line_count(
    label: &str,
    available_width: f32,
    max_lines: usize,
    font_size: f32,
) -> usize {
    let max_lines = max_lines.max(1);
    let mut line_start = 0;
    let mut lines = 0;
    while line_start < label.len() && lines < max_lines {
        lines += 1;
        let line_end =
            dolphin_wrapped_filename_line_end(label, line_start, available_width, font_size);
        if line_end >= label.len() {
            break;
        }
        line_start = line_end;
    }
    lines.max(1)
}

fn dolphin_wrapped_filename_line_end(
    label: &str,
    start: usize,
    available_width: f32,
    font_size: f32,
) -> usize {
    if start >= label.len() {
        return label.len();
    }
    let available_width = available_width.max(1.0);
    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    let mut width = 0.0;
    for (offset, ch) in label[start..].char_indices() {
        let byte = start + offset;
        let next = byte + ch.len_utf8();
        let char_width = estimated_name_char_width(ch) * scale;
        if width + char_width > available_width {
            return if byte == start { next } else { byte };
        }
        width += char_width;
    }
    label.len()
}

fn filename_base_and_extension(label: &str) -> Option<(&str, &str)> {
    let dot = label.rfind('.')?;
    if dot == 0 || dot + 1 >= label.len() {
        return None;
    }
    let (base, extension) = label.split_at(dot);
    (!base.is_empty() && extension.len() > 1).then_some((base, extension))
}

fn elide_text_right(label: &str, available: f32, font_size: f32) -> String {
    const ELLIPSIS: &str = "...";
    let ellipsis_width = estimated_text_width_without_padding(ELLIPSIS, font_size);
    if ellipsis_width >= available {
        return ELLIPSIS.to_string();
    }
    let mut result = take_prefix_to_width(label, available - ellipsis_width, font_size);
    result.push_str(ELLIPSIS);
    result
}

fn take_prefix_to_width(label: &str, max_width: f32, font_size: f32) -> String {
    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    let mut width = 0.0;
    let mut result = String::new();
    for ch in label.chars() {
        let char_width = estimated_name_char_width(ch) * scale;
        if width + char_width > max_width && !result.is_empty() {
            break;
        }
        if width + char_width > max_width {
            break;
        }
        width += char_width;
        result.push(ch);
    }
    result
}

pub(crate) fn required_compact_item_width(options: CompactLayoutOptions, text_width: f32) -> f32 {
    (options.padding * 2.0 + options.icon_size + options.text_gap + text_width).round()
}

pub(crate) fn compact_entry_text_width(entry: &Entry, scale_factor: f32) -> f32 {
    estimated_text_width_without_padding(
        entry.name.as_ref(),
        TEXT_FONT_SIZE * normalized_scale_factor(scale_factor),
    )
}

pub(crate) fn icons_entry_text_line_count(
    entry: &Entry,
    scale_factor: f32,
    available_width: f32,
) -> usize {
    dolphin_wrapped_filename_line_count(
        entry.name.as_ref(),
        available_width,
        DOLPHIN_ICONS_MAX_TEXT_LINES,
        TEXT_FONT_SIZE * normalized_scale_factor(scale_factor),
    )
}

fn estimated_name_char_width(ch: char) -> f32 {
    match ch {
        '\u{200B}' => 0.0,
        '\u{2026}' => 8.0,
        'i' | 'l' | 'I' | '!' | '.' | ',' | ':' | ';' | '\'' | '`' | '|' => 4.0,
        ' ' | '-' | '_' => 5.0,
        'm' | 'w' | 'M' | 'W' | '@' | '%' | '#' => 11.0,
        'A'..='Z' => 9.0,
        '0'..='9' => 8.0,
        ch if ch.is_ascii() => 7.5,
        _ => 14.0,
    }
}
