use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Context, IntoElement, ParentElement, Render, Styled, div, rgb};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceDrag {
    path: PathBuf,
    label: Arc<str>,
    source_index: usize,
    movable: bool,
}

impl PlaceDrag {
    pub(crate) fn new(path: PathBuf, label: &str, source_index: usize, movable: bool) -> Self {
        Self {
            path,
            label: Arc::from(label),
            source_index,
            movable,
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
}

impl PlaceDragPreview {
    pub(crate) fn from_drag(drag: &PlaceDrag) -> Self {
        Self {
            label: drag.label.clone(),
            path: drag.path.clone(),
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
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x94a3b8))
            .bg(rgb(0xffffff))
            .text_sm()
            .text_color(rgb(0x1f2937))
            .child(format!(
                "{} -> {}",
                self.label,
                display_path_for_drag(&self.path)
            ))
    }
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
}
