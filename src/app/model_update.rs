use crate::app::pane::PaneView;
use crate::{
    ItemViewBoundsEntry, ItemViewEntry, ItemViewHighlightEntry, ItemViewMediaEntry,
    ItemViewMetadataEntry,
};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewRowToken {
    name: SharedString,
    path: SharedString,
    is_dir: bool,
    selected: bool,
    thumbnail_state: i32,
    media_token: i32,
}

impl ItemViewRowToken {
    pub(crate) fn from_entry(entry: &ItemViewEntry) -> Self {
        Self {
            name: entry.name.clone(),
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            selected: false,
            thumbnail_state: entry.thumbnail_state,
            media_token: entry.media_token,
        }
    }

    pub(crate) fn path(&self) -> &str {
        self.path.as_str()
    }

    pub(crate) fn path_shared(&self) -> SharedString {
        self.path.clone()
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub(crate) fn thumbnail_state(&self) -> i32 {
        self.thumbnail_state
    }

    pub(crate) fn media_token(&self) -> i32 {
        self.media_token
    }

    pub(crate) fn selected(&self) -> bool {
        self.selected
    }

    pub(crate) fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub(crate) fn row_equals_ignoring_selection(&self, other: &Self) -> bool {
        let mut current = self.clone();
        let mut next = other.clone();
        current.selected = false;
        next.selected = false;
        current == next
    }

    pub(crate) fn has_renderable_title(&self) -> bool {
        !self.name.as_str().trim().is_empty()
    }
}

pub(crate) fn new_item_view_entries_model(entries: Vec<ItemViewEntry>) -> ModelRc<ItemViewEntry> {
    ModelRc::new(Rc::new(VecModel::from(entries)))
}

pub(crate) fn new_item_view_bounds_model(
    bounds_entries: Vec<ItemViewBoundsEntry>,
) -> ModelRc<ItemViewBoundsEntry> {
    if bounds_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(bounds_entries)))
}

pub(crate) fn new_item_view_metadata_model(
    metadata: Vec<ItemViewMetadataEntry>,
) -> ModelRc<ItemViewMetadataEntry> {
    if metadata.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(metadata)))
}

pub(crate) fn new_item_view_media_model(
    media_entries: Vec<ItemViewMediaEntry>,
) -> ModelRc<ItemViewMediaEntry> {
    if media_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(media_entries)))
}

fn item_view_highlight_entries(tokens: &[ItemViewRowToken]) -> Vec<ItemViewHighlightEntry> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(row, token)| {
            token.selected().then_some(ItemViewHighlightEntry {
                slice_index: row as i32,
            })
        })
        .collect()
}

fn update_item_view_highlight_entries_model(
    current: &mut ModelRc<ItemViewHighlightEntry>,
    highlights: Vec<ItemViewHighlightEntry>,
) -> bool {
    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewHighlightEntry>>()
    else {
        if highlights.is_empty() {
            if current.row_count() == 0 {
                return false;
            }
            *current = ModelRc::default();
        } else {
            *current = ModelRc::new(Rc::new(VecModel::from(highlights)));
        }
        return true;
    };

    let unchanged = model.row_count() == highlights.len()
        && highlights
            .iter()
            .enumerate()
            .all(|(row, next)| model.row_data(row).as_ref() == Some(next));
    if unchanged {
        return false;
    }

    model.set_vec(highlights);
    true
}

pub(crate) fn update_item_view_highlight_model(view: &mut PaneView) -> bool {
    update_item_view_highlight_entries_model(
        &mut view.virtual_highlight_entries,
        item_view_highlight_entries(&view.virtual_entry_tokens),
    )
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
    entries: Vec<ItemViewEntry>,
    bounds_entries: Vec<ItemViewBoundsEntry>,
    media_entries: Vec<ItemViewMediaEntry>,
    metadata_entries: Vec<ItemViewMetadataEntry>,
    selected_paths: &[String],
) {
    view.virtual_bounds_entries = new_item_view_bounds_model(bounds_entries);
    view.virtual_media_entries = new_item_view_media_model(media_entries);
    view.virtual_metadata_entries = new_item_view_metadata_model(metadata_entries);
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
    update_item_view_highlight_model(view);
    view.virtual_start_index = start_index;
}

