use fika_core::{CompactColumnMetrics, CompactLayout, CompactLayoutOptions};

const AVERAGE_COMPACT_CHAR_WIDTH: f32 = 7.0;

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
    widths: Vec<f32>,
    pub(crate) resolved_columns: Vec<bool>,
    metrics: Option<CompactColumnMetrics>,
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
            generation: model.data_generation(),
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
                self.cached.push(CompactColumnWidthCacheEntry::new(
                    key,
                    column_count,
                    options,
                ));
                self.cached.len() - 1
            }
        };

        let entry = &mut self.cached[position];
        entry.resolve_all_columns(
            model,
            filtered,
            item_count,
            rows_per_column,
            options,
            text_override,
        );
        entry.metrics(options)
    }
}

impl CompactColumnWidthCacheEntry {
    fn new(
        key: CompactColumnWidthCacheKey,
        column_count: usize,
        options: CompactLayoutOptions,
    ) -> Self {
        Self {
            key,
            widths: vec![options.item_width; column_count],
            resolved_columns: vec![false; column_count],
            metrics: None,
        }
    }

    fn metrics(&mut self, options: CompactLayoutOptions) -> CompactColumnMetrics {
        if let Some(metrics) = &self.metrics {
            return metrics.clone();
        }
        let metrics = CompactColumnMetrics::new(
            self.widths.len(),
            options.item_width,
            options.padding,
            options.gap,
            self.widths.clone(),
        );
        self.metrics = Some(metrics.clone());
        metrics
    }

    fn resolve_all_columns(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: Option<&fika_core::FilteredModel>,
        item_count: usize,
        rows_per_column: usize,
        options: CompactLayoutOptions,
        text_override: Option<CompactTextWidthOverride>,
    ) {
        if self.widths.is_empty() {
            return;
        }

        self.resolve_columns(
            model,
            filtered,
            item_count,
            rows_per_column,
            options,
            text_override,
            0..self.widths.len(),
        );
    }

    fn resolve_columns(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: Option<&fika_core::FilteredModel>,
        item_count: usize,
        rows_per_column: usize,
        options: CompactLayoutOptions,
        text_override: Option<CompactTextWidthOverride>,
        columns: std::ops::Range<usize>,
    ) -> bool {
        let mut width_changed = false;
        for column in columns {
            if self
                .resolved_columns
                .get(column)
                .copied()
                .unwrap_or_default()
            {
                continue;
            }
            let start = column * rows_per_column;
            let end = (start + rows_per_column).min(item_count);
            let mut width = options.item_width;
            for layout_index in start..end {
                let Some(model_index) = model_index_for_layout_index(filtered, layout_index) else {
                    continue;
                };
                if let Some(entry) = model.get(model_index) {
                    let override_text_width = text_override
                        .filter(|override_| override_.model_index == model_index)
                        .map(|override_| override_.text_width);
                    width = width.max(required_compact_item_width(
                        entry,
                        options,
                        override_text_width,
                    ));
                }
            }
            if let Some(resolved) = self.resolved_columns.get_mut(column) {
                *resolved = true;
            }
            if let Some(cached_width) = self.widths.get_mut(column)
                && (*cached_width - width).abs() > f32::EPSILON
            {
                *cached_width = width;
                width_changed = true;
            }
        }

        if width_changed {
            self.metrics = None;
        }
        width_changed
    }
}

fn required_compact_item_width(
    entry: &fika_core::EntryData,
    options: CompactLayoutOptions,
    text_override_width: Option<f32>,
) -> f32 {
    let text_width = compact_text_width(entry.name_width_units)
        .max(text_override_width.unwrap_or_default().max(0.0));
    options.padding * 4.0 + options.icon_size + text_width
}

pub(crate) fn compact_text_width(name_width_units: u16) -> f32 {
    f32::from(name_width_units) * AVERAGE_COMPACT_CHAR_WIDTH
}

pub(crate) fn compact_text_width_for_name(name: &str) -> f32 {
    compact_text_width(compact_name_width_units(name))
}

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
    let options = super::compact_layout_options(view, 0.0);
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

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::{DirectoryModel, Entry, EntryData, ViewState};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: compact_name_width_units(name),
            size_bytes: 0,
            modified_secs: None,
            mime_type: None,
            thumbnail_path: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    #[test]
    fn compact_text_width_for_name_counts_non_ascii_as_double_width() {
        assert_eq!(compact_text_width_for_name("a目"), compact_text_width(3));
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
}
