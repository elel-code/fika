use fika_core::{FilteredModel, NameFilter, NameFilterMode};

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