pub(crate) fn relayout_pane_item_view_entries_model(
    view: &mut PaneView,
    range: Range<usize>,
    bounds_entries: Vec<ItemViewBoundsEntry>,
) -> bool {
    let Some(model) = view
        .virtual_entries
        .as_any()
        .downcast_ref::<VecModel<ItemViewEntry>>()
    else {
        return false;
    };
    let row_count = model.row_count();
    if row_count != view.virtual_entry_tokens.len() {
        return false;
    }
    let old_start = view.virtual_start_index;
    let old_end = old_start.saturating_add(row_count);
    if range.is_empty() || range.start < old_start || range.end > old_end || range.end < range.start
    {
        return false;
    }

    let remove_front = range.start - old_start;
    for _ in 0..remove_front {
        model.remove(0);
    }
    view.virtual_entry_tokens
        .drain(0..remove_front.min(view.virtual_entry_tokens.len()));

    let target_len = range.end - range.start;
    while model.row_count() > target_len {
        model.remove(model.row_count() - 1);
        view.virtual_entry_tokens.pop();
    }
    if model.row_count() != target_len || view.virtual_entry_tokens.len() != target_len {
        return false;
    }

    let _ = update_item_view_highlight_model(view);
    view.virtual_bounds_entries = new_item_view_bounds_model(bounds_entries);
    trim_item_view_media_entries_model(&mut view.virtual_media_entries, remove_front, target_len);
    view.virtual_start_index = range.start;
    true
}

fn trim_item_view_media_entries_model(
    current: &mut ModelRc<ItemViewMediaEntry>,
    remove_front: usize,
    target_len: usize,
) {
    let entries = (0..current.row_count())
        .filter_map(|row| current.row_data(row))
        .filter_map(|mut entry| {
            let slice_index = usize::try_from(entry.slice_index).ok()?;
            if slice_index < remove_front {
                return None;
            }
            let shifted = slice_index - remove_front;
            if shifted >= target_len {
                return None;
            }
            entry.slice_index = shifted as i32;
            Some(entry)
        })
        .collect::<Vec<_>>();

    *current = new_item_view_media_model(entries);
}

