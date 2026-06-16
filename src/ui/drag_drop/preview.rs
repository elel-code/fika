const DRAG_PREVIEW_CURSOR_GAP: f32 = 8.0;

pub(crate) fn drag_preview_content_origin_for_cursor_offset(
    offset: gpui::Point<gpui::Pixels>,
) -> (f32, f32) {
    (
        offset.x.as_f32() + DRAG_PREVIEW_CURSOR_GAP,
        offset.y.as_f32() + DRAG_PREVIEW_CURSOR_GAP,
    )
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
}
