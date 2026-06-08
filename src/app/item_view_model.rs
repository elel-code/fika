use crate::app::item_view_renderer::ItemViewMetadataSource;
use crate::fs::entries::RawFileEntry;
use crate::{FileEntry, ItemViewEntry};
use std::sync::Arc;

pub(crate) trait ItemViewModelEntry {
    fn model_name(&self) -> &str;
    fn model_path(&self) -> &str;
    fn model_group(&self) -> &str;
    fn model_location(&self) -> &str;
    fn model_kind(&self) -> &str;
    fn model_size(&self) -> &str;
    fn model_is_dir(&self) -> bool;
    fn model_size_bytes(&self) -> f32;
    fn model_modified(&self) -> &str;
    fn model_modified_age_days(&self) -> i32;
    fn model_name_width_units(&self) -> f32;

    fn model_has_location(&self) -> bool {
        !self.model_location().is_empty()
    }

    fn model_path_string(&self) -> String {
        self.model_path().to_string()
    }

    fn model_to_file_entry(&self) -> FileEntry {
        FileEntry {
            name: self.model_name().into(),
            path: self.model_path().into(),
            group: self.model_group().into(),
            location: self.model_location().into(),
            kind: self.model_kind().into(),
            size: self.model_size().into(),
            size_bytes: self.model_size_bytes(),
            modified: self.model_modified().into(),
            modified_age_days: self.model_modified_age_days(),
            is_dir: self.model_is_dir(),
        }
    }

    fn model_to_item_view_entry(&self) -> ItemViewEntry {
        ItemViewEntry {
            name: self.model_name().into(),
            path: self.model_path().into(),
            is_dir: self.model_is_dir(),
            thumbnail_state: 0,
            media_token: 0,
        }
    }

    fn model_metadata_source(&self) -> ItemViewMetadataSource {
        ItemViewMetadataSource::new(self.model_group(), self.model_location())
    }
}

pub(crate) type ItemViewModelEntryArc = Arc<dyn ItemViewModelEntry + Send + Sync>;

pub(crate) fn item_view_model_entry_with_group(
    entry: ItemViewModelEntryArc,
    group: String,
) -> ItemViewModelEntryArc {
    Arc::new(ItemViewModelEntryGroupOverride { entry, group })
}

struct ItemViewModelEntryGroupOverride {
    entry: ItemViewModelEntryArc,
    group: String,
}

impl ItemViewModelEntry for ItemViewModelEntryGroupOverride {
    fn model_name(&self) -> &str {
        self.entry.model_name()
    }

    fn model_path(&self) -> &str {
        self.entry.model_path()
    }

    fn model_group(&self) -> &str {
        self.group.as_str()
    }

    fn model_location(&self) -> &str {
        self.entry.model_location()
    }

    fn model_kind(&self) -> &str {
        self.entry.model_kind()
    }

    fn model_size(&self) -> &str {
        self.entry.model_size()
    }

    fn model_is_dir(&self) -> bool {
        self.entry.model_is_dir()
    }

    fn model_size_bytes(&self) -> f32 {
        self.entry.model_size_bytes()
    }

    fn model_modified(&self) -> &str {
        self.entry.model_modified()
    }

    fn model_modified_age_days(&self) -> i32 {
        self.entry.model_modified_age_days()
    }

    fn model_name_width_units(&self) -> f32 {
        self.entry.model_name_width_units()
    }
}

