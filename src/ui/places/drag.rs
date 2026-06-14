use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Context, IntoElement, ParentElement, Render, Styled, div, px, rgb};

use super::super::drag_drop::{DragExportPayload, place_drag_export_payload};
use crate::ui::icons::{FileIconSnapshot, cached_icon_or_fallback};

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

pub(crate) struct PlaceDragPreview {
    label: Arc<str>,
    path: PathBuf,
    icon: FileIconSnapshot,
}

impl PlaceDragPreview {
    pub(crate) fn from_drag(drag: &PlaceDrag) -> Self {
        Self {
            label: drag.label.clone(),
            path: drag.path.clone(),
            icon: drag.icon.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceDropZone {
    InsertBefore,
    OnPlace,
    InsertAfter,
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
    match zone {
        PlaceDropZone::InsertBefore => Some(target_index),
        PlaceDropZone::InsertAfter => Some(target_index + 1),
        PlaceDropZone::OnPlace if source_index < target_index => Some(target_index + 1),
        PlaceDropZone::OnPlace if source_index > target_index => Some(target_index),
        PlaceDropZone::OnPlace => None,
    }
}

fn place_drop_zone_for_y(local_y: f32, height: f32) -> PlaceDropZone {
    let edge = (height * 0.28).clamp(4.0, 10.0);
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
        let icon = self.icon.clone();
        div()
            .px_2()
            .h(px(36.0))
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
            .child(div().max_w(px(180.0)).truncate().child(format!(
                "{} -> {}",
                self.label,
                display_path_for_drag(&self.path)
            )))
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

fn display_path_for_drag(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn place_drop_zone_uses_edges_for_insert_targets() {
        assert_eq!(
            place_drop_zone_for_y(0.0, 28.0),
            PlaceDropZone::InsertBefore
        );
        assert_eq!(
            place_drop_zone_for_y(7.0, 28.0),
            PlaceDropZone::InsertBefore
        );
        assert_eq!(place_drop_zone_for_y(8.0, 28.0), PlaceDropZone::OnPlace);
        assert_eq!(
            place_drop_zone_for_y(21.0, 28.0),
            PlaceDropZone::InsertAfter
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
    fn display_path_for_drag_prefers_filename() {
        assert_eq!(
            display_path_for_drag(Path::new("/home/yk/Work")),
            "Work".to_string()
        );
        assert_eq!(display_path_for_drag(Path::new("/")), "/".to_string());
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

    fn test_icon_snapshot() -> FileIconSnapshot {
        FileIconSnapshot {
            icon_name: Arc::from("test-place"),
            path: None,
            fallback_marker: Arc::from("P"),
            fallback_fg: 0x1f4fbf,
            fallback_bg: 0xeaf1ff,
        }
    }
}
