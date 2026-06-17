use gpui::SharedString;

pub(super) fn static_paint_single_line_text(text: SharedString) -> SharedString {
    if text.as_ref().contains('\n') {
        SharedString::from(text.as_ref().replace('\n', " "))
    } else {
        text
    }
}
