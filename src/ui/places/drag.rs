use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Context, Div, IntoElement, ParentElement, Render, Stateful, Styled, div, px, rgb};

use super::super::drag_drop::{
    DragExportPayload, DragPreviewLayout, PlaceDropTarget, drag_preview_layout_for_cursor_offset,
    place_drag_export_payload,
};
use crate::FikaApp;
use crate::ui::icons::{FileIconSnapshot, cached_icon_or_fallback};

use super::snapshot::PlaceSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceDrag {
    path: PathBuf,
    label: Arc<str>,
    icon: FileIconSnapshot,
    source_index: usize,
    movable: bool,
    pub(crate) export: Option<DragExportPayload>,
}

impl PlaceDrag {
    pub(crate) fn new(
        path: PathBuf,
        label: &str,
        icon: FileIconSnapshot,
        source_index: usize,
        movable: bool,
    ) -> Self {
        let export = place_drag_export_payload(&path);
        Self {
            path,
            label: Arc::from(label),
            icon,
            source_index,
            movable,
            export,
        }
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub(crate) fn source_index(&self) -> usize {
        self.source_index
    }

    pub(crate) fn movable(&self) -> bool {
        self.movable
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceDragStartSource {
    drag: PlaceDrag,
}

impl PlaceDragStartSource {
    pub(crate) fn from_snapshot(place: &PlaceSnapshot) -> Self {
        Self {
            drag: PlaceDrag::new(
                place.path.clone(),
                place.label.as_str(),
                place.icon.clone(),
                place.index,
                place_drag_is_movable(place),
            ),
        }
    }

    pub(crate) fn into_drag(self) -> PlaceDrag {
        self.drag
    }
}

pub(crate) fn install_place_drag_start_shell(
    row: Stateful<Div>,
    source: PlaceDragStartSource,
) -> Stateful<Div> {
    row.on_drag(source.into_drag(), |drag, cursor_offset, _, cx| {
        cx.new(|_| PlaceDragPreview::from_drag(drag, cursor_offset))
    })
}

pub(crate) fn place_drag_is_movable(place: &PlaceSnapshot) -> bool {
    place.group.is_empty()
}

impl FikaApp {
    pub(crate) fn set_place_drag_drop_target_for_path(&mut self, path: PathBuf) -> bool {
        self.drop_targets.set_place(PlaceDropTarget::Place { path })
    }

    pub(crate) fn set_place_drag_drop_target_for_insert(&mut self, index: usize) -> bool {
        let index = self.user_place_insert_index(index);
        self.drop_targets
            .set_place(PlaceDropTarget::Insert { index })
    }

    pub(crate) fn current_place_drop_target_is_insert(&self) -> bool {
        matches!(
            self.drop_targets.place(),
            Some(PlaceDropTarget::Insert { .. })
        )
    }

    pub(crate) fn current_place_drop_target_matches_path(&self, path: &Path) -> bool {
        matches!(
            self.drop_targets.place(),
            Some(PlaceDropTarget::Place { path: target_path }) if target_path == path
        )
    }

    pub(crate) fn clear_place_drop_target(&mut self) -> bool {
        self.drop_targets.clear_place()
    }

    pub(crate) fn clear_place_drop_target_if_window_position_is_in_pane_viewport(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<bool> {
        if !self.window_position_is_in_pane_viewport(position) {
            return None;
        }
        Some(self.clear_place_drop_target())
    }
}

pub(crate) struct PlaceDragPreview {
    label: Arc<str>,
    icon: FileIconSnapshot,
    layout: DragPreviewLayout,
}

impl PlaceDragPreview {
    pub(crate) fn from_drag(drag: &PlaceDrag, cursor_offset: gpui::Point<gpui::Pixels>) -> Self {
        Self {
            label: drag.label.clone(),
            icon: drag.icon.clone(),
            layout: drag_preview_layout_for_cursor_offset(
                cursor_offset,
                PLACE_DRAG_PREVIEW_MIN_WIDTH,
                PLACE_DRAG_PREVIEW_MIN_HEIGHT + 6.0,
            ),
        }
    }
}

const PLACE_DRAG_PREVIEW_MIN_WIDTH: f32 = 220.0;
const PLACE_DRAG_PREVIEW_MIN_HEIGHT: f32 = 36.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceDropZone {
    InsertBefore,
    OnPlace,
    InsertAfter,
}

impl PlaceDropZone {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::InsertBefore => "InsertBefore",
            Self::OnPlace => "OnPlace",
            Self::InsertAfter => "InsertAfter",
        }
    }
}

pub(crate) fn place_drop_zone<T>(event: &gpui::DragMoveEvent<T>) -> PlaceDropZone {
    let local_y = (event.event.position.y - event.bounds.origin.y).as_f32();
    place_drop_zone_for_y(local_y, event.bounds.size.height.as_f32())
}

pub(crate) fn place_drag_insert_index_for_zone(
    source_index: usize,
    target_index: usize,
    zone: PlaceDropZone,
) -> Option<usize> {
    let insert_index = match zone {
        PlaceDropZone::InsertBefore => target_index,
        PlaceDropZone::InsertAfter => target_index.saturating_add(1),
        PlaceDropZone::OnPlace if source_index < target_index => target_index.saturating_add(1),
        PlaceDropZone::OnPlace if source_index > target_index => target_index,
        PlaceDropZone::OnPlace => return None,
    };
    place_drag_insert_index(source_index, insert_index)
}

pub(crate) fn place_drag_insert_index(source_index: usize, insert_index: usize) -> Option<usize> {
    if insert_index == source_index || insert_index == source_index.saturating_add(1) {
        None
    } else {
        Some(insert_index)
    }
}

pub(crate) fn place_drop_zone_for_y(local_y: f32, height: f32) -> PlaceDropZone {
    let edge = (height * 0.18).clamp(4.0, 6.0);
    if local_y <= edge {
        PlaceDropZone::InsertBefore
    } else if local_y >= height - edge {
        PlaceDropZone::InsertAfter
    } else {
        PlaceDropZone::OnPlace
    }
}

impl Render for PlaceDragPreview {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let left = self.layout.content_origin_x;
        let top = self.layout.content_origin_y;
        let icon = self.icon.clone();
        div()
            .relative()
            .w(px(self.layout.surface_width))
            .h(px(self.layout.surface_height))
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .px_2()
                    .h(px(PLACE_DRAG_PREVIEW_MIN_HEIGHT))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x94a3b8))
                    .bg(rgb(0xffffff))
                    .shadow_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(0x1f2937))
                    .child(
                        div()
                            .w(px(26.0))
                            .h(px(26.0))
                            .rounded_sm()
                            .overflow_hidden()
                            .child(place_drag_icon_or_fallback(icon)),
                    )
                    .child(
                        div()
                            .max_w(px(170.0))
                            .truncate()
                            .child(self.label.as_ref().to_string()),
                    ),
            )
    }
}