pub(crate) fn item_view_model_entries_equal(
    left: &(impl ItemViewModelEntry + ?Sized),
    right: &(impl ItemViewModelEntry + ?Sized),
) -> bool {
    left.model_name() == right.model_name()
        && left.model_path() == right.model_path()
        && left.model_group() == right.model_group()
        && left.model_location() == right.model_location()
        && left.model_kind() == right.model_kind()
        && left.model_size() == right.model_size()
        && left.model_is_dir() == right.model_is_dir()
        && left.model_size_bytes() == right.model_size_bytes()
        && left.model_modified() == right.model_modified()
        && left.model_modified_age_days() == right.model_modified_age_days()
        && left.model_name_width_units() == right.model_name_width_units()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemViewModelEntrySummary {
    pub(crate) count: usize,
    pub(crate) folders: usize,
    pub(crate) files: usize,
    pub(crate) has_locations: bool,
    pub(crate) paths: Option<Vec<String>>,
    pub(crate) locations: Option<Vec<String>>,
}

impl ItemViewModelEntrySummary {
    pub(crate) fn new(collect_paths: bool, collect_locations: bool) -> Self {
        Self {
            paths: collect_paths.then(Vec::new),
            locations: collect_locations.then(Vec::new),
            ..Self::default()
        }
    }

    pub(crate) fn push_entry(&mut self, entry: &(impl ItemViewModelEntry + ?Sized)) {
        self.count += 1;
        if entry.model_is_dir() {
            self.folders += 1;
        } else {
            self.files += 1;
        }
        self.has_locations |= entry.model_has_location();
        if let Some(paths) = self.paths.as_mut() {
            paths.push(entry.model_path_string());
        }
        if let Some(locations) = self.locations.as_mut() {
            locations.push(entry.model_location().to_string());
        }
    }
}

pub(crate) fn item_view_model_entry_summary<'a, T>(
    entries: impl IntoIterator<Item = &'a T>,
    collect_paths: bool,
    collect_locations: bool,
) -> ItemViewModelEntrySummary
where
    T: ItemViewModelEntry + ?Sized + 'a,
{
    let mut summary = ItemViewModelEntrySummary::new(collect_paths, collect_locations);
    for entry in entries {
        summary.push_entry(entry);
    }
    summary
}

impl<T> ItemViewModelEntry for &T
where
    T: ItemViewModelEntry + ?Sized,
{
    fn model_name(&self) -> &str {
        (**self).model_name()
    }

    fn model_path(&self) -> &str {
        (**self).model_path()
    }

    fn model_group(&self) -> &str {
        (**self).model_group()
    }

    fn model_location(&self) -> &str {
        (**self).model_location()
    }

    fn model_kind(&self) -> &str {
        (**self).model_kind()
    }

    fn model_size(&self) -> &str {
        (**self).model_size()
    }

    fn model_is_dir(&self) -> bool {
        (**self).model_is_dir()
    }

    fn model_size_bytes(&self) -> f32 {
        (**self).model_size_bytes()
    }

    fn model_modified(&self) -> &str {
        (**self).model_modified()
    }

    fn model_modified_age_days(&self) -> i32 {
        (**self).model_modified_age_days()
    }

    fn model_name_width_units(&self) -> f32 {
        (**self).model_name_width_units()
    }
}

impl<T> ItemViewModelEntry for Arc<T>
where
    T: ItemViewModelEntry + ?Sized,
{
    fn model_name(&self) -> &str {
        (**self).model_name()
    }

    fn model_path(&self) -> &str {
        (**self).model_path()
    }

    fn model_group(&self) -> &str {
        (**self).model_group()
    }

    fn model_location(&self) -> &str {
        (**self).model_location()
    }

    fn model_kind(&self) -> &str {
        (**self).model_kind()
    }

    fn model_size(&self) -> &str {
        (**self).model_size()
    }

    fn model_is_dir(&self) -> bool {
        (**self).model_is_dir()
    }

    fn model_size_bytes(&self) -> f32 {
        (**self).model_size_bytes()
    }

    fn model_modified(&self) -> &str {
        (**self).model_modified()
    }

    fn model_modified_age_days(&self) -> i32 {
        (**self).model_modified_age_days()
    }

    fn model_name_width_units(&self) -> f32 {
        (**self).model_name_width_units()
    }
}

impl ItemViewModelEntry for FileEntry {
    fn model_name(&self) -> &str {
        self.name.as_str()
    }

    fn model_path(&self) -> &str {
        self.path.as_str()
    }

    fn model_group(&self) -> &str {
        self.group.as_str()
    }

    fn model_location(&self) -> &str {
        self.location.as_str()
    }

    fn model_kind(&self) -> &str {
        self.kind.as_str()
    }

    fn model_size(&self) -> &str {
        self.size.as_str()
    }

    fn model_is_dir(&self) -> bool {
        self.is_dir
    }

    fn model_size_bytes(&self) -> f32 {
        self.size_bytes
    }

    fn model_modified(&self) -> &str {
        self.modified.as_str()
    }

    fn model_modified_age_days(&self) -> i32 {
        self.modified_age_days
    }

    fn model_name_width_units(&self) -> f32 {
        crate::app::geometry::compact_text_width_units(self.name.as_str())
    }
}

impl ItemViewModelEntry for RawFileEntry {
    fn model_name(&self) -> &str {
        self.name.as_str()
    }

    fn model_path(&self) -> &str {
        self.path.as_str()
    }

    fn model_group(&self) -> &str {
        self.group.as_str()
    }

    fn model_location(&self) -> &str {
        self.location.as_str()
    }

    fn model_kind(&self) -> &str {
        self.kind.as_str()
    }

