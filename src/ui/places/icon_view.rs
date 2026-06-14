use gpui::{Div, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::ui::icons::{FileIconSnapshot, cached_icon_or_fallback};

pub(super) fn place_icon_view(icon: &FileIconSnapshot, active: bool) -> Div {
    let fallback_kind = place_fallback_kind_for_snapshot(icon);
    let fallback_fg = if active { 0x1f4fbf } else { icon.fallback_fg };
    let fallback_bg = if active { 0xeaf1ff } else { icon.fallback_bg };
    let container = div()
        .w(px(22.0))
        .h(px(22.0))
        .flex_none()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden();

    container.child(cached_icon_or_fallback(icon, move || {
        place_fallback_icon(fallback_kind, fallback_fg, fallback_bg)
    }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceFallbackKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Trash,
    Root,
    Bookmark,
    Folder,
}

fn place_fallback_kind_for_snapshot(icon: &FileIconSnapshot) -> PlaceFallbackKind {
    let icon_name = icon.icon_name.as_ref();
    if icon_name.contains("home") {
        PlaceFallbackKind::Home
    } else if icon_name.contains("desktop") || icon_name.contains("display") {
        PlaceFallbackKind::Desktop
    } else if icon_name.contains("document") {
        PlaceFallbackKind::Documents
    } else if icon_name.contains("download") {
        PlaceFallbackKind::Downloads
    } else if icon_name.contains("music") || icon_name.contains("audio") {
        PlaceFallbackKind::Music
    } else if icon_name.contains("picture")
        || icon_name.contains("image")
        || icon_name.contains("photo")
    {
        PlaceFallbackKind::Pictures
    } else if icon_name.contains("video") {
        PlaceFallbackKind::Videos
    } else if icon_name.contains("trash") {
        PlaceFallbackKind::Trash
    } else if icon_name.contains("harddisk") || icon_name.contains("root") {
        PlaceFallbackKind::Root
    } else if icon_name.contains("favorite") || icon_name.contains("bookmark") {
        PlaceFallbackKind::Bookmark
    } else {
        PlaceFallbackKind::Folder
    }
}

fn place_fallback_icon(kind: PlaceFallbackKind, fg: u32, bg: u32) -> gpui::AnyElement {
    let icon = div()
        .size_full()
        .rounded_md()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg))
        .overflow_hidden();

    match kind {
        PlaceFallbackKind::Home => icon
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(6.0))
                    .w(px(10.0))
                    .h(px(4.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .top(px(10.0))
                    .w(px(12.0))
                    .h(px(8.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(13.0))
                    .w(px(3.0))
                    .h(px(5.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Desktop => icon
            .child(
                div()
                    .absolute()
                    .left(px(4.0))
                    .top(px(5.0))
                    .w(px(14.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(15.0))
                    .w(px(2.0))
                    .h(px(3.0))
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(18.0))
                    .w(px(8.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Documents => icon
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(4.0))
                    .w(px(10.0))
                    .h(px(14.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(9.0))
                    .w(px(6.0))
                    .h(px(1.0))
                    .bg(rgb(bg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(12.0))
                    .w(px(6.0))
                    .h(px(1.0))
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Downloads => folder_icon_shape(icon, fg, bg)
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(8.0))
                    .w(px(2.0))
                    .h(px(7.0))
                    .bg(rgb(bg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(13.0))
                    .w(px(6.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Music => icon
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(5.0))
                    .w(px(2.0))
                    .h(px(10.0))
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(13.0))
                    .w(px(7.0))
                    .h(px(5.0))
                    .rounded_full()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(5.0))
                    .w(px(6.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Pictures => icon
            .child(
                div()
                    .absolute()
                    .left(px(4.0))
                    .top(px(5.0))
                    .w(px(14.0))
                    .h(px(12.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(8.0))
                    .w(px(3.0))
                    .h(px(3.0))
                    .rounded_full()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(14.0))
                    .w(px(10.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Videos => icon
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .top(px(6.0))
                    .w(px(12.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(9.0))
                    .w(px(6.0))
                    .h(px(4.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Trash => icon
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(5.0))
                    .w(px(8.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(8.0))
                    .w(px(10.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(10.0))
                    .w(px(2.0))
                    .h(px(6.0))
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Root => icon
            .child(
                div()
                    .absolute()
                    .left(px(4.0))
                    .top(px(6.0))
                    .w(px(14.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(12.0))
                    .w(px(8.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(15.0))
                    .top(px(8.0))
                    .w(px(2.0))
                    .h(px(2.0))
                    .rounded_full()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Bookmark => icon
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(4.0))
                    .w(px(8.0))
                    .h(px(14.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(9.0))
                    .top(px(14.0))
                    .w(px(4.0))
                    .h(px(4.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Folder => folder_icon_shape(icon, fg, bg),
    }
    .into_any_element()
}

fn folder_icon_shape(icon: Div, fg: u32, bg: u32) -> Div {
    icon.child(
        div()
            .absolute()
            .left(px(5.0))
            .top(px(6.0))
            .w(px(7.0))
            .h(px(4.0))
            .rounded_sm()
            .bg(rgb(fg)),
    )
    .child(
        div()
            .absolute()
            .left(px(4.0))
            .top(px(9.0))
            .w(px(14.0))
            .h(px(8.0))
            .rounded_sm()
            .bg(rgb(fg)),
    )
    .child(
        div()
            .absolute()
            .left(px(6.0))
            .top(px(11.0))
            .w(px(10.0))
            .h(px(2.0))
            .rounded_sm()
            .bg(rgb(bg)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon_snapshot(icon_name: &str, fallback_marker: &str) -> FileIconSnapshot {
        FileIconSnapshot {
            icon_name: std::sync::Arc::from(icon_name),
            path: None,
            render_image: None,
            fallback_marker: std::sync::Arc::from(fallback_marker),
            fallback_fg: 0x1f4fbf,
            fallback_bg: 0xeaf1ff,
        }
    }

    #[test]
    fn place_fallback_kind_uses_icon_identity_not_text_marker() {
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("user-desktop", "D")),
            PlaceFallbackKind::Desktop
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("folder-documents", "D")),
            PlaceFallbackKind::Documents
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("folder-download", "DL")),
            PlaceFallbackKind::Downloads
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("user-trash", "T")),
            PlaceFallbackKind::Trash
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("folder", "Documents")),
            PlaceFallbackKind::Folder
        );
    }
}