fn place_drag_icon_or_fallback(icon: FileIconSnapshot) -> gpui::AnyElement {
    let marker = icon.fallback_marker.clone();
    let fg = icon.fallback_fg;
    let bg = icon.fallback_bg;
    cached_icon_or_fallback(&icon, move || {
        div()
            .size_full()
            .rounded_sm()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(fg))
            .bg(rgb(bg))
            .child(marker.as_ref().to_string())
            .into_any_element()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::drag_drop::drag_preview_content_origin_for_cursor_offset;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn place_drop_zone_uses_edges_for_insert_targets() {
        assert_eq!(
            place_drop_zone_for_y(0.0, 28.0),
            PlaceDropZone::InsertBefore
        );
        assert_eq!(
            place_drop_zone_for_y(5.0, 28.0),
            PlaceDropZone::InsertBefore
        );
        assert_eq!(place_drop_zone_for_y(6.0, 28.0), PlaceDropZone::OnPlace);
        assert_eq!(
            place_drop_zone_for_y(23.0, 28.0),
            PlaceDropZone::InsertAfter
        );
    }

    #[test]
    fn place_drag_preview_compensates_for_row_cursor_offset() {
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(gpui::point(
                gpui::px(48.0),
                gpui::px(12.0)
            )),
            (56.0, 20.0)
        );
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(gpui::point(
                gpui::px(-12.0),
                gpui::px(-4.0)
            )),
            (-4.0, 4.0)
        );
    }

    #[test]
    fn place_drag_insert_index_tracks_reorder_direction() {
        assert_eq!(
            place_drag_insert_index_for_zone(3, 1, PlaceDropZone::InsertBefore),
            Some(1)
        );
        assert_eq!(
            place_drag_insert_index_for_zone(1, 3, PlaceDropZone::InsertAfter),
            Some(4)
        );
        assert_eq!(
            place_drag_insert_index_for_zone(1, 3, PlaceDropZone::OnPlace),
            Some(4)
        );
        assert_eq!(
            place_drag_insert_index_for_zone(3, 1, PlaceDropZone::OnPlace),
            Some(1)
        );
        assert_eq!(
            place_drag_insert_index_for_zone(2, 2, PlaceDropZone::OnPlace),
            None
        );
    }

    #[test]
    fn place_drag_insert_index_rejects_noop_insert_positions() {
        assert_eq!(
            place_drag_insert_index_for_zone(0, 0, PlaceDropZone::InsertBefore),
            None
        );
        assert_eq!(
            place_drag_insert_index_for_zone(0, 0, PlaceDropZone::InsertAfter),
            None
        );
        assert_eq!(
            place_drag_insert_index_for_zone(1, 0, PlaceDropZone::InsertAfter),
            None
        );
        assert_eq!(place_drag_insert_index(2, 2), None);
        assert_eq!(place_drag_insert_index(2, 3), None);
        assert_eq!(place_drag_insert_index(2, 4), Some(4));
    }

    #[test]
    fn place_drag_carries_external_export_for_directories_only() {
        let root = std::env::temp_dir().join(format!(
            "fika-place-drag-payload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dir = root.join("dir");
        let file = root.join("file.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "not exported").unwrap();

        let dir_drag = PlaceDrag::new(dir.clone(), "dir", test_icon_snapshot(), 0, true);
        assert_eq!(
            dir_drag
                .export
                .as_ref()
                .map(|payload| payload.paths.clone()),
            Some(vec![dir])
        );
        let file_drag = PlaceDrag::new(file, "file", test_icon_snapshot(), 1, true);
        assert_eq!(file_drag.export, None);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn place_drag_start_source_projects_snapshot_identity() {
        let place = test_place_snapshot("", "Home", "/home/yk", 4);
        let drag = PlaceDragStartSource::from_snapshot(&place).into_drag();

        assert_eq!(drag.path(), PathBuf::from("/home/yk"));
        assert_eq!(drag.source_index(), 4);
        assert!(drag.movable());
    }

    #[test]
    fn place_drag_start_source_marks_grouped_places_immovable() {
        let place = test_place_snapshot("Devices", "Root", "/", 7);
        let drag = PlaceDragStartSource::from_snapshot(&place).into_drag();

        assert_eq!(drag.path(), PathBuf::from("/"));
        assert_eq!(drag.source_index(), 7);
        assert!(!drag.movable());
    }

    fn test_icon_snapshot() -> FileIconSnapshot {
        FileIconSnapshot {
            icon_name: Arc::from("test-place"),
            path: None,
            fallback_marker: Arc::from("P"),
            fallback_fg: 0x1f4fbf,
            fallback_bg: 0xeaf1ff,
        }
    }

    fn test_place_snapshot(
        group: &'static str,
        label: &str,
        path: &str,
        index: usize,
    ) -> PlaceSnapshot {
        PlaceSnapshot {
            index,
            group,
            icon: test_icon_snapshot(),
            label: label.to_string(),
            path: PathBuf::from(path),
            device_id: None,
            mounted: true,
            device: false,
            network: false,
            device_ejectable: false,
            device_can_power_off: false,
            active: false,
            drop_target: false,
            insert_before: false,
            insert_after: false,
            trash_place: false,
            trash_has_items: false,
            editable: true,
            removable: true,
        }
    }
}
