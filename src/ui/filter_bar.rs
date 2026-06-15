mod icon;
mod state;

pub(crate) use icon::{FilterToggleSnapshot, filter_toggle_snapshot};
pub(crate) use state::{
    FilterBarSnapshot, FilteredModelCacheEntry, PaneFilterState, cached_filtered_model_for_pane,
};

pub(crate) const FILTER_BAR_HEIGHT: f32 = 35.0;
