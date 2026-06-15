use crate::FikaApp;
use crate::ui::filter_bar::FilterToggleSnapshot;
use crate::ui::icons::{FileIconCache, FileIconSnapshot, cached_icon_or_fallback};
use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb};

pub(crate) fn pane_split_icon_snapshot(cache: &mut FileIconCache) -> FileIconSnapshot {
    cache.named_icon(
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
    )
}

pub(crate) fn pane_close_icon_snapshot(cache: &mut FileIconCache) -> FileIconSnapshot {
    cache.named_icon(
        "pane-close",
        &["window-close", "dialog-close", "edit-delete"],
        "Close",
        0x475569,
        0xf1f5f9,
        18.0,
    )
}

pub(crate) fn filter_pane_button(
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

pub(crate) fn split_pane_button(
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

pub(crate) fn close_pane_button(
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
    div()
        .w(px(18.0))
        .h(px(18.0))
        .flex_none()
        .overflow_hidden()
        .child(cached_icon_or_fallback(&icon, move || {
            toolbar_icon_fallback_label(label, enabled)
        }))
        .into_any_element()
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
