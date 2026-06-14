use std::collections::HashMap;

use crate::FikaApp;
use fika_core::ViewPoint;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

use crate::ui::icons::FileIconSnapshot;

use super::actions::{context_menu_actions, context_submenu_actions};
use super::icons::context_menu_icon_slot;
use super::layout::{
    CONTEXT_MENU_ROW_HEIGHT, ContextMenuOverlayRect, context_menu_overlay_layout,
};
use super::{
    ContextMenuAction, ContextMenuIcon, ContextMenuItem, ContextMenuNestedSubmenu,
    ContextMenuOpenSubmenu, ContextMenuState, ContextMenuSubmenu,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuRowScope {
    Root,
    Submenu,
    NestedSubmenu,
}

pub(crate) fn context_menu_overlay(
    menu: ContextMenuState,
    clipboard_available: bool,
    icon_snapshots: HashMap<ContextMenuIcon, FileIconSnapshot>,
    viewport_width: f32,
    viewport_height: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let actions = context_menu_actions(&menu.target, clipboard_available);
    let submenu = menu
        .active_submenu
        .map(|open| (open, context_submenu_actions(open.submenu, &menu.target)));
    let nested_submenu = menu.active_submenu.and_then(|open| {
        open.nested.map(|nested| {
            (
                nested,
                context_submenu_actions(nested.submenu, &menu.target),
            )
        })
    });
    let layout = context_menu_overlay_layout(
        menu.position,
        actions.len(),
        menu.active_submenu,
        submenu
            .as_ref()
            .map(|(_, actions)| actions.len())
            .unwrap_or_default(),
        nested_submenu
            .as_ref()
            .map(|(_, actions)| actions.len())
            .unwrap_or_default(),
        viewport_width,
        viewport_height,
    );
    div()
        .id("context-menu-layer")
        .absolute()
        .inset_0()
        .occlude()
        .bg(rgba(0x00000001))
        .capture_any_mouse_down(cx.listener(
            move |this, event: &gpui::MouseDownEvent, _window, cx| {
                let point = ViewPoint {
                    x: event.position.x.as_f32(),
                    y: event.position.y.as_f32(),
                };
                if !layout.contains(point) {
                    this.dismiss_context_menu();
                    cx.stop_propagation();
                    cx.notify();
                }
            },
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_context_menu();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_context_menu();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(
            cx.listener(move |this, event: &gpui::MouseMoveEvent, _window, cx| {
                let point = ViewPoint {
                    x: event.position.x.as_f32(),
                    y: event.position.y.as_f32(),
                };
                if this.set_context_menu_tree_hovered(layout.contains(point), cx) {
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .child(
            div()
                .id(format!("context-menu-{}", menu.pane_id.0))
                .absolute()
                .left(px(layout.root.x))
                .top(px(layout.root.y))
                .w(px(layout.root.width))
                .max_h(px(layout.root.max_height))
                .overflow_y_scroll()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_move(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                    if *hovered {
                        this.cancel_context_submenu_hide();
                        cx.notify();
                    }
                }))
                .children(actions.into_iter().enumerate().map(|(index, action)| {
                    context_menu_row(
                        action,
                        index,
                        ContextMenuRowScope::Root,
                        &icon_snapshots,
                        cx,
                    )
                })),
        )
        .when_some(
            submenu.zip(layout.submenu),
            |layer, ((open, actions), rect)| {
                layer.child(context_submenu_overlay(
                    open,
                    actions,
                    rect,
                    ContextMenuRowScope::Submenu,
                    &icon_snapshots,
                    cx,
                ))
            },
        )
        .when_some(
            nested_submenu.zip(layout.nested_submenu),
            |layer, ((open, actions), rect)| {
                layer.child(context_nested_submenu_overlay(
                    open,
                    actions,
                    rect,
                    &icon_snapshots,
                    cx,
                ))
            },
        )
}

fn context_submenu_overlay(
    open: ContextMenuOpenSubmenu,
    actions: Vec<ContextMenuItem>,
    rect: ContextMenuOverlayRect,
    scope: ContextMenuRowScope,
    icon_snapshots: &HashMap<ContextMenuIcon, FileIconSnapshot>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!(
            "context-submenu-{:?}-{}",
            open.submenu, open.parent_index
        ))
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .max_h(px(rect.max_height))
        .overflow_y_scroll()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xffffff))
        .shadow_md()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
            if *hovered {
                this.cancel_context_submenu_hide();
                cx.notify();
            }
        }))
        .children(
            actions
                .into_iter()
                .enumerate()
                .map(|(index, item)| context_menu_row(item, index, scope, icon_snapshots, cx)),
        )
}

