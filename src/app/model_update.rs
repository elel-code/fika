use crate::ItemViewEntry;
use crate::app::item_view::ItemViewRowToken;
use crate::app::pane::PaneView;
use slint::{Model, ModelRc, VecModel};
use std::collections::HashSet;
use std::rc::Rc;

pub(crate) fn new_item_view_entries_model(entries: Vec<ItemViewEntry>) -> ModelRc<ItemViewEntry> {
    ModelRc::new(Rc::new(VecModel::from(entries)))
}

fn item_view_row_tokens(entries: &[ItemViewEntry]) -> Vec<ItemViewRowToken> {
    entries.iter().map(ItemViewRowToken::from_entry).collect()
}

pub(crate) fn update_item_view_entries_model(
    current: &ModelRc<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    current_tokens: &mut Vec<ItemViewRowToken>,
    entries: Vec<ItemViewEntry>,
) -> Option<ModelRc<ItemViewEntry>> {
    let mut next_tokens = item_view_row_tokens(&entries);
    let Some(model) = current.as_any().downcast_ref::<VecModel<ItemViewEntry>>() else {
        *current_tokens = next_tokens;
        return Some(new_item_view_entries_model(entries));
    };

    update_vec_model(
        model,
        old_start,
        new_start,
        current_tokens,
        &mut next_tokens,
        entries,
    );
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
    if let Some(model) = update_item_view_entries_model(
        &current,
        old_start,
        start_index,
        &mut view.virtual_entry_tokens,
        entries,
    ) {
        view.virtual_entries = model;
    }
    view.virtual_start_index = start_index;
    view.virtual_start_column = start_column;
}

pub(crate) fn update_item_view_entry_selection_tokens(
    current_tokens: &mut [ItemViewRowToken],
    selected_paths: &[String],
) -> Vec<(usize, bool)> {
    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut updates = Vec::new();
    for (row, token) in current_tokens.iter_mut().enumerate() {
        let selected = selected.contains(token.path());
        if token.selected() != selected {
            token.set_selected(selected);
            updates.push((row, selected));
        }
    }
    updates
}

pub(crate) fn apply_item_view_entry_selection_updates(
    current: &ModelRc<ItemViewEntry>,
    updates: &[(usize, bool)],
) -> bool {
    let Some(model) = current.as_any().downcast_ref::<VecModel<ItemViewEntry>>() else {
        return false;
    };
    let mut changed = false;
    for &(row, selected) in updates {
        let Some(mut entry) = model.row_data(row) else {
            continue;
        };
        if entry.selected == selected {
            continue;
        }
        entry.selected = selected;
        model.set_row_data(row, entry);
        changed = true;
    }
    changed
}

#[cfg(test)]
fn update_item_view_entries_model_selection(
    current: &ModelRc<ItemViewEntry>,
    current_tokens: &mut [ItemViewRowToken],
    selected_paths: &[String],
) -> bool {
    if current
        .as_any()
        .downcast_ref::<VecModel<ItemViewEntry>>()
        .is_none()
    {
        return false;
    }
    let updates = update_item_view_entry_selection_tokens(current_tokens, selected_paths);
    apply_item_view_entry_selection_updates(current, &updates)
}

