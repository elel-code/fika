use std::path::PathBuf;

use fika_core::{CompactLayout, FilteredModel};

use super::layout::model_index_for_layout_index;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContentItemHit {
    pub(crate) model_index: usize,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PaneLayoutProjection {
    pub(crate) layout: CompactLayout,
    pub(crate) filtered: Option<FilteredModel>,
}

impl PaneLayoutProjection {
    pub(crate) fn new(layout: CompactLayout, filtered: Option<FilteredModel>) -> Self {
        Self { layout, filtered }
    }

    pub(crate) fn model_index_for_layout_index(&self, layout_index: usize) -> Option<usize> {
        model_index_for_layout_index(self.filtered.as_ref(), layout_index)
    }

    pub(crate) fn layout_index_for_model_index(&self, model_index: usize) -> Option<usize> {
        self.filtered
            .as_ref()
            .map_or(Some(model_index), |filtered| {
                filtered.layout_index_for_model_index(model_index)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::{CompactLayoutOptions, DirectoryModel, Entry, EntryData, NameFilter};
    use std::sync::Arc;

    #[test]
    fn projection_maps_filtered_layout_indexes_to_model_indexes() {
        let entries = Arc::new(vec![
            test_entry("alpha.txt"),
            test_entry("beta.txt"),
            test_entry("gamma.txt"),
        ]);
        let mut model = DirectoryModel::for_directory("/tmp/fika-projection".into());
        model.replace_listing("/tmp/fika-projection".into(), entries);
        let filtered = FilteredModel::from_model(&model, &NameFilter::glob("beta.txt"));
        let projection = PaneLayoutProjection::new(
            CompactLayout::new(1, CompactLayoutOptions::default()),
            Some(filtered),
        );

        assert_eq!(projection.model_index_for_layout_index(0), Some(1));
        assert_eq!(projection.layout_index_for_model_index(1), Some(0));
        assert_eq!(projection.layout_index_for_model_index(0), None);
    }

    fn test_entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            mime_type: None,
            thumbnail_path: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }
}
