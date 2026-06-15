use crate::ui::rename::RenameDraft;

use fika_core::{CompactColumnMetrics, CompactLayout, CompactLayoutOptions, IconsLayoutOptions};

const AVERAGE_COMPACT_CHAR_WIDTH: f32 = 8.5;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompactColumnWidthCacheKey {
    generation: u64,
    source_revision: u64,
    item_count: usize,
    rows_per_column: usize,
    min_item_width: f32,
    icon_size: f32,
    padding: f32,
    gap: f32,
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
            gap: options.gap,
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
                options.padding,
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
    options.padding * 4.0 + options.icon_size + text_width
}

pub(crate) fn compact_text_width(name_width_units: u16) -> f32 {
    f32::from(name_width_units) * AVERAGE_COMPACT_CHAR_WIDTH
}

pub(crate) fn compact_text_width_for_name(name: &str) -> f32 {
    name.chars().map(estimated_name_char_width).sum()
}

fn estimated_name_char_width(ch: char) -> f32 {
    match ch {
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
    item_name_text_height_for_width(compact_text_width_for_name(name), available_text_width)
}

pub(crate) fn item_name_text_height_for_width(text_width: f32, available_text_width: f32) -> f32 {
    let available_text_width = available_text_width.max(1.0);
    let line_count = (text_width.max(1.0) / available_text_width).ceil().max(1.0);
    line_count * super::ITEM_NAME_LINE_HEIGHT
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
        .map(|draft| base_width.max(compact_text_width_for_name(&draft.draft_name)))
        .unwrap_or(base_width)
}

pub(crate) fn rename_text_override_for_model(
    model: &fika_core::DirectoryModel,
    draft: Option<&RenameDraft>,
) -> Option<CompactTextWidthOverride> {
    let draft = draft?;
    let model_index = model.index_of_path(&draft.original_path)?;
    Some(CompactTextWidthOverride {
        model_index,
        text_width: compact_text_width_for_name(&draft.draft_name),
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
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    item_count: usize,
    view: &fika_core::ViewState,
    rename_draft: Option<&RenameDraft>,
    reserved_bottom: f32,
) -> IconsLayoutOptions {
    let text_override = rename_text_override_for_model(model, rename_draft);
    let mut options = super::icons_layout_options(view, reserved_bottom);
    let available_text_width = icon_name_available_width(options);
    let name_height = max_item_name_text_height_for_model(
        model,
        filtered,
        item_count,
        text_override,
        available_text_width,
    );
    options.text_height = options.text_height.max(name_height);
    options.item_height =
        options.padding * 3.0 + options.icon_size + options.gap + options.text_height;
    options
}

fn compact_layout_options_for_model_view(
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    item_count: usize,
    view: &fika_core::ViewState,
    text_override: Option<CompactTextWidthOverride>,
) -> CompactLayoutOptions {
    let mut options = super::compact_layout_options(view, 0.0);
    let available_text_width = compact_base_name_available_width(options);
    let name_height = max_item_name_text_height_for_model(
        model,
        filtered,
        item_count,
        text_override,
        available_text_width,
    );
    options.text_height = options
        .text_height
        .max(name_height + super::ITEM_HELPER_LABEL_HEIGHT);
    options.item_height =
        (options.icon_size + 32.0).max(options.text_height + options.padding * 2.0);
    options
}

fn max_item_name_text_height_for_model(
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    item_count: usize,
    text_override: Option<CompactTextWidthOverride>,
    available_text_width: f32,
) -> f32 {
    max_required_text_width_for_model(model, filtered, item_count, text_override)
        .map(|width| item_name_text_height_for_width(width, available_text_width))
        .unwrap_or(super::ITEM_NAME_LINE_HEIGHT)
}

fn max_required_text_width_for_model(
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    item_count: usize,
    text_override: Option<CompactTextWidthOverride>,
) -> Option<f32> {
    (0..item_count)
        .filter_map(|layout_index| {
            let model_index = model_index_for_layout_index(filtered, layout_index)?;
            let entry = model.get(model_index)?;
            let override_text_width = text_override
                .filter(|override_| override_.model_index == model_index)
                .map(|override_| override_.text_width);
            Some(entry_name_text_width(entry).max(override_text_width.unwrap_or_default()))
        })
        .reduce(f32::max)
}

fn icon_name_available_width(options: IconsLayoutOptions) -> f32 {
    (options.item_width - options.padding * 2.0).max(1.0)
}

fn compact_base_name_available_width(options: CompactLayoutOptions) -> f32 {
    (options.item_width - options.padding * 2.0 - options.icon_size - options.gap).max(1.0)
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
    fn icons_layout_options_expand_text_height_for_wrapped_long_names() {
        let long_name = "Very Long Desktop Launcher Name.desktop";
        let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
        model.replace_listing(PathBuf::from("/tmp"), Arc::new(vec![test_entry(long_name)]));
        let view = ViewState {
            viewport_width: 160.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let base_options = crate::ui::file_grid::icons_layout_options(&view, 0.0);

        let options = icons_layout_options_for_model(&model, None, model.len(), &view, None, 0.0);

        assert!(options.text_height > base_options.text_height);
        assert!(options.item_height > base_options.item_height);
    }

    #[test]
    fn compact_layout_expands_text_height_for_wrapped_long_names() {
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

        assert!(item.text_rect.height > base_options.text_height);
        assert!(item.item_rect.height > base_options.item_height);
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