fn context_nested_submenu_overlay(
    open: ContextMenuNestedSubmenu,
    actions: Vec<ContextMenuItem>,
    rect: ContextMenuOverlayRect,
    icon_snapshots: &HashMap<ContextMenuIcon, FileIconSnapshot>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!(
            "context-nested-submenu-{:?}-{}",
            open.submenu, open.parent_index
        ))
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .max_h(px(rect.max_height))
        .overflow_y_scroll()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xffffff))
        .shadow_md()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
            if *hovered {
                this.cancel_context_submenu_hide();
                cx.notify();
            }
        }))
        .children(actions.into_iter().enumerate().map(|(index, item)| {
            context_menu_row(
                item,
                index,
                ContextMenuRowScope::NestedSubmenu,
                icon_snapshots,
                cx,
            )
        }))
}

fn context_menu_row(
    item: ContextMenuItem,
    index: usize,
    scope: ContextMenuRowScope,
    icon_snapshots: &HashMap<ContextMenuIcon, FileIconSnapshot>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let action = item.action.clone();
    let submenu = item.submenu;
    let click_action = action.clone();
    let mut row = div()
        .id(format!("context-menu-action-{action:?}"))
        .flex()
        .items_center()
        .justify_between()
        .h(px(CONTEXT_MENU_ROW_HEIGHT))
        .px_2()
        .gap_2()
        .text_sm()
        .text_color(if item.enabled {
            rgb(0x24292f)
        } else {
            rgb(0x9aa4b2)
        })
        .when(item.separator_before, |row| {
            row.border_t_1().border_color(rgb(0xe5e7eb))
        })
        .when(item.enabled, |row| {
            row.hover(|row| row.bg(rgb(0xeaf1ff)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if let Some(submenu) = submenu {
                        match scope {
                            ContextMenuRowScope::Root => {
                                this.open_context_submenu(submenu, index);
                            }
                            ContextMenuRowScope::Submenu => {
                                this.open_context_nested_submenu(submenu, index);
                            }
                            ContextMenuRowScope::NestedSubmenu => {}
                        }
                    } else {
                        this.run_context_menu_action(click_action.clone(), cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }))
        })
        .child(context_menu_icon_slot(
            item.icon,
            item.enabled,
            icon_snapshots,
        ))
        .child(div().flex_1().truncate().child(item.label))
        .when(item.submenu.is_some(), |row| {
            row.child(div().text_color(rgb(0x6b7280)).child(">"))
        });

    if let Some(submenu) = item.submenu {
        row = row.on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
            if *hovered {
                match scope {
                    ContextMenuRowScope::Root => {
                        this.open_context_submenu(submenu, index);
                    }
                    ContextMenuRowScope::Submenu => {
                        this.open_context_nested_submenu(submenu, index);
                    }
                    ContextMenuRowScope::NestedSubmenu => {}
                }
                cx.notify();
            }
        }));
    } else if item.enabled && scope == ContextMenuRowScope::Root {
        row = row.on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
            if *hovered {
                this.schedule_context_submenu_hide(cx);
                cx.notify();
            }
        }));
    } else if item.enabled && scope == ContextMenuRowScope::Submenu {
        row = row.on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
            if *hovered && this.clear_context_nested_submenu() {
                cx.notify();
            }
        }));
    }
    row
}