    fn model_size(&self) -> &str {
        self.size.as_str()
    }

    fn model_is_dir(&self) -> bool {
        self.is_dir
    }

    fn model_size_bytes(&self) -> f32 {
        self.size_bytes as f32
    }

    fn model_modified(&self) -> &str {
        self.modified.as_str()
    }

    fn model_modified_age_days(&self) -> i32 {
        self.modified_age_days
    }

    fn model_name_width_units(&self) -> f32 {
        self.name_width_units
    }
}

pub(crate) fn item_view_filters_are_identity(
    query: &str,
    kind_filter: i32,
    modified_filter: i32,
    size_filter: i32,
    chooser_patterns: &[String],
) -> bool {
    query.is_empty()
        && kind_filter == 0
        && modified_filter == 0
        && size_filter == 0
        && chooser_patterns.is_empty()
}

pub(crate) fn item_view_entry_matches_filters(
    entry: &(impl ItemViewModelEntry + ?Sized),
    query: &str,
    kind_filter: i32,
    modified_filter: i32,
    size_filter: i32,
    chooser_patterns: &[String],
) -> bool {
    item_view_entry_matches_search_query(entry, query)
        && item_view_entry_matches_kind_filter(entry, kind_filter)
        && item_view_entry_matches_modified_filter(entry, modified_filter)
        && item_view_entry_matches_size_filter(entry, size_filter)
        && item_view_entry_matches_chooser_patterns(entry, chooser_patterns)
}

pub(crate) fn item_view_entry_matches_chooser_patterns(
    entry: &(impl ItemViewModelEntry + ?Sized),
    patterns: &[String],
) -> bool {
    entry.model_is_dir()
        || patterns.is_empty()
        || patterns
            .iter()
            .any(|pattern| item_view_glob_matches(pattern, entry.model_name()))
}

fn item_view_entry_matches_search_query(
    entry: &(impl ItemViewModelEntry + ?Sized),
    query: &str,
) -> bool {
    query.is_empty()
        || entry.model_name().to_ascii_lowercase().contains(query)
        || entry.model_path().to_ascii_lowercase().contains(query)
}

fn item_view_entry_matches_kind_filter(
    entry: &(impl ItemViewModelEntry + ?Sized),
    filter: i32,
) -> bool {
    match filter {
        1 => entry.model_is_dir(),
        2 => !entry.model_is_dir(),
        3 => !entry.model_is_dir() && item_view_is_image_path(entry.model_path()),
        _ => true,
    }
}

fn item_view_entry_matches_modified_filter(
    entry: &(impl ItemViewModelEntry + ?Sized),
    filter: i32,
) -> bool {
    match filter {
        1 => entry.model_modified_age_days() == 0,
        2 => entry.model_modified_age_days() >= 0 && entry.model_modified_age_days() <= 7,
        3 => entry.model_modified_age_days() >= 0 && entry.model_modified_age_days() <= 30,
        _ => true,
    }
}

fn item_view_entry_matches_size_filter(
    entry: &(impl ItemViewModelEntry + ?Sized),
    filter: i32,
) -> bool {
    if entry.model_is_dir() {
        return filter == 0;
    }

    match filter {
        1 => entry.model_size_bytes() < 1_048_576.0,
        2 => entry.model_size_bytes() >= 1_048_576.0 && entry.model_size_bytes() <= 104_857_600.0,
        3 => entry.model_size_bytes() > 104_857_600.0,
        _ => true,
    }
}

fn item_view_is_image_path(path: &str) -> bool {
    let Some(extension) = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "avif" | "bmp" | "gif" | "heic" | "heif" | "jpeg" | "jpg" | "png" | "svg" | "webp"
    )
}

fn item_view_glob_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    item_view_glob_matches_bytes(pattern.as_bytes(), text.as_bytes())
}

