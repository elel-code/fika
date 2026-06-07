use crate::FileEntry;
use crate::app::pane::PaneEntrySnapshot;

pub(crate) trait ItemViewModelEntry {
    fn model_name(&self) -> &str;
    fn model_path(&self) -> &str;
    fn model_is_dir(&self) -> bool;
    fn model_size_bytes(&self) -> f32;
    fn model_modified_age_days(&self) -> i32;
    fn model_name_width_units(&self) -> f32;
}

impl ItemViewModelEntry for PaneEntrySnapshot {
    fn model_name(&self) -> &str {
        self.name.as_str()
    }

    fn model_path(&self) -> &str {
        self.path.as_str()
    }

    fn model_is_dir(&self) -> bool {
        self.is_dir
    }

    fn model_size_bytes(&self) -> f32 {
        self.size_bytes
    }

    fn model_modified_age_days(&self) -> i32 {
        self.modified_age_days
    }

    fn model_name_width_units(&self) -> f32 {
        self.name_width_units
    }
}

impl ItemViewModelEntry for FileEntry {
    fn model_name(&self) -> &str {
        self.name.as_str()
    }

    fn model_path(&self) -> &str {
        self.path.as_str()
    }

    fn model_is_dir(&self) -> bool {
        self.is_dir
    }

    fn model_size_bytes(&self) -> f32 {
        self.size_bytes
    }

    fn model_modified_age_days(&self) -> i32 {
        self.modified_age_days
    }

    fn model_name_width_units(&self) -> f32 {
        crate::app::geometry::compact_text_width_units(self.name.as_str())
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
}
