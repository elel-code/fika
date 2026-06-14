use fika_core::{DirectoryModel, FilteredModel, NameFilter, NameFilterMode, PaneId};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilterBarSnapshot {
    pub(crate) query: String,
    pub(crate) focused: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) mode: NameFilterMode,
    pub(crate) match_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaneFilterState {
    pub(crate) visible: bool,
    pub(crate) focused: bool,
    pub(crate) query: String,
    pub(crate) mode: NameFilterMode,
    pub(crate) case_sensitive: bool,
}

impl Default for PaneFilterState {
    fn default() -> Self {
        Self {
            visible: false,
            focused: false,
            query: String::new(),
            mode: NameFilterMode::Glob,
            case_sensitive: false,
        }
    }
}

impl PaneFilterState {
    pub(crate) fn active_filter(&self) -> Option<NameFilter> {
        if self.query.is_empty() {
            return None;
        }
        let filter = match self.mode {
            NameFilterMode::PlainText => NameFilter::plain_text(self.query.clone()),
            NameFilterMode::Glob => NameFilter::glob(self.query.clone()),
        }
        .with_case_sensitive(self.case_sensitive);
        Some(filter)
    }
}

pub(crate) fn filter_source_revision(filter: &NameFilter) -> u64 {
    let mut hasher = DefaultHasher::new();
    filter.hash(&mut hasher);
    match hasher.finish() {
        0 => 1,
        revision => revision,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilteredModelCacheKey {
    pub(crate) model_generation: u64,
    pub(crate) filter: NameFilter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilteredModelCacheEntry {
    pub(crate) key: FilteredModelCacheKey,
    pub(crate) model: FilteredModel,
}

pub(crate) fn cached_filtered_model_for_pane(
    pane_id: PaneId,
    pane_filters: &HashMap<PaneId, PaneFilterState>,
    filtered_models: &mut HashMap<PaneId, FilteredModelCacheEntry>,
    model: Option<&DirectoryModel>,
) -> Option<(FilteredModel, u64)> {
    let Some(filter) = pane_filters
        .get(&pane_id)
        .and_then(PaneFilterState::active_filter)
    else {
        filtered_models.remove(&pane_id);
        return None;
    };
    let source_revision = filter_source_revision(&filter);
    let model = model?;
    let key = FilteredModelCacheKey {
        model_generation: model.data_generation(),
        filter,
    };
    if let Some(cached) = filtered_models
        .get(&pane_id)
        .filter(|cached| cached.key == key)
    {
        return Some((cached.model.clone(), source_revision));
    }

    let filtered_model = FilteredModel::from_model(model, &key.filter);
    filtered_models.insert(
        pane_id,
        FilteredModelCacheEntry {
            key,
            model: filtered_model.clone(),
        },
    );
    Some((filtered_model, source_revision))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::{Entry, EntryData};
    use std::sync::Arc;

    #[test]
    fn filter_source_revision_is_stable_and_nonzero() {
        let filter = NameFilter::glob("*.rs").with_case_sensitive(true);
        let revision = filter_source_revision(&filter);

        assert_ne!(revision, 0);
        assert_eq!(revision, filter_source_revision(&filter));
    }

    #[test]
    fn cached_filtered_model_for_pane_reuses_cache_until_model_or_filter_changes() {
        let pane_id = PaneId(1);
        let mut filters = HashMap::new();
        let mut cache = HashMap::new();
        let mut model = test_model(["alpha.rs", "beta.txt", "gamma.rs"]);
        filters.insert(
            pane_id,
            PaneFilterState {
                visible: true,
                focused: true,
                query: "*.rs".to_string(),
                ..PaneFilterState::default()
            },
        );

        let first = cached_filtered_model_for_pane(pane_id, &filters, &mut cache, Some(&model))
            .expect("filtered model");
        let first_cached = cache.get(&pane_id).cloned().expect("cache entry");
        let second = cached_filtered_model_for_pane(pane_id, &filters, &mut cache, Some(&model))
            .expect("filtered model");

        assert_eq!(first.0.len(), 2);
        assert_eq!(second.0, first.0);
        assert_eq!(cache.get(&pane_id), Some(&first_cached));

        model.apply_items_added(vec![test_entry("delta.rs")]);
        let third = cached_filtered_model_for_pane(pane_id, &filters, &mut cache, Some(&model))
            .expect("filtered model");

        assert_eq!(third.0.len(), 3);
        assert_ne!(cache.get(&pane_id), Some(&first_cached));
    }

    #[test]
    fn cached_filtered_model_for_pane_clears_cache_without_active_filter() {
        let pane_id = PaneId(1);
        let filters = HashMap::new();
        let mut cache = HashMap::new();
        let model = test_model(["alpha.rs"]);
        cache.insert(
            pane_id,
            FilteredModelCacheEntry {
                key: FilteredModelCacheKey {
                    model_generation: model.data_generation(),
                    filter: NameFilter::glob("*.rs"),
                },
                model: FilteredModel::from_model(&model, &NameFilter::glob("*.rs")),
            },
        );

        assert!(
            cached_filtered_model_for_pane(pane_id, &filters, &mut cache, Some(&model)).is_none()
        );
        assert!(!cache.contains_key(&pane_id));
    }

    fn test_model<const N: usize>(names: [&str; N]) -> DirectoryModel {
        let entries = names.into_iter().map(test_entry).collect::<Vec<_>>();
        let mut model = DirectoryModel::for_directory("/tmp/fika-filter-cache".into());
        model.replace_listing("/tmp/fika-filter-cache".into(), Arc::new(entries));
        model
    }

    fn test_entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
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
}
