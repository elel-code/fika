use std::path::{Path, PathBuf};

use super::drag::{
    PlaceDropZone, place_drag_insert_index, place_drag_insert_index_for_zone, place_drop_zone_for_y,
};
use super::snapshot::PlaceSnapshot;
use super::visual::{PLACE_ROW_HEIGHT, PLACE_SECTION_HEADING_HEIGHT};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceInteractionCursor {
    Copy,
    Move,
    DropMenu,
    NotAllowed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlaceInteractionTarget {
    Clear,
    Insert { index: usize },
    Place { path: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceInteractionDecision {
    pub(crate) target: PlaceInteractionTarget,
    pub(crate) cursor: PlaceInteractionCursor,
}

impl PlaceInteractionDecision {
    fn clear() -> Self {
        Self {
            target: PlaceInteractionTarget::Clear,
            cursor: PlaceInteractionCursor::NotAllowed,
        }
    }

    fn insert(index: usize, cursor: PlaceInteractionCursor) -> Self {
        Self {
            target: PlaceInteractionTarget::Insert { index },
            cursor,
        }
    }

    fn place(path: &Path) -> Self {
        Self {
            target: PlaceInteractionTarget::Place {
                path: path.to_path_buf(),
            },
            cursor: PlaceInteractionCursor::DropMenu,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlaceRowTargetInput<'a> {
    pub(crate) drop_zone: PlaceDropZone,
    pub(crate) mounted: bool,
    pub(crate) can_add_place: bool,
    pub(crate) accepts_place: bool,
    pub(crate) insert_before_index: usize,
    pub(crate) insert_after_index: usize,
    pub(crate) target_path: &'a Path,
}

pub(crate) fn place_row_path_list_target(
    input: PlaceRowTargetInput<'_>,
) -> PlaceInteractionDecision {
    match input.drop_zone {
        PlaceDropZone::InsertBefore if input.can_add_place => PlaceInteractionDecision::insert(
            input.insert_before_index,
            PlaceInteractionCursor::Copy,
        ),
        PlaceDropZone::InsertAfter if input.can_add_place => {
            PlaceInteractionDecision::insert(input.insert_after_index, PlaceInteractionCursor::Copy)
        }
        PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
            PlaceInteractionDecision::clear()
        }
        PlaceDropZone::OnPlace if input.mounted && input.accepts_place => {
            PlaceInteractionDecision::place(input.target_path)
        }
        PlaceDropZone::OnPlace => PlaceInteractionDecision::clear(),
    }
}

pub(crate) fn place_row_place_drag_target(
    movable: bool,
    source_index: usize,
    drop_zone: PlaceDropZone,
    insert_before_index: usize,
    insert_after_index: usize,
) -> PlaceInteractionDecision {
    if !movable {
        return PlaceInteractionDecision::clear();
    }
    let insert_index = match drop_zone {
        PlaceDropZone::InsertBefore => place_drag_insert_index(source_index, insert_before_index),
        PlaceDropZone::InsertAfter => place_drag_insert_index(source_index, insert_after_index),
        PlaceDropZone::OnPlace => {
            place_drag_insert_index_for_zone(source_index, insert_before_index, drop_zone)
        }
    };
    match insert_index {
        Some(index) => PlaceInteractionDecision::insert(index, PlaceInteractionCursor::Move),
        None => PlaceInteractionDecision::clear(),
    }
}

pub(crate) fn place_section_path_list_target(
    can_add_place: bool,
    insert_index: usize,
) -> PlaceInteractionDecision {
    if can_add_place {
        PlaceInteractionDecision::insert(insert_index, PlaceInteractionCursor::Copy)
    } else {
        PlaceInteractionDecision::clear()
    }
}

pub(crate) fn place_section_place_drag_target(
    movable: bool,
    source_index: usize,
    insert_index: usize,
) -> PlaceInteractionDecision {
    if !movable {
        return PlaceInteractionDecision::clear();
    }
    match place_drag_insert_index(source_index, insert_index) {
        Some(index) => PlaceInteractionDecision::insert(index, PlaceInteractionCursor::Move),
        None => PlaceInteractionDecision::clear(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacesInteractionGeometry {
    rows: Vec<PlaceRowInteractionGeometry>,
    sections: Vec<PlaceSectionInteractionGeometry>,
    content_height: f32,
}

impl PlacesInteractionGeometry {
    pub(crate) fn rows(&self) -> &[PlaceRowInteractionGeometry] {
        &self.rows
    }

    pub(crate) fn sections(&self) -> &[PlaceSectionInteractionGeometry] {
        &self.sections
    }

    pub(crate) fn entries(&self) -> usize {
        self.rows.len() + self.sections.len()
    }

    pub(crate) fn content_height(&self) -> f32 {
        self.content_height
    }

    pub(crate) fn hit_test_y(&self, y: f32) -> Option<PlaceInteractionHit<'_>> {
        if !y.is_finite() || y < 0.0 || y >= self.content_height {
            return None;
        }

        for section in &self.sections {
            if section.contains_y(y) {
                return Some(PlaceInteractionHit::Section(section));
            }
        }

        for row in &self.rows {
            if row.contains_y(y) {
                let local_y = y - row.y;
                return Some(PlaceInteractionHit::Row {
                    row,
                    drop_zone: place_drop_zone_for_y(local_y, row.height),
                });
            }
        }

        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlaceRowInteractionGeometry {
    pub(crate) visible_index: usize,
    pub(crate) place_index: usize,
    pub(crate) group: &'static str,
    pub(crate) path: PathBuf,
    pub(crate) y: f32,
    pub(crate) height: f32,
    pub(crate) insert_before_index: usize,
    pub(crate) insert_after_index: usize,
    pub(crate) mounted: bool,
}

impl PlaceRowInteractionGeometry {
    fn contains_y(&self, y: f32) -> bool {
        y >= self.y && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlaceSectionInteractionGeometry {
    pub(crate) group: &'static str,
    pub(crate) insert_index: usize,
    pub(crate) y: f32,
    pub(crate) height: f32,
}

impl PlaceSectionInteractionGeometry {
    fn contains_y(&self, y: f32) -> bool {
        y >= self.y && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PlaceInteractionHit<'a> {
    Row {
        row: &'a PlaceRowInteractionGeometry,
        drop_zone: PlaceDropZone,
    },
    Section(&'a PlaceSectionInteractionGeometry),
}

impl PlaceInteractionHit<'_> {
    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::Row { .. } => "Row",
            Self::Section(_) => "Section",
        }
    }

    pub(crate) fn drop_zone(self) -> &'static str {
        match self {
            Self::Row { drop_zone, .. } => drop_zone.as_str(),
            Self::Section(_) => "Section",
        }
    }

    pub(crate) fn visible_index(self) -> Option<usize> {
        match self {
            Self::Row { row, .. } => Some(row.visible_index),
            Self::Section(_) => None,
        }
    }

    pub(crate) fn insert_index(self) -> usize {
        match self {
            Self::Row { row, drop_zone } => match drop_zone {
                PlaceDropZone::InsertBefore => row.insert_before_index,
                PlaceDropZone::OnPlace | PlaceDropZone::InsertAfter => row.insert_after_index,
            },
            Self::Section(section) => section.insert_index,
        }
    }
}

pub(crate) fn places_interaction_geometry(places: &[PlaceSnapshot]) -> PlacesInteractionGeometry {
    let mut rows = Vec::with_capacity(places.len());
    let mut sections = Vec::new();
    let mut current_group = None;
    let mut y = 0.0;

    for (visible_index, place) in places.iter().enumerate() {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                sections.push(PlaceSectionInteractionGeometry {
                    group: place.group,
                    insert_index: place.index,
                    y,
                    height: PLACE_SECTION_HEADING_HEIGHT,
                });
                y += PLACE_SECTION_HEADING_HEIGHT;
            }
        }
        rows.push(PlaceRowInteractionGeometry {
            visible_index,
            place_index: place.index,
            group: place.group,
            path: place.path.clone(),
            y,
            height: PLACE_ROW_HEIGHT,
            insert_before_index: place.index,
            insert_after_index: place.index + 1,
            mounted: place.mounted,
        });
        y += PLACE_ROW_HEIGHT;
    }

    PlacesInteractionGeometry {
        rows,
        sections,
        content_height: y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icons::FileIconSnapshot;

    #[test]
    fn row_path_list_target_prefers_insert_edges_when_addable() {
        let path = Path::new("/tmp/target");

        assert_eq!(
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone: PlaceDropZone::InsertBefore,
                mounted: true,
                can_add_place: true,
                accepts_place: true,
                insert_before_index: 2,
                insert_after_index: 3,
                target_path: path,
            }),
            PlaceInteractionDecision::insert(2, PlaceInteractionCursor::Copy)
        );
        assert_eq!(
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone: PlaceDropZone::InsertAfter,
                mounted: true,
                can_add_place: true,
                accepts_place: true,
                insert_before_index: 2,
                insert_after_index: 3,
                target_path: path,
            }),
            PlaceInteractionDecision::insert(3, PlaceInteractionCursor::Copy)
        );
    }

    #[test]
    fn row_path_list_target_uses_place_body_only_when_mounted_and_accepted() {
        let path = Path::new("/tmp/target");

        assert_eq!(
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone: PlaceDropZone::OnPlace,
                mounted: true,
                can_add_place: false,
                accepts_place: true,
                insert_before_index: 2,
                insert_after_index: 3,
                target_path: path,
            }),
            PlaceInteractionDecision::place(path)
        );
        assert_eq!(
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone: PlaceDropZone::OnPlace,
                mounted: false,
                can_add_place: false,
                accepts_place: true,
                insert_before_index: 2,
                insert_after_index: 3,
                target_path: path,
            }),
            PlaceInteractionDecision::clear()
        );
        assert_eq!(
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone: PlaceDropZone::OnPlace,
                mounted: true,
                can_add_place: false,
                accepts_place: false,
                insert_before_index: 2,
                insert_after_index: 3,
                target_path: path,
            }),
            PlaceInteractionDecision::clear()
        );
    }

    #[test]
    fn row_place_drag_uses_body_and_edges_for_reorder() {
        assert_eq!(
            place_row_place_drag_target(true, 0, PlaceDropZone::OnPlace, 1, 2),
            PlaceInteractionDecision::insert(2, PlaceInteractionCursor::Move)
        );
        assert_eq!(
            place_row_place_drag_target(true, 2, PlaceDropZone::OnPlace, 1, 2),
            PlaceInteractionDecision::insert(1, PlaceInteractionCursor::Move)
        );
        assert_eq!(
            place_row_place_drag_target(true, 0, PlaceDropZone::InsertAfter, 1, 2),
            PlaceInteractionDecision::insert(2, PlaceInteractionCursor::Move)
        );
        assert_eq!(
            place_row_place_drag_target(true, 2, PlaceDropZone::InsertBefore, 1, 2),
            PlaceInteractionDecision::insert(1, PlaceInteractionCursor::Move)
        );
    }

    #[test]
    fn row_place_drag_rejects_noop_and_non_movable_targets() {
        assert_eq!(
            place_row_place_drag_target(true, 1, PlaceDropZone::OnPlace, 1, 2),
            PlaceInteractionDecision::clear()
        );
        assert_eq!(
            place_row_place_drag_target(true, 0, PlaceDropZone::InsertBefore, 0, 1),
            PlaceInteractionDecision::clear()
        );
        assert_eq!(
            place_row_place_drag_target(false, 0, PlaceDropZone::InsertAfter, 1, 2),
            PlaceInteractionDecision::clear()
        );
    }

    #[test]
    fn section_targets_use_insert_only_when_allowed() {
        assert_eq!(
            place_section_path_list_target(true, 4),
            PlaceInteractionDecision::insert(4, PlaceInteractionCursor::Copy)
        );
        assert_eq!(
            place_section_path_list_target(false, 4),
            PlaceInteractionDecision::clear()
        );
        assert_eq!(
            place_section_place_drag_target(true, 0, 4),
            PlaceInteractionDecision::insert(4, PlaceInteractionCursor::Move)
        );
        assert_eq!(
            place_section_place_drag_target(true, 0, 1),
            PlaceInteractionDecision::clear()
        );
    }

    #[test]
    fn interaction_geometry_matches_visual_row_and_section_stack() {
        let mut first = test_place(0, "", "Home", "/home/yk");
        first.mounted = true;
        let mut second = test_place(1, "Devices", "Root", "/");
        second.mounted = false;

        let geometry = places_interaction_geometry(&[first, second]);

        assert_eq!(geometry.rows().len(), 2);
        assert_eq!(geometry.sections().len(), 1);
        assert_eq!(geometry.entries(), 3);
        assert_eq!(
            geometry.content_height(),
            PLACE_ROW_HEIGHT * 2.0 + PLACE_SECTION_HEADING_HEIGHT
        );
        assert_eq!(geometry.rows()[0].visible_index, 0);
        assert_eq!(geometry.rows()[0].place_index, 0);
        assert_eq!(geometry.rows()[0].y, 0.0);
        assert_eq!(geometry.rows()[0].height, PLACE_ROW_HEIGHT);
        assert!(geometry.rows()[0].mounted);
        assert_eq!(geometry.sections()[0].group, "Devices");
        assert_eq!(geometry.sections()[0].insert_index, 1);
        assert_eq!(geometry.sections()[0].y, PLACE_ROW_HEIGHT);
        assert_eq!(geometry.sections()[0].height, PLACE_SECTION_HEADING_HEIGHT);
        assert_eq!(
            geometry.rows()[1].y,
            PLACE_ROW_HEIGHT + PLACE_SECTION_HEADING_HEIGHT
        );
        assert!(!geometry.rows()[1].mounted);
    }

    #[test]
    fn interaction_geometry_hit_test_routes_sections_rows_and_edges() {
        let first = test_place(0, "", "Home", "/home/yk");
        let second = test_place(1, "Devices", "Root", "/");
        let geometry = places_interaction_geometry(&[first, second]);

        assert_eq!(geometry.hit_test_y(-1.0), None);
        assert_eq!(geometry.hit_test_y(geometry.content_height()), None);
        assert_eq!(geometry.hit_test_y(f32::NAN), None);

        assert!(matches!(
            geometry.hit_test_y(1.0),
            Some(PlaceInteractionHit::Row {
                row,
                drop_zone: PlaceDropZone::InsertBefore,
            }) if row.visible_index == 0 && row.place_index == 0
        ));
        assert!(matches!(
            geometry.hit_test_y(PLACE_ROW_HEIGHT / 2.0),
            Some(PlaceInteractionHit::Row {
                row,
                drop_zone: PlaceDropZone::OnPlace,
            }) if row.visible_index == 0
        ));
        assert!(matches!(
            geometry.hit_test_y(PLACE_ROW_HEIGHT - 1.0),
            Some(PlaceInteractionHit::Row {
                row,
                drop_zone: PlaceDropZone::InsertAfter,
            }) if row.visible_index == 0
        ));
        assert!(matches!(
            geometry.hit_test_y(PLACE_ROW_HEIGHT + 1.0),
            Some(PlaceInteractionHit::Section(section)) if section.group == "Devices" && section.insert_index == 1
        ));
        assert!(matches!(
            geometry.hit_test_y(PLACE_ROW_HEIGHT + PLACE_SECTION_HEADING_HEIGHT + 10.0),
            Some(PlaceInteractionHit::Row {
                row,
                drop_zone: PlaceDropZone::OnPlace,
            }) if row.visible_index == 1 && row.place_index == 1
        ));
    }

    fn test_place(index: usize, group: &'static str, label: &str, path: &str) -> PlaceSnapshot {
        PlaceSnapshot {
            index,
            group,
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
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
