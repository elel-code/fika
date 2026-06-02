use crate::FileEntry;
use crate::fs::search;
use crate::support::generation::GenerationCounter;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[derive(Debug)]
pub(crate) struct PaneState {
    pub(crate) current_dir: PathBuf,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) history: PaneHistory,
    pub(crate) selection: PaneSelection,
    pub(crate) search: PaneSearch,
    pub(crate) search_cancel: Option<Arc<AtomicBool>>,
    pub(crate) search_progress: search::SearchProgress,
    pub(crate) search_generation: GenerationCounter,
    pub(crate) view: PaneView,
}

impl PaneState {
    pub(crate) fn new(current_dir: PathBuf) -> Self {
        Self {
            current_dir,
            entries: Vec::new(),
            history: PaneHistory::default(),
            selection: PaneSelection::default(),
            search: PaneSearch::default(),
            search_cancel: None,
            search_progress: search::SearchProgress::default(),
            search_generation: GenerationCounter::default(),
            view: PaneView::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaneNavigation {
    pub(crate) previous: PathBuf,
    pub(crate) target: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneSelection {
    pub(crate) paths: Vec<String>,
    pub(crate) anchor: Option<String>,
}

impl PaneSelection {
    pub(crate) fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneSearch {
    pub(crate) query: String,
    pub(crate) kind_filter: i32,
    pub(crate) modified_filter: i32,
    pub(crate) size_filter: i32,
    pub(crate) visible_entry_indices: Option<Vec<usize>>,
}

impl PaneSearch {
    pub(crate) fn reset_all(&mut self) {
        self.query.clear();
        self.kind_filter = 0;
        self.modified_filter = 0;
        self.size_filter = 0;
        self.visible_entry_indices = None;
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaneView {
    pub(crate) virtual_view: VirtualViewCache,
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewCache {
    pub(crate) range: Range<usize>,
    pub(crate) entry_count: usize,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) thumbnail_size_px: u32,
}

impl Default for VirtualViewCache {
    fn default() -> Self {
        Self {
            range: 0..0,
            entry_count: 0,
            rows_per_column: 0,
            cell_width: 0.0,
            thumbnail_size_px: 0,
        }
    }
}

impl VirtualViewCache {
    pub(crate) fn invalidate(&mut self) {
        self.range = 0..0;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneHistory {
    back_stack: Vec<PathBuf>,
    forward_stack: Vec<PathBuf>,
}

impl PaneHistory {
    pub(crate) fn navigate_from(&mut self, previous: PathBuf, target: PathBuf) -> PaneNavigation {
        self.back_stack.push(previous.clone());
        self.forward_stack.clear();
        PaneNavigation { previous, target }
    }

    pub(crate) fn go_back_from(&mut self, previous: PathBuf) -> Option<PaneNavigation> {
        let target = self.back_stack.pop()?;
        self.forward_stack.push(previous.clone());
        Some(PaneNavigation { previous, target })
    }

    pub(crate) fn go_forward_from(&mut self, previous: PathBuf) -> Option<PaneNavigation> {
        let target = self.forward_stack.pop()?;
        self.back_stack.push(previous.clone());
        Some(PaneNavigation { previous, target })
    }

    pub(crate) fn prune_under(&mut self, mount_path: &Path) {
        self.back_stack.retain(|path| !path.starts_with(mount_path));
        self.forward_stack
            .retain(|path| !path.starts_with(mount_path));
    }

    pub(crate) fn back_len(&self) -> usize {
        self.back_stack.len()
    }

    pub(crate) fn forward_len(&self) -> usize {
        self.forward_stack.len()
    }

    #[cfg(test)]
    pub(crate) fn from_stacks(back_stack: Vec<PathBuf>, forward_stack: Vec<PathBuf>) -> Self {
        Self {
            back_stack,
            forward_stack,
        }
    }

    #[cfg(test)]
    pub(crate) fn back_paths(&self) -> &[PathBuf] {
        &self.back_stack
    }

    #[cfg(test)]
    pub(crate) fn forward_paths(&self) -> &[PathBuf] {
        &self.forward_stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_history_navigation_keeps_back_and_forward_independent() {
        let mut history = PaneHistory::default();
        let mut current = PathBuf::from("/home/yk");

        let nav = history.navigate_from(current.clone(), PathBuf::from("/tmp"));
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/home/yk"));
        assert_eq!(nav.target, PathBuf::from("/tmp"));
        assert_eq!(current, PathBuf::from("/tmp"));
        assert_eq!(history.back_paths(), &[PathBuf::from("/home/yk")]);
        assert!(history.forward_paths().is_empty());

        let nav = history.go_back_from(current.clone()).unwrap();
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/tmp"));
        assert_eq!(nav.target, PathBuf::from("/home/yk"));
        assert_eq!(current, PathBuf::from("/home/yk"));
        assert!(history.back_paths().is_empty());
        assert_eq!(history.forward_paths(), &[PathBuf::from("/tmp")]);

        let nav = history.go_forward_from(current.clone()).unwrap();
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/home/yk"));
        assert_eq!(nav.target, PathBuf::from("/tmp"));
        assert_eq!(current, PathBuf::from("/tmp"));
        assert_eq!(history.back_paths(), &[PathBuf::from("/home/yk")]);
        assert!(history.forward_paths().is_empty());
    }

    #[test]
    fn pane_history_prunes_removed_mount_paths() {
        let mount_path = PathBuf::from("/run/media/yk/USB");
        let mut history = PaneHistory::from_stacks(
            vec![PathBuf::from("/tmp"), mount_path.join("old")],
            vec![
                mount_path.join("future"),
                PathBuf::from("/run/media/yk/USB-sibling"),
            ],
        );

        history.prune_under(&mount_path);

        assert_eq!(history.back_paths(), &[PathBuf::from("/tmp")]);
        assert_eq!(
            history.forward_paths(),
            &[PathBuf::from("/run/media/yk/USB-sibling")]
        );
    }

    #[test]
    fn pane_selection_clear_resets_paths_and_anchor() {
        let mut selection = PaneSelection {
            paths: vec!["/tmp/a".to_string()],
            anchor: Some("/tmp/a".to_string()),
        };

        selection.clear();

        assert!(selection.paths.is_empty());
        assert!(selection.anchor.is_none());
    }

    #[test]
    fn pane_search_reset_all_clears_query_filters_and_visible_indices() {
        let mut search = PaneSearch {
            query: "report".to_string(),
            kind_filter: 1,
            modified_filter: 2,
            size_filter: 3,
            visible_entry_indices: Some(vec![0, 2, 4]),
        };

        search.reset_all();

        assert_eq!(search.query, "");
        assert_eq!(search.kind_filter, 0);
        assert_eq!(search.modified_filter, 0);
        assert_eq!(search.size_filter, 0);
        assert!(search.visible_entry_indices.is_none());
    }

    #[test]
    fn pane_state_owns_recursive_search_runtime() {
        let mut pane = PaneState::new(PathBuf::from("/tmp"));
        let cancel = Arc::new(AtomicBool::new(false));

        pane.search_cancel = Some(cancel);
        pane.search_progress = search::SearchProgress {
            directories_scanned: 4,
            matches_found: 2,
        };
        let generation = pane.search_generation.next();

        assert!(pane.search_cancel.is_some());
        assert_eq!(pane.search_progress.directories_scanned, 4);
        assert_eq!(pane.search_progress.matches_found, 2);
        assert!(pane.search_generation.is_current(generation));
    }

    #[test]
    fn pane_view_virtual_cache_invalidate_keeps_metrics_but_clears_range() {
        let mut view = PaneView {
            virtual_view: VirtualViewCache {
                range: 4..12,
                entry_count: 64,
                rows_per_column: 8,
                cell_width: 96.0,
                thumbnail_size_px: 128,
            },
        };

        view.virtual_view.invalidate();

        assert!(view.virtual_view.range.is_empty());
        assert_eq!(view.virtual_view.entry_count, 64);
        assert_eq!(view.virtual_view.rows_per_column, 8);
        assert_eq!(view.virtual_view.cell_width, 96.0);
        assert_eq!(view.virtual_view.thumbnail_size_px, 128);
    }
}
