use crate::ItemViewEntry;
use crate::app::pane::PaneView;
use slint::{Model, ModelRc, VecModel};
use std::collections::HashSet;
use std::rc::Rc;

pub(crate) fn new_item_view_entries_model(entries: Vec<ItemViewEntry>) -> ModelRc<ItemViewEntry> {
    ModelRc::new(Rc::new(VecModel::from(entries)))
}

pub(crate) fn update_item_view_entries_model(
    current: &ModelRc<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    entries: Vec<ItemViewEntry>,
) -> Option<ModelRc<ItemViewEntry>> {
    let Some(model) = current.as_any().downcast_ref::<VecModel<ItemViewEntry>>() else {
        return Some(new_item_view_entries_model(entries));
    };

    update_vec_model(model, old_start, new_start, entries);
    None
}

pub(crate) fn update_pane_item_view_entries_model(
    view: &mut PaneView,
    start_index: usize,
    start_column: usize,
    entries: Vec<ItemViewEntry>,
) {
    let current = view.virtual_entries.clone();
    let old_start = view.virtual_start_index;
    if let Some(model) = update_item_view_entries_model(&current, old_start, start_index, entries) {
        view.virtual_entries = model;
    }
    view.virtual_start_index = start_index;
    view.virtual_start_column = start_column;
}

pub(crate) fn update_item_view_entries_model_selection(
    current: &ModelRc<ItemViewEntry>,
    selected_paths: &[String],
) -> bool {
    let Some(model) = current.as_any().downcast_ref::<VecModel<ItemViewEntry>>() else {
        return false;
    };
    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut changed = false;
    for row in 0..model.row_count() {
        let Some(mut entry) = model.row_data(row) else {
            continue;
        };
        let selected = selected.contains(entry.path.as_str());
        if entry.selected != selected {
            entry.selected = selected;
            model.set_row_data(row, entry);
            changed = true;
        }
    }
    changed
}

fn update_vec_model(
    model: &VecModel<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    entries: Vec<ItemViewEntry>,
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
    use crate::app::pane::PaneView;
    use slint::Image;

    fn entry(index: usize) -> ItemViewEntry {
        ItemViewEntry {
            name: format!("item-{index}").into(),
            path: format!("/tmp/item-{index}").into(),
            group: String::new().into(),
            location: String::new().into(),
            is_dir: false,
            selected: false,
            thumbnail_state: 0,
            thumbnail: Image::default(),
            tile_width: 0.0,
            tile_height: 0.0,
            media_x: 0.0,
            media_y: 0.0,
            text_x: 0.0,
            text_width: 0.0,
            group_y: 0.0,
            title_y: 0.0,
            location_y: 0.0,
            metadata_line_height: 0.0,
            title_line_height: 0.0,
            thumbnail_width: 0.0,
            thumbnail_height: 0.0,
            metadata_font_size: 0.0,
            title_font_size: 0.0,
            glyph_doc_font_size: 0.0,
        }
    }

    fn rows(model: &ModelRc<ItemViewEntry>) -> Vec<String> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| entry.path.to_string())
            .collect()
    }

    fn selected_rows(model: &ModelRc<ItemViewEntry>) -> Vec<String> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .filter(|entry| entry.selected)
            .map(|entry| entry.path.to_string())
            .collect()
    }

    #[test]
    fn pane_item_view_entry_model_updates_each_view_independently() {
        let mut left = PaneView::default();
        let mut right = PaneView::default();

        update_pane_item_view_entries_model(&mut left, 0, 0, (0..3).map(entry).collect());
        update_pane_item_view_entries_model(&mut right, 20, 4, (20..23).map(entry).collect());

        assert_eq!(left.virtual_start_index, 0);
        assert_eq!(left.virtual_start_column, 0);
        assert_eq!(right.virtual_start_index, 20);
        assert_eq!(right.virtual_start_column, 4);
        assert_eq!(
            rows(&left.virtual_entries),
            (0..3)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            rows(&right.virtual_entries),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );

        update_pane_item_view_entries_model(&mut right, 22, 5, (22..25).map(entry).collect());

        assert_eq!(
            rows(&left.virtual_entries),
            (0..3)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(right.virtual_start_index, 22);
        assert_eq!(right.virtual_start_column, 5);
    }

    #[test]
    fn item_view_entry_model_reuses_vec_model_when_range_slides_forward() {
        let model = new_item_view_entries_model((0..6).map(entry).collect());
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 0, 2, (2..8).map(entry).collect()).is_none()
        );

        assert_eq!(model, original);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_reuses_vec_model_when_range_slides_backward() {
        let model = new_item_view_entries_model((4..10).map(entry).collect());
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 4, 2, (2..8).map(entry).collect()).is_none()
        );

        assert_eq!(model, original);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_resets_same_vec_model_without_overlap() {
        let model = new_item_view_entries_model((0..3).map(entry).collect());
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 0, 20, (20..23).map(entry).collect()).is_none()
        );

        assert_eq!(model, original);
        assert_eq!(
            rows(&model),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pane_item_view_entry_model_updates_selection_without_replacing_entries() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(&mut view, 0, 0, (0..4).map(entry).collect());
        let original = view.virtual_entries.clone();

        assert!(update_item_view_entries_model_selection(
            &view.virtual_entries,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_entries, original);
        assert_eq!(
            selected_rows(&view.virtual_entries),
            vec!["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        );

        assert!(!update_item_view_entries_model_selection(
            &view.virtual_entries,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));

        assert!(update_item_view_entries_model_selection(
            &view.virtual_entries,
            &[]
        ));
        assert!(selected_rows(&view.virtual_entries).is_empty());
    }
}
