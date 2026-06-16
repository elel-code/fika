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
    use gpui::{point, px};

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
}
