#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConstraintAdjustments, LogicalRect};

    #[test]
    fn positioner_rejects_zero_sized_geometry() {
        let positioner = PopupPositioner {
            size: LogicalSize::new(0, 20),
            ..PopupPositioner::default()
        };
        assert!(matches!(
            validate_positioner(&positioner),
            Err(RuntimeError::InvalidPositioner(_))
        ));
    }

    #[test]
    fn positioner_preserves_all_constraint_bits() {
        let all = ConstraintAdjustments::all();
        let mapped = map_constraints(all);
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::SlideX));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::SlideY));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::FlipX));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::FlipY));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::ResizeX));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::ResizeY));
    }

    #[test]
    fn blur_region_keeps_surface_local_rectangles() {
        let region = BlurRegion::Rectangles(vec![LogicalRect::new(4, 8, 120, 40)]);
        assert_eq!(
            region,
            BlurRegion::Rectangles(vec![LogicalRect::new(4, 8, 120, 40)])
        );
    }

    #[test]
    fn nested_popups_are_removed_before_their_parents() {
        let root = SurfaceId(1);
        let popup = SurfaceId(2);
        let nested_popup = SurfaceId(3);
        let dialog = SurfaceId(4);
        let children = HashMap::from([(root, vec![popup, dialog]), (popup, vec![nested_popup])]);
        let mut order = Vec::new();

        collect_post_order(&children, root, &mut order);

        assert_eq!(order, vec![nested_popup, popup, dialog, root]);
    }

    #[test]
    fn dnd_actions_round_trip_all_protocol_bits() {
        let actions = DndActions::COPY | DndActions::MOVE | DndActions::ASK;
        assert_eq!(dnd_actions(map_dnd_actions(actions)), actions);
        assert_eq!(dnd_action(map_dnd_action(DndAction::Copy)), Some(DndAction::Copy));
        assert_eq!(dnd_action(map_dnd_action(DndAction::Move)), Some(DndAction::Move));
        assert_eq!(dnd_action(map_dnd_action(DndAction::Ask)), Some(DndAction::Ask));
    }

    #[test]
    fn cursor_icons_cover_fika_runtime_vocabulary() {
        assert_eq!(map_cursor_icon(CursorIcon::Default), SctkCursorIcon::Default);
        assert_eq!(map_cursor_icon(CursorIcon::Pointer), SctkCursorIcon::Pointer);
        assert_eq!(map_cursor_icon(CursorIcon::Text), SctkCursorIcon::Text);
        assert_eq!(
            map_cursor_icon(CursorIcon::ColResize),
            SctkCursorIcon::ColResize
        );
    }

    #[test]
    fn drag_icon_rgba_is_premultiplied_and_encoded_as_native_argb() {
        let mut encoded = [0; 8];
        copy_rgba_to_premultiplied_argb8888(
            &[200, 100, 50, 128, 255, 200, 100, 0],
            &mut encoded,
        );

        assert_eq!(u32::from_ne_bytes(encoded[..4].try_into().unwrap()), 0x8064_3219);
        assert_eq!(u32::from_ne_bytes(encoded[4..].try_into().unwrap()), 0);
    }

    #[test]
    fn drag_seat_requires_origin_focus_data_device_and_matching_button_surface() {
        let origin = SurfaceId(7);
        let other = SurfaceId(8);
        let candidates = [
            (
                1,
                Some(other),
                true,
                Some(ButtonSerial {
                    surface: other,
                    serial: 10,
                    order: 1,
                }),
            ),
            (
                2,
                Some(origin),
                false,
                Some(ButtonSerial {
                    surface: origin,
                    serial: 20,
                    order: 2,
                }),
            ),
            (
                3,
                Some(origin),
                true,
                Some(ButtonSerial {
                    surface: other,
                    serial: 30,
                    order: 3,
                }),
            ),
        ];

        assert_eq!(select_drag_seat(origin, candidates), None);
    }

    #[test]
    fn drag_seat_uses_newest_matching_button_across_multiple_seats() {
        let origin = SurfaceId(7);
        let button = |serial, order| {
            Some(ButtonSerial {
                surface: origin,
                serial,
                order,
            })
        };
        let candidates = [
            (11, Some(origin), true, button(110, 4)),
            (12, Some(origin), true, button(120, 9)),
            (13, Some(origin), true, None),
        ];

        assert_eq!(select_drag_seat(origin, candidates), Some((12, 120)));
    }

    #[test]
    fn selection_seat_uses_newest_focused_data_device_serial() {
        let input = |serial, order| Some(SelectionSerial { serial, order });
        let candidates = [
            (1, true, true, input(10, 2)),
            (2, false, true, input(20, 8)),
            (3, true, false, input(30, 9)),
            (4, true, true, input(40, 7)),
        ];

        assert_eq!(select_selection_seat(candidates), Some((4, 40)));
    }
}
