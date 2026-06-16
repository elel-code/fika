use crate::ui::rename::{RENAME_TEXT_INSET_X, RenameDraft};

use fika_core::{
    CompactColumnMetrics, CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions,
};
use std::ops::Range;

const AVERAGE_COMPACT_CHAR_WIDTH: f32 = 8.5;
const DOLPHIN_WRAP_OPPORTUNITY: char = '\u{200B}';
const DOLPHIN_ELISION_MARKER: &str = "\u{2026}";

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompactColumnWidthCacheKey {
    generation: u64,
    source_revision: u64,
    item_count: usize,
    rows_per_column: usize,
    min_item_width: f32,
    icon_size: f32,
    padding: f32,
    side_padding: f32,
    gap: f32,
    text_gap: f32,
    text_override_model_index: Option<usize>,
    text_override_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CompactTextWidthOverride {
    pub(crate) model_index: usize,
    pub(crate) text_width: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CompactColumnWidthCache {
    pub(crate) cached: Vec<CompactColumnWidthCacheEntry>,
}

#[derive(Clone, Debug)]
pub(crate) struct CompactColumnWidthCacheEntry {
    key: CompactColumnWidthCacheKey,
    metrics: CompactColumnMetrics,
}

impl CompactColumnWidthCache {
    const MAX_CACHED_LAYOUTS: usize = 4;

    pub(crate) fn metrics_for_model_with_text_override(
        &mut self,
        model: &fika_core::DirectoryModel,
        rows_per_column: usize,
        options: CompactLayoutOptions,
        text_override: Option<CompactTextWidthOverride>,
    ) -> CompactColumnMetrics {
        self.metrics_for_model_view(model, None, 0, rows_per_column, options, text_override)
    }

    pub(crate) fn metrics_for_filtered_model(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: &fika_core::FilteredModel,
        source_revision: u64,
        rows_per_column: usize,
        options: CompactLayoutOptions,
        text_override: Option<CompactTextWidthOverride>,
    ) -> CompactColumnMetrics {
        self.metrics_for_model_view(
            model,
            Some(filtered),
            source_revision,
            rows_per_column,
            options,
            text_override,
        )
    }

    fn metrics_for_model_view(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: Option<&fika_core::FilteredModel>,
        source_revision: u64,
        rows_per_column: usize,
        options: CompactLayoutOptions,
        text_override: Option<CompactTextWidthOverride>,
    ) -> CompactColumnMetrics {
        let item_count = filtered.map_or_else(|| model.len(), fika_core::FilteredModel::len);
        let key = CompactColumnWidthCacheKey {
            generation: model.structure_generation(),
            source_revision,
            item_count,
            rows_per_column,
            min_item_width: options.item_width,
            icon_size: options.icon_size,
            padding: options.padding,
            side_padding: options.side_padding,
            gap: options.gap,
            text_gap: options.text_gap,
            text_override_model_index: text_override.map(|override_| override_.model_index),
            text_override_width: text_override
                .map(|override_| override_.text_width.max(0.0))
                .unwrap_or_default(),
        };
        let column_count = item_count.div_ceil(rows_per_column);
        let position = self.cached.iter().position(|entry| entry.key == key);
        let position = match position {
            Some(position) => position,
            None => {
                if self.cached.len() >= Self::MAX_CACHED_LAYOUTS {
                    self.cached.remove(0);
                }
                let widths = resolve_all_column_widths(
                    model,
                    filtered,
                    item_count,
                    rows_per_column,
                    options,
                    text_override,
                );
                self.cached.push(CompactColumnWidthCacheEntry::new(
                    key,
                    column_count,
                    options,
                    widths,
                ));
                self.cached.len() - 1
            }
        };

        self.cached[position].metrics.clone()
    }
}

impl CompactColumnWidthCacheEntry {
    fn new(
        key: CompactColumnWidthCacheKey,
        column_count: usize,
        options: CompactLayoutOptions,
        widths: Vec<f32>,
    ) -> Self {
        Self {
            key,
            metrics: CompactColumnMetrics::new(
                column_count,
                options.item_width,
                options.side_padding,
                options.gap,
                widths,
            ),
        }
    }
}

fn resolve_all_column_widths(
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    item_count: usize,
    rows_per_column: usize,
    options: CompactLayoutOptions,
    text_override: Option<CompactTextWidthOverride>,
) -> Vec<f32> {
    let column_count = item_count.div_ceil(rows_per_column);
    let mut widths = vec![options.item_width; column_count];
    for layout_index in 0..item_count {
        let column = layout_index / rows_per_column;
        let Some(model_index) = model_index_for_layout_index(filtered, layout_index) else {
            continue;
        };
        let Some(entry) = model.get(model_index) else {
            continue;
        };
        let override_text_width = text_override
            .filter(|override_| override_.model_index == model_index)
            .map(|override_| override_.text_width);
        let width = required_compact_item_width(entry, options, override_text_width);
        if let Some(cached_width) = widths.get_mut(column) {
            *cached_width = cached_width.max(width);
        }
    }
    widths
}

fn required_compact_item_width(
    entry: &fika_core::EntryData,
    options: CompactLayoutOptions,
    text_override_width: Option<f32>,
) -> f32 {
    let text_width =
        entry_name_text_width(entry).max(text_override_width.unwrap_or_default().max(0.0));
    options.padding * 2.0 + options.text_gap + options.icon_size + text_width
}

pub(crate) fn compact_text_width(name_width_units: u16) -> f32 {
    f32::from(name_width_units) * AVERAGE_COMPACT_CHAR_WIDTH
}

pub(crate) fn compact_text_width_for_name(name: &str) -> f32 {
    name.chars().map(estimated_name_char_width).sum()
}

fn estimated_name_char_width(ch: char) -> f32 {
    match ch {
        DOLPHIN_WRAP_OPPORTUNITY => 0.0,
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

pub(crate) fn item_name_text_height_for_name(name: &str, available_text_width: f32) -> f32 {
    wrapped_item_name_line_count(name, available_text_width) as f32 * super::ITEM_NAME_LINE_HEIGHT
}

pub(crate) fn icon_name_display_lines(
    name: &str,
    available_text_width: f32,
    max_lines: usize,
) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }

    let name = dolphin_preprocess_wrap(name);
    let ranges = wrapped_item_name_line_ranges(&name, available_text_width);
    if ranges.len() <= max_lines {
        return ranges
            .into_iter()
            .map(|range| visible_item_name_text(&name[range]))
            .collect();
    }

    let mut lines = ranges
        .iter()
        .take(max_lines.saturating_sub(1))
        .map(|range| visible_item_name_text(&name[range.clone()]))
        .collect::<Vec<_>>();
    let last_start = ranges[max_lines - 1].start;
    let last_start = skip_leading_whitespace(&name, last_start);
    lines.push(elide_middle_text_for_width(
        &name[last_start..],
        available_text_width,
    ));
    lines
}

fn visible_item_name_text(text: &str) -> String {
    text.chars()
        .filter(|ch| *ch != DOLPHIN_WRAP_OPPORTUNITY)
        .collect()
}

fn elide_middle_text_for_width(text: &str, available_text_width: f32) -> String {
    let available_text_width = available_text_width.max(1.0);
    let visible = visible_item_name_text(text);
    if estimated_text_width(&visible) <= available_text_width {
        return visible;
    }

    if estimated_text_width(DOLPHIN_ELISION_MARKER) >= available_text_width {
        return DOLPHIN_ELISION_MARKER.to_string();
    }

    let chars = visible.chars().collect::<Vec<_>>();
    let mut omitted_start = chars.len() / 2;
    let mut omitted_end = omitted_start;
    loop {
        let candidate = middle_elided_candidate(&chars, omitted_start, omitted_end);
        if estimated_text_width(&candidate) <= available_text_width {
            return candidate;
        }
        if omitted_start == 0 && omitted_end == chars.len() {
            return DOLPHIN_ELISION_MARKER.to_string();
        }
        let prefix_width = estimated_chars_width(&chars[..omitted_start]);
        let suffix_width = estimated_chars_width(&chars[omitted_end..]);
        if prefix_width >= suffix_width && omitted_start > 0 {
            omitted_start -= 1;
        } else if omitted_end < chars.len() {
            omitted_end += 1;
        } else if omitted_start > 0 {
            omitted_start -= 1;
        }
    }
}

fn estimated_chars_width(chars: &[char]) -> f32 {
    chars.iter().copied().map(estimated_name_char_width).sum()
}

fn middle_elided_candidate(chars: &[char], omitted_start: usize, omitted_end: usize) -> String {
    let mut candidate = String::new();
    candidate.extend(chars[..omitted_start].iter().copied());
    candidate.push_str(DOLPHIN_ELISION_MARKER);
    candidate.extend(chars[omitted_end..].iter().copied());
    candidate
}

pub(crate) fn dolphin_preprocess_wrap(text: &str) -> String {
    let mut processed = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        processed.push(ch);
        let Some(next) = chars.peek().copied() else {
            continue;
        };
        if dolphin_wrap_opportunity_after(ch)
            && !next.is_whitespace()
            && next != DOLPHIN_WRAP_OPPORTUNITY
        {
            processed.push(DOLPHIN_WRAP_OPPORTUNITY);
        }
    }
    processed
}

fn dolphin_wrap_opportunity_after(ch: char) -> bool {
    ch.is_ascii_punctuation()
}

fn wrapped_item_name_line_count(name: &str, available_text_width: f32) -> usize {
    wrapped_item_name_line_ranges(name, available_text_width).len()
}

fn wrapped_item_name_line_ranges(name: &str, available_text_width: f32) -> Vec<Range<usize>> {
    let available_text_width = available_text_width.max(1.0);
    if name.is_empty() {
        return vec![0..0];
    }

    let mut lines = Vec::new();
    let mut line_start = 0usize;
    let mut line_width = 0.0f32;
    let mut last_wrap_candidate = None;
    let mut chars = name.char_indices().peekable();

    while let Some((byte_index, ch)) = chars.next() {
        let next_byte = chars.peek().map(|(index, _)| *index).unwrap_or(name.len());
        if ch == '\n' {
            push_wrapped_line(&mut lines, name, line_start, byte_index);
            line_start = next_byte;
            line_width = 0.0;
            last_wrap_candidate = None;
            continue;
        }

        let char_width = estimated_name_char_width(ch);
        if line_width + char_width > available_text_width && line_width > 0.0 {
            let break_byte = last_wrap_candidate
                .filter(|candidate| *candidate > line_start && *candidate <= byte_index)
                .unwrap_or(byte_index);
            push_wrapped_line(&mut lines, name, line_start, break_byte);
            line_start = skip_leading_whitespace(name, break_byte);
            line_width = estimated_text_width(&name[line_start..byte_index]);
            last_wrap_candidate = last_wrap_candidate.filter(|candidate| *candidate > line_start);
        }

        line_width += char_width;
        if is_estimated_wrap_candidate(ch) {
            last_wrap_candidate = Some(next_byte);
        }
    }

    if line_start <= name.len() {
        push_wrapped_line(&mut lines, name, line_start, name.len());
    }

    if lines.is_empty() {
        lines.push(0..0);
    }
    lines
}

fn push_wrapped_line(
    lines: &mut Vec<Range<usize>>,
    name: &str,
    line_start: usize,
    line_end: usize,
) {
    lines.push(line_start..trim_trailing_whitespace(name, line_start, line_end));
}

fn trim_trailing_whitespace(name: &str, line_start: usize, mut line_end: usize) -> usize {
    while line_end > line_start {
        let Some((previous_index, previous)) = name[..line_end].char_indices().next_back() else {
            break;
        };
        if !previous.is_whitespace() {
            break;
        }
        line_end = previous_index;
    }
    line_end
}

fn skip_leading_whitespace(name: &str, mut index: usize) -> usize {
    while index < name.len() {
        let Some(ch) = name[index..].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn estimated_text_width(text: &str) -> f32 {
    text.chars().map(estimated_name_char_width).sum()
}

fn is_estimated_wrap_candidate(ch: char) -> bool {
    if ch == DOLPHIN_WRAP_OPPORTUNITY {
        return true;
    }
    ch.is_whitespace() || !(ch.is_ascii_alphanumeric() || matches!(ch, '\u{00C0}'..='\u{024F}'))
}

pub(crate) fn entry_name_text_width(entry: &fika_core::EntryData) -> f32 {
    compact_text_width_for_name(&entry.name).max(compact_text_width(entry.name_width_units))
}

pub(crate) fn required_text_width_for_entry(
    entry: &fika_core::EntryData,
    draft: Option<&RenameDraft>,
) -> f32 {
    let base_width = entry_name_text_width(entry);
    draft
        .map(|draft| {
            base_width.max(rename_editor_required_text_width(
                compact_text_width_for_name(&draft.draft_name),
            ))
        })
        .unwrap_or(base_width)
}

pub(crate) fn rename_editor_required_text_width(text_width: f32) -> f32 {
    text_width + RENAME_TEXT_INSET_X * 2.0
}

pub(crate) fn rename_text_override_for_model(
    model: &fika_core::DirectoryModel,
    draft: Option<&RenameDraft>,
) -> Option<CompactTextWidthOverride> {
    let draft = draft?;
    let model_index = model.index_of_path(&draft.original_path)?;
    Some(CompactTextWidthOverride {
        model_index,
        text_width: rename_editor_required_text_width(compact_text_width_for_name(
            &draft.draft_name,
        )),
    })
}

#[cfg(test)]
fn compact_name_width_units(name: &str) -> u16 {
    name.chars()
        .map(|ch| if ch.is_ascii() { 1u32 } else { 2u32 })
        .sum::<u32>()
        .min(u16::MAX as u32) as u16
}

pub(crate) fn model_index_for_layout_index(
    filtered: Option<&fika_core::FilteredModel>,
    layout_index: usize,
) -> Option<usize> {
    filtered.map_or(Some(layout_index), |filtered| {
        filtered.model_index(layout_index)
    })
}

pub(crate) fn compact_layout_for_model_with_text_override(
    cache: &mut CompactColumnWidthCache,
    model: &fika_core::DirectoryModel,
    view: &fika_core::ViewState,
    text_override: Option<CompactTextWidthOverride>,
) -> CompactLayout {
    compact_layout_for_model_view(cache, model, None, 0, view, text_override)
}

pub(crate) fn compact_layout_for_filtered_model_with_text_override(
    cache: &mut CompactColumnWidthCache,
    model: &fika_core::DirectoryModel,
    filtered: &fika_core::FilteredModel,
    source_revision: u64,
    view: &fika_core::ViewState,
    text_override: Option<CompactTextWidthOverride>,
) -> CompactLayout {
    compact_layout_for_model_view(
        cache,
        model,
        Some(filtered),
        source_revision,
        view,
        text_override,
    )
}

fn compact_layout_for_model_view(
    cache: &mut CompactColumnWidthCache,
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    source_revision: u64,
    view: &fika_core::ViewState,
    text_override: Option<CompactTextWidthOverride>,
) -> CompactLayout {
    let item_count = filtered.map_or_else(|| model.len(), fika_core::FilteredModel::len);
    let options =
        compact_layout_options_for_model_view(model, filtered, item_count, view, text_override);
    let rows_per_column = CompactLayout::rows_per_column_for_options(options);
    let metrics = match filtered {
        Some(filtered) => cache.metrics_for_filtered_model(
            model,
            filtered,
            source_revision,
            rows_per_column,
            options,
            text_override,
        ),
        None => cache.metrics_for_model_with_text_override(
            model,
            rows_per_column,
            options,
            text_override,
        ),
    };
    CompactLayout::new_with_column_metrics(item_count, options, metrics)
}

pub(crate) fn icons_layout_options_for_model(
    _model: &fika_core::DirectoryModel,
    _filtered: Option<&fika_core::FilteredModel>,
    _item_count: usize,
    view: &fika_core::ViewState,
    _rename_draft: Option<&RenameDraft>,
    reserved_bottom: f32,
) -> IconsLayoutOptions {
    super::icons_layout_options(view, reserved_bottom)
}

pub(crate) fn icons_layout_for_model(
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    item_count: usize,
    view: &fika_core::ViewState,
    rename_draft: Option<&RenameDraft>,
    reserved_bottom: f32,
) -> IconsLayout {
    let options = icons_layout_options_for_model(
        model,
        filtered,
        item_count,
        view,
        rename_draft,
        reserved_bottom,
    );
    IconsLayout::new(item_count, options)
}

fn compact_layout_options_for_model_view(
    _model: &fika_core::DirectoryModel,
    _filtered: Option<&fika_core::FilteredModel>,
    _item_count: usize,
    view: &fika_core::ViewState,
    _text_override: Option<CompactTextWidthOverride>,
) -> CompactLayoutOptions {
    super::compact_layout_options(view, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::{DirectoryModel, Entry, EntryData, EntryMetadataRole, ViewState};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn test_entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: compact_name_width_units(name),
            target_path: None,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    #[test]
    fn compact_text_width_for_name_uses_conservative_character_estimates() {
        assert!(compact_text_width_for_name("目") > compact_text_width_for_name("a"));
        assert!(compact_text_width_for_name("WWW") > compact_text_width(3));
        assert!(compact_text_width_for_name("iii") < compact_text_width(3));
    }

    #[test]
    fn item_name_text_height_accounts_for_word_boundary_wrapping() {
        let word = "wwwww";
        let name = "wwwww wwwww wwwww";
        let available_width = compact_text_width_for_name(word) * 2.0 - 1.0;

        assert_eq!(
            item_name_text_height_for_name(name, available_width),
            super::super::ITEM_NAME_LINE_HEIGHT * 3.0
        );
    }

    #[test]
    fn icon_name_display_lines_elides_remaining_text_on_last_line() {
        let name = "elzykosuda227446+breuyev@hotmail.cpa.2026-06-22.json";
        let available_width = compact_text_width_for_name("elzykosuda227");

        let lines = icon_name_display_lines(name, available_width, 3);

        assert_eq!(lines.len(), 3);
        assert!(!lines[0].contains(DOLPHIN_ELISION_MARKER));
        assert!(!lines[1].contains(DOLPHIN_ELISION_MARKER));
        assert!(lines[2].contains(DOLPHIN_ELISION_MARKER));
        assert!(
            lines
                .iter()
                .all(|line| estimated_text_width(line) <= available_width)
        );
    }

    #[test]
    fn icon_name_display_lines_hide_wrap_opportunities() {
        let name = "alpha-beta.gamma";
        let lines = icon_name_display_lines(name, 240.0, 3);

        assert_eq!(lines, vec![name.to_string()]);
    }

    #[test]
    fn middle_elision_preserves_both_ends() {
        let name = "very-long-filename-with-extension.txt";
        let available_width =
            compact_text_width_for_name("very-l") + compact_text_width_for_name(".txt");

        let elided = elide_middle_text_for_width(name, available_width);

        assert!(elided.starts_with("very"));
        assert!(elided.ends_with(".txt"));
        assert!(elided.contains(DOLPHIN_ELISION_MARKER));
        assert!(estimated_text_width(&elided) <= available_width);
    }

    #[test]
    fn icons_layout_reserves_three_name_lines_for_all_items() {
        let long_name = "elzykosuda227446+breuyev@hotmail.cpa.2026-06-22.json";
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            Arc::new(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry(long_name),
            ]),
        );
        let view = ViewState {
            viewport_width: 260.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let base_options = crate::ui::file_grid::icons_layout_options(&view, 0.0);

        let layout = icons_layout_for_model(&model, None, model.len(), &view, None, 0.0);
        let expected_text_height =
            super::super::ITEM_NAME_LINE_HEIGHT * super::super::DOLPHIN_ICON_MAX_TEXT_LINES as f32;

        assert_eq!(base_options.text_height, expected_text_height);
        assert_eq!(
            layout.item(0).unwrap().item_rect.height,
            base_options.item_height
        );
        assert_eq!(
            layout.item(0).unwrap().text_rect.height,
            expected_text_height
        );
        assert_eq!(
            layout.item(2).unwrap().item_rect.height,
            base_options.item_height
        );
        assert_eq!(
            layout.item(2).unwrap().text_rect.height,
            expected_text_height
        );
        assert_eq!(
            layout.item(2).unwrap().item_rect.y,
            base_options.gap + base_options.item_height + base_options.gap
        );
    }

    #[test]
    fn dolphin_preprocess_wrap_adds_invisible_breaks_without_ellipsis() {
        let name = "alpha-beta.gamma";
        let display_name = dolphin_preprocess_wrap(name);

        assert!(display_name.contains(DOLPHIN_WRAP_OPPORTUNITY));
        assert!(!display_name.contains("..."));
        assert!(!display_name.contains('\u{2026}'));
        assert_eq!(display_name.replace(DOLPHIN_WRAP_OPPORTUNITY, ""), name);
        assert_eq!(
            compact_text_width_for_name(&display_name),
            compact_text_width_for_name(name)
        );
    }

    #[test]
    fn compact_layout_keeps_row_height_for_long_names() {
        let long_name = "Very Long Desktop Launcher Name.desktop";
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(PathBuf::from("/tmp"), Arc::new(vec![test_entry(long_name)]));
        let view = ViewState {
            viewport_width: 220.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let base_options = crate::ui::file_grid::compact_layout_options(&view, 0.0);
        let mut cache = CompactColumnWidthCache::default();

        let layout = compact_layout_for_model_with_text_override(&mut cache, &model, &view, None);
        let item = layout.item(0).unwrap();

        assert_eq!(item.text_rect.height, base_options.text_height);
        assert_eq!(item.item_rect.height, base_options.item_height);
        assert!(item.item_rect.width > base_options.item_width);
    }

    #[test]
    fn rename_text_override_expands_column_width() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(PathBuf::from("/tmp"), Arc::new(vec![test_entry("a.txt")]));
        let view = ViewState {
            viewport_width: 300.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let mut cache = CompactColumnWidthCache::default();

        let base = compact_layout_for_model_with_text_override(&mut cache, &model, &view, None);
        let expanded = compact_layout_for_model_with_text_override(
            &mut cache,
            &model,
            &view,
            Some(CompactTextWidthOverride {
                model_index: 0,
                text_width: compact_text_width_for_name("much-longer-name.txt"),
            }),
        );

        assert!(expanded.item(0).unwrap().item_rect.width > base.item(0).unwrap().item_rect.width);
    }

    #[test]
    fn compact_column_width_cache_resolves_all_columns_before_first_layout() {
        let mut entries = (0..240)
            .map(|index| test_entry(&format!("file-{index}.txt")))
            .collect::<Vec<_>>();
        entries[239] = test_entry("this-name-is-intentionally-far-outside-the-viewport.txt");
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(PathBuf::from("/tmp"), Arc::new(entries));
        let view = ViewState {
            viewport_width: 220.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let mut cache = CompactColumnWidthCache::default();

        let layout = compact_layout_for_model_with_text_override(&mut cache, &model, &view, None);

        assert!(layout.item(239).unwrap().item_rect.width > 168.0);
    }

    #[test]
    fn compact_column_width_cache_survives_metadata_role_updates() {
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(
            PathBuf::from("/tmp"),
            Arc::new(vec![test_entry("a.txt"), test_entry("long-name.txt")]),
        );
        let item_id = model.entries()[0].id;
        let view = ViewState {
            viewport_width: 220.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let mut cache = CompactColumnWidthCache::default();

        let _ = compact_layout_for_model_with_text_override(&mut cache, &model, &view, None);
        let key = cache.cached[0].key;
        model.set_metadata_role(
            item_id,
            Path::new("/tmp/a.txt"),
            EntryMetadataRole {
                size_bytes: 1024,
                modified_secs: Some(42),
                mime_type: Some(Arc::from("text/plain")),
                mime_magic_checked: true,
            },
        );
        let _ = compact_layout_for_model_with_text_override(&mut cache, &model, &view, None);

        assert_eq!(cache.cached.len(), 1);
        assert_eq!(cache.cached[0].key, key);
    }
}
