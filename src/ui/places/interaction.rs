use std::path::{Path, PathBuf};

use super::drag::{PlaceDropZone, place_drag_insert_index, place_drag_insert_index_for_zone};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