fn item_view_glob_matches_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let (mut pattern_index, mut text_index) = (0usize, 0usize);
    let mut star_index = None;
    let mut star_text_index = 0usize;

    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_text_index = text_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_text_index += 1;
            text_index = star_text_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::SharedString;
    use std::sync::Arc;

    fn file_entry(name: &str, path: &str, is_dir: bool) -> FileEntry {
        FileEntry {
            name: SharedString::from(name),
            path: SharedString::from(path),
            group: SharedString::new(),
            location: SharedString::new(),
            kind: SharedString::from(if is_dir { "Folder" } else { "File" }),
            size: SharedString::from("1 KB"),
            size_bytes: 1024.0,
            modified: SharedString::from("Today"),
            modified_age_days: 0,
            is_dir,
        }
    }

    fn raw_entry(name: &str, path: &str, location: &str) -> RawFileEntry {
        RawFileEntry {
            name: name.to_string(),
            name_width_units: crate::app::geometry::compact_text_width_units(name),
            path: path.to_string(),
            group: "Current folder".to_string(),
            location: location.to_string(),
            kind: "File".to_string(),
            size: "2 KB".to_string(),
            size_bytes: 2048,
            modified: "Yesterday".to_string(),
            modified_age_days: 1,
            is_dir: false,
        }
    }

    #[test]
    fn item_view_model_trait_projects_complete_entry_rows() {
        let raw = raw_entry("draft.md", "/tmp/docs/draft.md", "docs");
        let file = raw.model_to_file_entry();
        let item = raw.model_to_item_view_entry();
        let grouped = item_view_model_entry_with_group(
            Arc::new(raw.clone()) as ItemViewModelEntryArc,
            "Search result".to_string(),
        );
        let metadata = raw.model_metadata_source();

        assert_eq!(raw.model_name(), "draft.md");
        assert_eq!(raw.model_path(), "/tmp/docs/draft.md");
        assert_eq!(raw.model_group(), "Current folder");
        assert_eq!(raw.model_location(), "docs");
        assert_eq!(raw.model_kind(), "File");
        assert_eq!(raw.model_size(), "2 KB");
        assert_eq!(raw.model_size_bytes(), 2048.0);
        assert_eq!(raw.model_modified(), "Yesterday");
        assert_eq!(raw.model_modified_age_days(), 1);
        assert!(!raw.model_is_dir());
        assert_eq!(
            grouped.model_name_width_units(),
            raw.model_name_width_units()
        );

        assert_eq!(file.name, "draft.md");
        assert_eq!(file.path, "/tmp/docs/draft.md");
        assert_eq!(file.group, "Current folder");
        assert_eq!(file.location, "docs");
        assert_eq!(file.size_bytes, 2048.0);
        assert_eq!(file.modified_age_days, 1);
        assert_eq!(item.name, "draft.md");
        assert_eq!(item.path, "/tmp/docs/draft.md");
        assert!(!item.is_dir);
        assert_eq!(item.thumbnail_state, 0);
        assert_eq!(item.media_token, 0);
        assert_eq!(grouped.model_name(), "draft.md");
        assert_eq!(grouped.model_path(), "/tmp/docs/draft.md");
        assert_eq!(grouped.model_group(), "Search result");
        assert_eq!(grouped.model_location(), "docs");
        assert_eq!(grouped.model_size_bytes(), 2048.0);
        assert_eq!(metadata.group, "Current folder");
        assert_eq!(metadata.location, "docs");
    }

    #[test]
    fn item_view_model_summary_collects_counts_paths_and_locations() {
        let entries = [
            raw_entry("docs", "/tmp/docs", "."),
            raw_entry("draft.md", "/tmp/docs/draft.md", "docs"),
            raw_entry("notes.txt", "/tmp/docs/notes.txt", "docs"),
        ];
        let mut entries = entries.to_vec();
        entries[0].is_dir = true;

        let summary = item_view_model_entry_summary(entries.iter(), true, true);

        assert_eq!(summary.count, 3);
        assert_eq!(summary.folders, 1);
        assert_eq!(summary.files, 2);
        assert!(summary.has_locations);
        assert_eq!(
            summary.paths,
            Some(vec![
                "/tmp/docs".to_string(),
                "/tmp/docs/draft.md".to_string(),
                "/tmp/docs/notes.txt".to_string()
            ])
        );
        assert_eq!(
            summary.locations,
            Some(vec![
                ".".to_string(),
                "docs".to_string(),
                "docs".to_string()
            ])
        );

        let count_only = item_view_model_entry_summary(entries.iter(), false, false);
        assert_eq!(count_only.paths, None);
        assert_eq!(count_only.locations, None);
        assert_eq!(count_only.count, 3);
    }

    #[test]
    fn item_view_model_filters_work_for_file_entry_without_snapshot_conversion() {
        let image = file_entry("photo.PNG", "/tmp/photo.PNG", false);
        let directory = file_entry("Pictures", "/tmp/Pictures", true);
        let patterns = vec!["*.png".to_string()];

        assert!(item_view_entry_matches_filters(
            &image, "photo", 3, 1, 1, &patterns
        ));
        assert!(item_view_entry_matches_chooser_patterns(
            &directory, &patterns
        ));
        assert!(!item_view_entry_matches_filters(
            &image,
            "missing",
            0,
            0,
            0,
            &[]
        ));
    }

    #[test]
    fn item_view_filter_logic_is_centralized_for_selection_and_virtual_view() {
        let selection = include_str!("selection.rs");
        let virtual_view = include_str!("virtual_view.rs");

        for (name, source) in [("selection", selection), ("virtual_view", virtual_view)] {
            assert!(
                source.contains("item_view_entry_matches_filters")
                    && source.contains("item_view_filters_are_identity")
                    && !source.contains("fn matches_search_query")
                    && !source.contains("fn matches_kind_filter")
                    && !source.contains("fn matches_modified_filter")
                    && !source.contains("fn matches_size_filter")
                    && !source.contains("fn matches_chooser_patterns")
                    && !source.contains("fn snapshot_matches_search_query")
                    && !source.contains("fn snapshot_matches_kind_filter")
                    && !source.contains("fn snapshot_matches_modified_filter")
                    && !source.contains("fn snapshot_matches_size_filter")
                    && !source.contains("fn snapshot_matches_chooser_filter")
                    && !source.contains("fn glob_matches")
                    && !source.contains("fn snapshot_glob_matches")
                    && !source.contains("fn is_image_path")
                    && !source.contains("fn snapshot_is_image_path"),
                "{name} should consume item_view_model filters instead of owning duplicate file-model predicates"
            );
        }
    }

    #[test]
    fn model_store_uses_trait_object_backing_without_concrete_row() {
        let model = include_str!("item_view_model.rs");
        let pane = include_str!("pane.rs");
        let selection = include_str!("selection.rs");
        let main = include_str!("../main.rs");
        let row_type_name = ["ItemView", "ModelRow"].concat();
        let concrete_row_decl = format!("pub(crate) struct {row_type_name}");
        let owned_method = ["model_to", "_owned_entry"].concat();
        let borrowed_constructor = ["from_model", "_entries"].concat();
        let raw_constructor = ["from_raw", "_entries"].concat();

        assert!(
            !pane.contains("fn to_file_entry")
                && !pane.contains("fn to_item_view_entry")
                && !pane.contains(concrete_row_decl.as_str())
                && !model.contains(concrete_row_decl.as_str())
                && !model.contains(owned_method.as_str())
                && !pane.contains(owned_method.as_str())
                && !pane.contains(borrowed_constructor.as_str())
                && !pane.contains(raw_constructor.as_str())
                && model.contains("pub(crate) type ItemViewModelEntryArc")
                && pane.contains("Arc<[ItemViewModelEntryArc]>")
                && pane.contains("from_entries<T>")
                && pane.contains("entry_arc")
                && pane.contains("entry_arcs_range")
                && selection.contains("model_to_file_entry")
                && main.contains("model_metadata_source")
                && main.contains("model_to_item_view_entry"),
            "Pane entry storage should use model trait objects and ItemViewModelEntry defaults without a concrete row compatibility type"
        );
    }

    #[test]
    fn pane_entry_model_store_hides_concrete_row_container_from_consumers() {
        let pane = include_str!("pane.rs");
        let selection = include_str!("selection.rs");
        let virtual_view = include_str!("virtual_view.rs");
        let row_type_name = ["ItemView", "ModelRow"].concat();
        let deref_impl = ["impl Deref", " for PaneEntryModel"].concat();
        let as_ref_impl = ["impl AsRef", "<[ItemViewModelEntryArc]>"].concat();

        assert!(
            pane.contains("pub(crate) struct PaneEntryModel")
                && pane.contains("pub(crate) entries: PaneEntryModel")
                && pane.contains("pub(crate) entries: PaneEntryModel")
                && virtual_view.contains("pub(crate) entries: PaneEntryModel")
                && virtual_view.contains("VirtualViewSnapshotUpdate")
                && !pane.contains(deref_impl.as_str())
                && !pane.contains(as_ref_impl.as_str())
                && !virtual_view
                    .contains(format!("pub(crate) entries: Vec<{row_type_name}>").as_str())
                && !virtual_view
                    .contains(format!("pub(crate) entries: Arc<[{row_type_name}]>").as_str())
                && !selection.contains(format!("Iterator<Item = &{row_type_name}>").as_str())
                && !selection
                    .contains(format!("Iterator<Item = (usize, &{row_type_name})>").as_str()),
            "Pane entry storage should be exposed through PaneEntryModel and model-trait iterators, not concrete row containers"
        );
    }
}
