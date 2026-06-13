use crate::FikaApp;
use crate::ui::filter_bar::{FilterToggleSnapshot, filter_toggle_snapshot};
use crate::ui::icons::{FileIconCache, FileIconSnapshot};
use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{
    Context, Div, MouseButton, ParentElement, Stateful, Styled, StyledImage, div, img, px, rgb,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaneToolbarSnapshot {
    pub(crate) filter_toggle: FilterToggleSnapshot,
    pub(crate) split_icon: FileIconSnapshot,
    pub(crate) close_icon: FileIconSnapshot,
    pub(crate) split_enabled: bool,
    pub(crate) close_enabled: bool,
}

pub(crate) fn pane_toolbar_snapshot(
    cache: &mut FileIconCache,
    filter_active: bool,
    pane_count: usize,
) -> PaneToolbarSnapshot {
    PaneToolbarSnapshot {
        filter_toggle: filter_toggle_snapshot(cache, filter_active),
        split_icon: cache.named_icon(
            "pane-split",
            &[
                "view-split-left-right",
                "view-split-left-right-symbolic",
                "view-restore",
            ],
            "Split",
            0x1f4fbf,
            0xeaf1ff,
            18.0,
        ),
        close_icon: cache.named_icon(
            "pane-close",
            &["window-close", "dialog-close", "edit-delete"],
            "Close",
            0x475569,
            0xf1f5f9,
            18.0,
        ),
        split_enabled: pane_count == 1,
        close_enabled: pane_count > 1,
    }
}

pub(super) fn pane_toolbar_buttons(
    pane_id: PaneId,
    toolbar: PaneToolbarSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("pane-toolbar-{}", pane_id.0))
        .flex()
        .items_center()
        .gap_1()
        .flex_none()
        .child(filter_toggle_button(pane_id, toolbar.filter_toggle, cx))
        .child(split_pane_button(
            pane_id,
            toolbar.split_icon,
            toolbar.split_enabled,
            cx,
        ))
        .child(close_pane_button(
            pane_id,
            toolbar.close_icon,
            toolbar.close_enabled,
            cx,
        ))
}

fn filter_toggle_button(
    pane_id: PaneId,
    toggle: FilterToggleSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let active = toggle.active;
    let label = toggle.label;
    toolbar_button_base(
        format!("filter-toggle-{}", pane_id.0),
        toggle.icon,
        label,
        active,
        true,
    )
    .on_click(
        cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
            if event.standard_click() {
                this.toggle_filter_bar_from_button(pane_id);
                cx.stop_propagation();
                cx.notify();
            }
        }),
    )
}

fn split_pane_button(
    pane_id: PaneId,
    icon: FileIconSnapshot,
    enabled: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    toolbar_button_base(
        format!("pane-split-button-{}", pane_id.0),
        icon,
        "Split",
        false,
        enabled,
    )
    .when(enabled, |button| {
        button.on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() {
                    this.split_pane_from_button(pane_id);
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
    })
}

fn close_pane_button(
    pane_id: PaneId,
    icon: FileIconSnapshot,
    enabled: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    toolbar_button_base(
        format!("pane-close-button-{}", pane_id.0),
        icon,
        "Close Pane",
        false,
        enabled,
    )
    .when(enabled, |button| {
        button.on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() {
                    this.close_pane_from_button(pane_id);
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
    })
}

fn toolbar_button_base(
    id: String,
    icon: FileIconSnapshot,
    label: &'static str,
    active: bool,
    enabled: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .min_w(px(28.0))
        .px_1()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .border_color(if active { rgb(0x2f6fed) } else { rgb(0xb6bcc6) })
        .bg(if active { rgb(0xeaf1ff) } else { rgb(0xffffff) })
        .when(!enabled, |button| {
            button.bg(rgb(0xf1f3f5)).border_color(rgb(0xd5d9df))
        })
        .when(enabled, |button| {
            button
                .hover(|button| button.bg(rgb(0xdbe7fb)))
                .cursor_pointer()
        })
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(toolbar_icon_or_label(icon, label, enabled))
}

fn toolbar_icon_or_label(
    icon: FileIconSnapshot,
    label: &'static str,
    enabled: bool,
) -> gpui::AnyElement {
    match icon.path {
        Some(path) => div()
            .w(px(18.0))
            .h(px(18.0))
            .flex_none()
            .overflow_hidden()
            .child(
                img(path)
                    .size_full()
                    .with_fallback(move || toolbar_icon_fallback_label(label, enabled)),
            )
            .into_any_element(),
        None => toolbar_icon_fallback_label(label, enabled),
    }
}

fn toolbar_icon_fallback_label(label: &'static str, enabled: bool) -> gpui::AnyElement {
    div()
        .px_1()
        .text_xs()
        .text_color(if enabled {
            rgb(0x1f2937)
        } else {
            rgb(0x8b95a1)
        })
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_toolbar_snapshot_tracks_split_and_close_availability() {
        let mut cache = FileIconCache::default();

        let single = pane_toolbar_snapshot(&mut cache, false, 1);
        assert!(single.split_enabled);
        assert!(!single.close_enabled);
        assert!(matches!(
            single.split_icon.icon_name.as_str(),
            "view-split-left-right" | "view-split-left-right-symbolic" | "view-restore"
        ));

        let split = pane_toolbar_snapshot(&mut cache, true, 2);
        assert!(!split.split_enabled);
        assert!(split.close_enabled);
        assert!(split.filter_toggle.active);
        assert!(matches!(
            split.close_icon.icon_name.as_str(),
            "window-close" | "dialog-close" | "edit-delete"
        ));
    }
}
