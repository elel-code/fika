use crate::app::item_view::ItemViewRowToken;
use crate::app::pane::PaneView;
use crate::{ItemViewEntry, ItemViewMetadataEntry};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

pub(crate) fn new_item_view_entries_model(entries: Vec<ItemViewEntry>) -> ModelRc<ItemViewEntry> {
    ModelRc::new(Rc::new(VecModel::from(entries)))
}

pub(crate) fn new_item_view_metadata_model(
    entries: &[ItemViewEntry],
    show_location: bool,
) -> ModelRc<ItemViewMetadataEntry> {
    if !show_location {
        return ModelRc::default();
    }

    let mut metadata = Vec::new();
    for (row, entry) in entries.iter().enumerate() {
        if !entry.group.is_empty() {
            metadata.push(ItemViewMetadataEntry {
                slice_index: row as i32,
                text: entry.group.clone(),
                text_x: entry.text_x,
                text_width: entry.text_width,
                y: entry.group_y,
                line_height: entry.metadata_line_height,
                font_size: entry.metadata_font_size,
                is_group: true,
            });
        }
        if !entry.location.is_empty() {
            metadata.push(ItemViewMetadataEntry {
                slice_index: row as i32,
                text: entry.location.clone(),
                text_x: entry.text_x,
                text_width: entry.text_width,
                y: entry.location_y,
                line_height: entry.metadata_line_height,
                font_size: entry.metadata_font_size,
                is_group: false,
            });
        }
    }

    if metadata.is_empty() {
        ModelRc::default()
    } else {
        ModelRc::new(Rc::new(VecModel::from(metadata)))
    }
}

fn item_view_row_tokens(
    entries: &[ItemViewEntry],
    selected_paths: &[String],
) -> Vec<ItemViewRowToken> {
    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    entries
        .iter()
        .map(|entry| {
            let mut token = ItemViewRowToken::from_entry(entry);
            token.set_selected(selected.contains(entry.path.as_str()));
            token
        })
        .collect()
}

pub(crate) fn update_item_view_entries_model(
    current: &ModelRc<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    current_tokens: &mut Vec<ItemViewRowToken>,
    entries: Vec<ItemViewEntry>,
    selected_paths: &[String],
) -> Option<ModelRc<ItemViewEntry>> {
    let mut next_tokens = item_view_row_tokens(&entries, selected_paths);
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
    show_location: bool,
    selected_paths: &[String],
) {
    view.virtual_metadata_entries = new_item_view_metadata_model(&entries, show_location);
    let current = view.virtual_entries.clone();
    let old_start = view.virtual_start_index;
    if let Some(model) = update_item_view_entries_model(
        &current,
        old_start,
        start_index,
        &mut view.virtual_entry_tokens,
        entries,
        selected_paths,
    ) {
        view.virtual_entries = model;
    }
    view.virtual_start_index = start_index;
    view.virtual_start_column = start_column;
}

