use crate::FileEntry;
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

pub(crate) fn new_file_entries_model(entries: Vec<FileEntry>) -> ModelRc<FileEntry> {
    ModelRc::new(Rc::new(VecModel::from(entries)))
}

pub(crate) fn update_file_entries_model(
    current: &ModelRc<FileEntry>,
    old_start: usize,
    new_start: usize,
    entries: Vec<FileEntry>,
) -> Option<ModelRc<FileEntry>> {
    let Some(model) = current.as_any().downcast_ref::<VecModel<FileEntry>>() else {
        return Some(new_file_entries_model(entries));
    };

    update_vec_model(model, old_start, new_start, entries);
    None
}

fn update_vec_model(
    model: &VecModel<FileEntry>,
    old_start: usize,
    new_start: usize,
    entries: Vec<FileEntry>,
) {
    let old_len = model.row_count();
    if old_len == 0 || entries.is_empty() {
        model.set_vec(entries);
        return;
    }

    let old_end = old_start.saturating_add(old_len);
    let new_end = new_start.saturating_add(entries.len());
    let overlap_start = old_start.max(new_start);
    let overlap_end = old_end.min(new_end);

    if overlap_start >= overlap_end {
        model.set_vec(entries);
        return;
    }

    if new_start > old_start {
        let remove_count = (new_start - old_start).min(model.row_count());
        for _ in 0..remove_count {
            model.remove(0);
        }
    } else if new_start < old_start {
        let prefix_len = (old_start - new_start).min(entries.len());
        for entry in entries[..prefix_len].iter().rev() {
            model.insert(0, entry.clone());
        }
    }

    let overlap_rows = overlap_start - new_start..overlap_end - new_start;
    for row in overlap_rows {
        if model.row_data(row).as_ref() != Some(&entries[row]) {
            model.set_row_data(row, entries[row].clone());
        }
    }

    while model.row_count() > entries.len() {
        model.remove(model.row_count() - 1);
    }

    let current_len = model.row_count();
    if current_len < entries.len() {
        model.extend(entries[current_len..].iter().cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Image;

    fn entry(index: usize) -> FileEntry {
        FileEntry {
            name: format!("item-{index}").into(),
            path: format!("/tmp/item-{index}").into(),
            group: String::new().into(),
            location: String::new().into(),
            kind: "File".into(),
            size: "1 KB".into(),
            size_bytes: 1024.0,
            modified: "Today".into(),
            modified_age_days: 0,
            is_dir: false,
            thumbnail_state: 0,
            thumbnail: Image::default(),
        }
    }

    fn rows(model: &ModelRc<FileEntry>) -> Vec<String> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| entry.path.to_string())
            .collect()
    }

    #[test]
    fn file_entry_model_reuses_vec_model_when_range_slides_forward() {
        let model = new_file_entries_model((0..6).map(entry).collect());
        let original = model.clone();

        assert!(update_file_entries_model(&model, 0, 2, (2..8).map(entry).collect()).is_none());

        assert_eq!(model, original);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn file_entry_model_reuses_vec_model_when_range_slides_backward() {
        let model = new_file_entries_model((4..10).map(entry).collect());
        let original = model.clone();

        assert!(update_file_entries_model(&model, 4, 2, (2..8).map(entry).collect()).is_none());

        assert_eq!(model, original);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn file_entry_model_resets_same_vec_model_without_overlap() {
        let model = new_file_entries_model((0..3).map(entry).collect());
        let original = model.clone();

        assert!(update_file_entries_model(&model, 0, 20, (20..23).map(entry).collect()).is_none());

        assert_eq!(model, original);
        assert_eq!(
            rows(&model),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }
}
