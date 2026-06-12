use fika_core::{FilteredModel, NameFilter, NameFilterMode};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_source_revision_is_stable_and_nonzero() {
        let filter = NameFilter::glob("*.rs").with_case_sensitive(true);
        let revision = filter_source_revision(&filter);

        assert_ne!(revision, 0);
        assert_eq!(revision, filter_source_revision(&filter));
    }
}