pub(crate) fn update_item_view_selection_tokens(
    current_tokens: &mut [ItemViewRowToken],
    selected_paths: &[String],
) -> bool {
    if selected_paths.is_empty() {
        let mut changed = false;
        for token in current_tokens {
            if token.selected() {
                token.set_selected(false);
                changed = true;
            }
        }
        return changed;
    }

    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let mut changed = false;
    for token in current_tokens.iter_mut() {
        let selected = selected.contains(token.path());
        if token.selected() != selected {
            token.set_selected(selected);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
fn selected_token_rows(current_tokens: &[ItemViewRowToken]) -> Vec<String> {
    current_tokens
        .iter()
        .filter(|token| token.selected())
        .map(|token| token.path().to_string())
        .collect()
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
        let rows_differ = current_tokens
            .get(row)
            .zip(next_tokens.get(row))
            .is_none_or(|(current, next)| !current.row_equals_ignoring_selection(next));
        if rows_differ {
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

        update_pane_item_view_entries_model(
            &mut left,
            0,
            0,
            (0..3).map(entry).collect(),
            false,
            &[],
        );
        update_pane_item_view_entries_model(
            &mut right,
            20,
            4,
            (20..23).map(entry).collect(),
            false,
            &[],
        );

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

        update_pane_item_view_entries_model(
            &mut right,
            22,
            5,
            (22..25).map(entry).collect(),
            false,
            &[],
        );

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
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                2,
                &mut tokens,
                (2..8).map(entry).collect(),
                &[]
            )
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
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                4,
                2,
                &mut tokens,
                (2..8).map(entry).collect(),
                &[]
            )
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
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                20,
                &mut tokens,
                (20..23).map(entry).collect(),
                &[]
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
            update_item_view_entries_model(
                &model,
                0,
                2,
                &mut tokens,
                (2..8).map(entry).collect(),
                &[]
            )
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
    fn item_view_entry_model_repairs_empty_title_geometry_for_same_row() {
        let mut empty_title = entry(0);
        empty_title.text_width = 0.0;
        empty_title.title_y = 0.0;
        empty_title.title_line_height = 0.0;
        empty_title.title_font_size = 0.0;
        let mut tokens = item_view_row_tokens(&[empty_title.clone()], &[]);
        let model = new_item_view_entries_model(vec![empty_title]);
        let original = model.clone();

        let mut rendered_title = entry(0);
        rendered_title.text_x = 52.0;
        rendered_title.text_width = 75.0;
        rendered_title.title_y = 14.5;
        rendered_title.title_line_height = 21.0;
        rendered_title.title_font_size = 15.0;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![rendered_title], &[])
                .is_none()
        );

        assert_eq!(model, original);
        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.name, "item-0");
        assert_eq!(updated.text_width, 75.0);
        assert_eq!(updated.title_y, 14.5);
        assert_eq!(updated.title_line_height, 21.0);
        assert_eq!(updated.title_font_size, 15.0);
    }

    #[test]
    fn item_view_entry_model_uses_media_token_instead_of_image_comparison_for_overlap() {
        let mut old_entry = entry(0);
        old_entry.media = colored_image(Rgba8Pixel::new(255, 0, 0, 255));
        old_entry.media_token = 42;
        let initial_entries = vec![old_entry];
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        let mut same_token_entry = entry(0);
        same_token_entry.media = colored_image(Rgba8Pixel::new(0, 0, 255, 255));
        same_token_entry.media_token = 42;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![same_token_entry], &[])
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
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![new_token_entry], &[])
                .is_none()
        );

        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.media_token, 43);
        assert_eq!(first_pixel(&updated.media), Rgba8Pixel::new(0, 0, 255, 255));
    }

    #[test]
    fn pane_item_view_entry_model_updates_selection_sidecar_without_replacing_entries() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            0,
            (0..4).map(entry).collect(),
            false,
            &[],
        );
        let original = view.virtual_entries.clone();

        assert!(update_item_view_selection_tokens(
            &mut view.virtual_entry_tokens,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_entries, original);
        assert_eq!(
            selected_token_rows(&view.virtual_entry_tokens),
            vec!["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        );

        assert!(!update_item_view_selection_tokens(
            &mut view.virtual_entry_tokens,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));

        assert!(update_item_view_selection_tokens(
            &mut view.virtual_entry_tokens,
            &[]
        ));
        assert!(selected_token_rows(&view.virtual_entry_tokens).is_empty());
    }

    #[test]
    fn virtual_row_reuse_ignores_selection_sidecar_changes() {
        let initial_entries = (0..3).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries, &["/tmp/item-1".to_string()]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                0,
                &mut tokens,
                (0..3).map(entry).collect(),
                &["/tmp/item-2".to_string()]
            )
            .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(
            selected_token_rows(&tokens),
            vec!["/tmp/item-2".to_string()]
        );
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
            overlap_body.contains("!current.row_equals_ignoring_selection(next)")
                && overlap_body.contains("model.set_row_data(row, entries[row].clone());")
                && !overlap_body.contains(".row_data("),
            "overlap row reuse should compare the Rust sidecar token instead of cloning current ItemViewEntry rows from Slint"
        );
    }

    #[test]
    fn metadata_model_contains_only_renderable_location_rows() {
        let mut plain = entry(0);
        plain.group = "ignored".into();
        plain.location = "/tmp/ignored".into();
        plain.text_x = 52.0;
        plain.text_width = 75.0;
        plain.group_y = 2.0;
        plain.location_y = 41.0;
        plain.metadata_line_height = 14.0;
        plain.metadata_font_size = 11.0;

        let hidden = new_item_view_metadata_model(&[plain.clone()], false);
        assert_eq!(hidden.row_count(), 0);

        let mut location = entry(1);
        location.group = "Documents".into();
        location.location = "/home/user/Documents".into();
        location.text_x = 52.0;
        location.text_width = 75.0;
        location.group_y = 2.0;
        location.location_y = 41.0;
        location.metadata_line_height = 14.0;
        location.metadata_font_size = 11.0;

        let model = new_item_view_metadata_model(&[plain, entry(2), location], true);

        assert_eq!(model.row_count(), 4);
        let rows = (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| {
                (
                    entry.slice_index,
                    entry.text.to_string(),
                    entry.y,
                    entry.line_height,
                    entry.font_size,
                    entry.is_group,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            vec![
                (0, "ignored".to_string(), 2.0, 14.0, 11.0, true),
                (0, "/tmp/ignored".to_string(), 41.0, 14.0, 11.0, false),
                (2, "Documents".to_string(), 2.0, 14.0, 11.0, true),
                (
                    2,
                    "/home/user/Documents".to_string(),
                    41.0,
                    14.0,
                    11.0,
                    false,
                ),
            ]
        );
    }
}