fn update_vec_model(
    model: &VecModel<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    current_tokens: &mut Vec<ItemViewRowToken>,
    next_tokens: &mut Vec<ItemViewRowToken>,
    entries: Vec<ItemViewEntry>,
) {
    let old_len = model.row_count();
    if old_len == 0 || entries.is_empty() {
        model.set_vec(entries);
        *current_tokens = std::mem::take(next_tokens);
        return;
    }

    let old_end = old_start.saturating_add(old_len);
    let new_end = new_start.saturating_add(entries.len());
    let overlap_start = old_start.max(new_start);
    let overlap_end = old_end.min(new_end);

    if overlap_start >= overlap_end {
        model.set_vec(entries);
        *current_tokens = std::mem::take(next_tokens);
        return;
    }

    if new_start > old_start {
        let remove_count = (new_start - old_start).min(model.row_count());
        for _ in 0..remove_count {
            model.remove(0);
        }
        current_tokens.drain(0..remove_count.min(current_tokens.len()));
    } else if new_start < old_start {
        let prefix_len = (old_start - new_start).min(entries.len());
        for entry in entries[..prefix_len].iter().rev() {
            model.insert(0, entry.clone());
        }
        current_tokens.splice(0..0, next_tokens[..prefix_len].iter().cloned());
    }

    let overlap_rows = overlap_start - new_start..overlap_end - new_start;
    for row in overlap_rows {
        if current_tokens.get(row) != next_tokens.get(row) {
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
    *current_tokens = std::mem::take(next_tokens);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pane::PaneView;
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

    fn entry(index: usize) -> ItemViewEntry {
        ItemViewEntry {
            name: format!("item-{index}").into(),
            path: format!("/tmp/item-{index}").into(),
            group: String::new().into(),
            location: String::new().into(),
            is_dir: false,
            selected: false,
            thumbnail_state: 0,
            media: Image::default(),
            media_token: 0,
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
            media_width: 0.0,
            media_height: 0.0,
            metadata_font_size: 0.0,
            title_font_size: 0.0,
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

    fn colored_image(pixel: Rgba8Pixel) -> Image {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        buffer.make_mut_slice()[0] = pixel;
        Image::from_rgba8(buffer)
    }

    fn first_pixel(image: &Image) -> Rgba8Pixel {
        image
            .to_rgba8()
            .expect("test image should be rgba")
            .as_slice()[0]
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
        let initial_entries = (0..6).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 0, 2, &mut tokens, (2..8).map(entry).collect())
                .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(tokens.len(), 6);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_reuses_vec_model_when_range_slides_backward() {
        let initial_entries = (4..10).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 4, 2, &mut tokens, (2..8).map(entry).collect())
                .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(tokens.len(), 6);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_resets_same_vec_model_without_overlap() {
        let initial_entries = (0..3).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                20,
                &mut tokens,
                (20..23).map(entry).collect()
            )
            .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(tokens.len(), 3);
        assert_eq!(
            rows(&model),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_repairs_missing_sidecar_tokens_after_update() {
        let initial_entries = (0..6).map(entry).collect::<Vec<_>>();
        let mut tokens = Vec::new();
        let model = new_item_view_entries_model(initial_entries);

        assert!(
            update_item_view_entries_model(&model, 0, 2, &mut tokens, (2..8).map(entry).collect())
                .is_none()
        );

        assert_eq!(tokens.len(), 6);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_uses_media_token_instead_of_image_comparison_for_overlap() {
        let mut old_entry = entry(0);
        old_entry.media = colored_image(Rgba8Pixel::new(255, 0, 0, 255));
        old_entry.media_token = 42;
        let initial_entries = vec![old_entry];
        let mut tokens = item_view_row_tokens(&initial_entries);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        let mut same_token_entry = entry(0);
        same_token_entry.media = colored_image(Rgba8Pixel::new(0, 0, 255, 255));
        same_token_entry.media_token = 42;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![same_token_entry])
                .is_none()
        );

        assert_eq!(model, original);
        let unchanged = model.row_data(0).expect("row should remain present");
        assert_eq!(
            first_pixel(&unchanged.media),
            Rgba8Pixel::new(255, 0, 0, 255)
        );

        let mut new_token_entry = entry(0);
        new_token_entry.media = colored_image(Rgba8Pixel::new(0, 0, 255, 255));
        new_token_entry.media_token = 43;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![new_token_entry])
                .is_none()
        );

        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.media_token, 43);
        assert_eq!(first_pixel(&updated.media), Rgba8Pixel::new(0, 0, 255, 255));
    }

    #[test]
    fn pane_item_view_entry_model_updates_selection_without_replacing_entries() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(&mut view, 0, 0, (0..4).map(entry).collect());
        let original = view.virtual_entries.clone();

        assert!(update_item_view_entries_model_selection(
            &view.virtual_entries,
            &mut view.virtual_entry_tokens,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_entries, original);
        assert_eq!(
            selected_rows(&view.virtual_entries),
            vec!["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        );

        assert!(!update_item_view_entries_model_selection(
            &view.virtual_entries,
            &mut view.virtual_entry_tokens,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));

        assert!(update_item_view_entries_model_selection(
            &view.virtual_entries,
            &mut view.virtual_entry_tokens,
            &[]
        ));
        assert!(selected_rows(&view.virtual_entries).is_empty());
    }

    #[test]
    fn virtual_row_reuse_compares_tokens_without_cloning_existing_rows() {
        let source = include_str!("model_update.rs");
        let body = source
            .split_once("fn update_vec_model(")
            .and_then(|(_, rest)| rest.split_once("#[cfg(test)]"))
            .map(|(body, _)| body)
            .expect("update_vec_model body should be present");
        let overlap_body = body
            .split_once("for row in overlap_rows {")
            .and_then(|(_, rest)| rest.split_once("while model.row_count() > entries.len()"))
            .map(|(body, _)| body)
            .expect("overlap loop should be present");

        assert!(
            overlap_body.contains("current_tokens.get(row) != next_tokens.get(row)")
                && overlap_body.contains("model.set_row_data(row, entries[row].clone());")
                && !overlap_body.contains(".row_data("),
            "overlap row reuse should compare the Rust sidecar token instead of cloning current ItemViewEntry rows from Slint"
        );
    }
}
