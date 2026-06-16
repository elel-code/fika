const DRAG_PREVIEW_CURSOR_GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DragPreviewLayout {
    pub(crate) content_origin_x: f32,
    pub(crate) content_origin_y: f32,
    pub(crate) surface_width: f32,
    pub(crate) surface_height: f32,
}

pub(crate) fn drag_preview_content_origin_for_cursor_offset(
    offset: gpui::Point<gpui::Pixels>,
) -> (f32, f32) {
    (
        offset.x.as_f32() + DRAG_PREVIEW_CURSOR_GAP,
        offset.y.as_f32() + DRAG_PREVIEW_CURSOR_GAP,
    )
}

pub(crate) fn drag_preview_layout_for_cursor_offset(
    offset: gpui::Point<gpui::Pixels>,
    content_width: f32,
    surface_content_height: f32,
) -> DragPreviewLayout {
    let (content_origin_x, content_origin_y) =
        drag_preview_content_origin_for_cursor_offset(offset);
    DragPreviewLayout {
        content_origin_x,
        content_origin_y,
        surface_width: content_origin_x.max(0.0) + content_width,
        surface_height: content_origin_y.max(0.0) + surface_content_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Pixels, Point, point, px};

    fn global_content_origin(mouse: Point<Pixels>, cursor_offset: Point<Pixels>) -> (f32, f32) {
        let root_x = mouse.x.as_f32() - cursor_offset.x.as_f32();
        let root_y = mouse.y.as_f32() - cursor_offset.y.as_f32();
        let (content_x, content_y) = drag_preview_content_origin_for_cursor_offset(cursor_offset);
        (root_x + content_x, root_y + content_y)
    }

    fn global_layout_content_origin(
        mouse: Point<Pixels>,
        cursor_offset: Point<Pixels>,
    ) -> (f32, f32) {
        let root_x = mouse.x.as_f32() - cursor_offset.x.as_f32();
        let root_y = mouse.y.as_f32() - cursor_offset.y.as_f32();
        let layout = drag_preview_layout_for_cursor_offset(cursor_offset, 220.0, 42.0);
        (
            root_x + layout.content_origin_x,
            root_y + layout.content_origin_y,
        )
    }

    #[test]
    fn content_origin_compensates_gpui_drag_hotspot() {
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(point(px(48.0), px(12.0))),
            (56.0, 20.0)
        );
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(point(px(-12.0), px(-10.0))),
            (-4.0, -2.0)
        );
    }

    #[test]
    fn drag_preview_content_origin_stays_cursor_relative_for_varied_source_offsets() {
        let mouse = point(px(800.0), px(420.0));
        let offsets = [
            point(px(8.0), px(8.0)),
            point(px(56.0), px(20.0)),
            point(px(360.0), px(18.0)),
            point(px(1440.0), px(32.0)),
            point(px(-12.0), px(-10.0)),
        ];

        for offset in offsets {
            assert_eq!(global_content_origin(mouse, offset), (808.0, 428.0));
        }
    }

    #[test]
    fn drag_preview_layout_stays_cursor_relative_across_view_shapes() {
        let mouse = point(px(800.0), px(420.0));
        let offsets = [
            point(px(24.0), px(18.0)),
            point(px(80.0), px(42.0)),
            point(px(190.0), px(86.0)),
            point(px(720.0), px(15.0)),
            point(px(1440.0), px(32.0)),
        ];

        for offset in offsets {
            assert_eq!(global_layout_content_origin(mouse, offset), (808.0, 428.0));
            let layout = drag_preview_layout_for_cursor_offset(offset, 220.0, 42.0);
            assert!(layout.surface_width >= 220.0);
            assert!(layout.surface_height >= 42.0);
        }
    }
}