pub(crate) fn update_pane_item_view_selection_model(
    view: &mut PaneView,
    selected_paths: &[String],
) -> bool {
    let changed = update_item_view_selection_tokens(&mut view.virtual_entry_tokens, selected_paths);
    if changed {
        update_item_view_highlight_model(view);
    }
    changed
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
            is_dir: false,
            thumbnail_state: 0,
            media_token: 0,
        }
    }

    fn rows(model: &ModelRc<ItemViewEntry>) -> Vec<String> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| entry.path.to_string())
            .collect()
    }

    fn highlight_rows(model: &ModelRc<ItemViewHighlightEntry>) -> Vec<i32> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| entry.slice_index)
            .collect()
    }

    fn media_rows(model: &ModelRc<ItemViewMediaEntry>) -> Vec<(i32, Rgba8Pixel)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.slice_index, first_pixel(&entry.media)))
            .collect()
    }

    fn entries_with_tile_metrics(count: usize) -> Vec<ItemViewEntry> {
        (0..count).map(entry).collect()
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
            (0..3).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );
        update_pane_item_view_entries_model(
            &mut right,
            20,
            (20..23).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(left.virtual_start_index, 0);
        assert_eq!(right.virtual_start_index, 20);
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
            (22..25).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(
            rows(&left.virtual_entries),
            (0..3)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(right.virtual_start_index, 22);
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
    fn item_view_entry_model_ignores_pane_level_layout_for_same_row() {
        let initial = entry(0);
        let mut tokens = item_view_row_tokens(&[initial.clone()], &[]);
        let model = new_item_view_entries_model(vec![initial]);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![entry(0)], &[])
                .is_none()
        );

        assert_eq!(model, original);
        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.name, "item-0");
    }

    #[test]
    fn item_view_entry_model_uses_media_token_without_row_images() {
        let mut old_entry = entry(0);
        old_entry.media_token = 42;
        let initial_entries = vec![old_entry];
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        let mut same_token_entry = entry(0);
        same_token_entry.media_token = 42;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![same_token_entry], &[])
                .is_none()
        );

        assert_eq!(model, original);
        let unchanged = model.row_data(0).expect("row should remain present");
        assert_eq!(unchanged.media_token, 42);

        let mut new_token_entry = entry(0);
        new_token_entry.media_token = 43;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![new_token_entry], &[])
                .is_none()
        );

        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.media_token, 43);
    }

    #[test]
    fn pane_item_view_entry_model_updates_selection_sidecar_without_replacing_entries() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(4),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );
        let original = view.virtual_entries.clone();

        assert!(update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_entries, original);
        assert_eq!(
            selected_token_rows(&view.virtual_entry_tokens),
            vec!["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        );
        assert_eq!(highlight_rows(&view.virtual_highlight_entries), vec![1, 3]);

        let selected_highlights = view.virtual_highlight_entries.clone();
        assert!(!update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_highlight_entries, selected_highlights);

        assert!(update_pane_item_view_selection_model(&mut view, &[]));
        assert!(selected_token_rows(&view.virtual_entry_tokens).is_empty());
        assert_eq!(view.virtual_highlight_entries, selected_highlights);
        assert!(highlight_rows(&view.virtual_highlight_entries).is_empty());
    }

    #[test]
    fn pane_item_view_highlight_model_reuses_vec_model_when_selection_shape_changes() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(4),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert!(update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        let original_highlights = view.virtual_highlight_entries.clone();

        assert!(update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-2".to_string()]
        ));

        assert_eq!(view.virtual_highlight_entries, original_highlights);
        assert_eq!(highlight_rows(&view.virtual_highlight_entries), vec![2]);
    }

    #[test]
    fn pane_item_view_cached_relayout_reuses_vec_model_and_selection() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            10,
            entries_with_tile_metrics(4),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &["/tmp/item-2".to_string()],
        );
        let original = view.virtual_entries.clone();

        assert!(relayout_pane_item_view_entries_model(
            &mut view,
            11..13,
            Vec::new()
        ));

        assert_eq!(view.virtual_entries, original);
        assert_eq!(view.virtual_start_index, 11);
        assert_eq!(
            rows(&view.virtual_entries),
            vec!["/tmp/item-1".to_string(), "/tmp/item-2".to_string()]
        );
        assert_eq!(
            selected_token_rows(&view.virtual_entry_tokens),
            vec!["/tmp/item-2".to_string()]
        );
        assert_eq!(highlight_rows(&view.virtual_highlight_entries), vec![1]);
    }

    #[test]
    fn pane_item_view_cached_relayout_trims_sparse_media_sidecar() {
        let mut entries = entries_with_tile_metrics(4);
        entries[1].thumbnail_state = 2;
        entries[1].media_token = 101;
        entries[3].thumbnail_state = 2;
        entries[3].media_token = 103;
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            10,
            entries,
            Vec::new(),
            vec![
                ItemViewMediaEntry {
                    slice_index: 1,
                    media: colored_image(Rgba8Pixel::new(255, 0, 0, 255)),
                },
                ItemViewMediaEntry {
                    slice_index: 3,
                    media: colored_image(Rgba8Pixel::new(0, 0, 255, 255)),
                },
            ],
            Vec::new(),
            &[],
        );

        assert_eq!(
            media_rows(&view.virtual_media_entries),
            vec![
                (1, Rgba8Pixel::new(255, 0, 0, 255)),
                (3, Rgba8Pixel::new(0, 0, 255, 255)),
            ]
        );

        assert!(relayout_pane_item_view_entries_model(
            &mut view,
            11..13,
            Vec::new()
        ));

        assert_eq!(
            media_rows(&view.virtual_media_entries),
            vec![(0, Rgba8Pixel::new(255, 0, 0, 255))]
        );
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
    fn metadata_model_uses_preprojected_sparse_rows() {
        let hidden = new_item_view_metadata_model(Vec::new());
        assert_eq!(hidden.row_count(), 0);

        let model = new_item_view_metadata_model(vec![
            ItemViewMetadataEntry {
                slice_index: 0,
                text: "Documents".into(),
                text_x: 52.0,
                text_width: 75.0,
                y: 2.0,
                line_height: 14.0,
                font_size: 11.0,
                is_group: true,
            },
            ItemViewMetadataEntry {
                slice_index: 0,
                text: "/home/user/Documents".into(),
                text_x: 52.0,
                text_width: 75.0,
                y: 41.0,
                line_height: 14.0,
                font_size: 11.0,
                is_group: false,
            },
        ]);

        assert_eq!(model.row_count(), 2);
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
                (0, "Documents".to_string(), 2.0, 14.0, 11.0, true),
                (
                    0,
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
