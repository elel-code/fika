use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::FikaApp;
use fika_core::{
    MimeApplication, PaneId, ServiceMenuAction, ServiceMenuPriority, ViewPoint,
    is_archive_mime_or_path,
};
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, img, px, rgb, rgba};

use super::icons::{FileIconCache, FileIconSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextMenuSubmenu {
    CreateNew,
    OpenWith,
    ServiceMenu,
    ServiceMenuGroup(usize),
    SortBy,
    TrashSortBy,
    ViewMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextMenuOpenSubmenu {
    pub(crate) submenu: ContextMenuSubmenu,
    pub(crate) parent_index: usize,
    pub(crate) nested: Option<ContextMenuNestedSubmenu>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextMenuNestedSubmenu {
    pub(crate) submenu: ContextMenuSubmenu,
    pub(crate) parent_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextMenuState {
    pub(crate) pane_id: PaneId,
    pub(crate) target: ContextMenuTarget,
    pub(crate) position: ViewPoint,
    pub(crate) active_submenu: Option<ContextMenuOpenSubmenu>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ContextMenuTarget {
    Blank {
        trash_view: bool,
        trash_has_items: bool,
        service_actions: Vec<ServiceMenuAction>,
    },
    PlacesBlank {
        has_hidden_places: bool,
    },
    PlaceSection {
        group: &'static str,
    },
    Place {
        path: PathBuf,
        mounted: bool,
        device: bool,
        device_ejectable: bool,
        device_can_power_off: bool,
        trash_place: bool,
        trash_has_items: bool,
        editable: bool,
        removable: bool,
    },
    Item {
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
        trash_view: bool,
        trash_can_restore: bool,
        mime_type: Option<Arc<str>>,
        open_with_apps: Vec<MimeApplication>,
        service_actions: Vec<ServiceMenuAction>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ContextMenuAction {
    Open,
    OpenInNewPane,
    OpenInNewWindow,
    OpenWithSubmenu,
    OpenWithApplication { desktop_id: String },
    OtherApplication,
    CreateNewSubmenu,
    ServiceMenuSubmenu,
    ServiceMenuGroupSubmenu { group_index: usize },
    RunServiceMenuAction { action_id: String },
    CompressWithArk,
    ExtractHereWithArk,
    ExtractToWithArk,
    MountDevice,
    UnmountDevice,
    EjectDevice,
    SafelyRemoveDevice,
    AddPlace,
    EditPlace,
    RemovePlace,
    HidePlace,
    HidePlaceSection,
    ShowHiddenPlaces,
    SortBySubmenu,
    ViewModeSubmenu,
    SortByName,
    SortByModified,
    SortBySize,
    SortByOriginalPath,
    SortByDeletionTime,
    SortAscending,
    SortDescending,
    SortFoldersFirst,
    SortHiddenLast,
    ViewCompact,
    ViewIcons,
    ViewDetails,
    Rename,
    Copy,
    CopyLocation,
    Cut,
    Trash,
    RestoreFromTrash,
    DeletePermanently,
    EmptyTrash,
    Properties,
    CreateFolder,
    CreateFile,
    Paste,
    SelectAll,
    Refresh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContextMenuItem {
    pub(crate) action: ContextMenuAction,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) submenu: Option<ContextMenuSubmenu>,
    pub(crate) icon: Option<ContextMenuIcon>,
    pub(crate) separator_before: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ContextMenuIcon {
    Named(String),
    Open,
    NewWindow,
    OpenWith,
    Application,
    Service,
    Archive,
    CreateNew,
    NewFolder,
    NewFile,
    Edit,
    Remove,
    Hide,
    Sort,
    View,
    Rename,
    Copy,
    Cut,
    Paste,
    Location,
    Trash,
    Restore,
    Delete,
    Properties,
    Select,
    Refresh,
    Place,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContextMenuOverlayRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) max_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContextMenuOverlayLayout {
    pub(crate) root: ContextMenuOverlayRect,
    pub(crate) submenu: Option<ContextMenuOverlayRect>,
    pub(crate) nested_submenu: Option<ContextMenuOverlayRect>,
}

impl ContextMenuOverlayLayout {
    pub(crate) fn contains(self, point: ViewPoint) -> bool {
        self.root.contains(point)
            || self.submenu.is_some_and(|rect| rect.contains(point))
            || self.nested_submenu.is_some_and(|rect| rect.contains(point))
    }
}

impl ContextMenuOverlayRect {
    fn contains(self, point: ViewPoint) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width
            && point.y >= self.y
            && point.y < self.y + self.max_height
    }
}

const CONTEXT_MENU_WIDTH: f32 = 196.0;
pub(crate) const CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
pub(crate) const CONTEXT_MENU_VERTICAL_PADDING: f32 = 4.0;
pub(crate) const CONTEXT_MENU_VIEWPORT_MARGIN: f32 = 8.0;

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

pub(crate) fn context_menu_icon_snapshots(
    cache: &mut FileIconCache,
    menu: &ContextMenuState,
    clipboard_available: bool,
) -> HashMap<ContextMenuIcon, FileIconSnapshot> {
    let mut snapshots = HashMap::new();
    collect_context_menu_icon_snapshots(
        cache,
        &context_menu_actions(&menu.target, clipboard_available),
        &mut snapshots,
    );
    if let Some(open) = menu.active_submenu {
        collect_context_menu_icon_snapshots(
            cache,
            &context_submenu_actions(open.submenu, &menu.target),
            &mut snapshots,
        );
        if let Some(nested) = open.nested {
            collect_context_menu_icon_snapshots(
                cache,
                &context_submenu_actions(nested.submenu, &menu.target),
                &mut snapshots,
            );
        }
    }
    snapshots
}

fn collect_context_menu_icon_snapshots(
    cache: &mut FileIconCache,
    actions: &[ContextMenuItem],
    snapshots: &mut HashMap<ContextMenuIcon, FileIconSnapshot>,
) {
    for icon in actions.iter().filter_map(|action| action.icon.clone()) {
        snapshots
            .entry(icon.clone())
            .or_insert_with(|| context_menu_icon_snapshot(cache, icon));
    }
}

fn context_menu_icon_snapshot(
    cache: &mut FileIconCache,
    icon: ContextMenuIcon,
) -> FileIconSnapshot {
    let (name, candidates) = context_menu_theme_icon_candidates(&icon);
    let (marker, fg, bg) = context_menu_icon_style(&icon, true);
    let candidate_refs = candidates.iter().map(String::as_str).collect::<Vec<_>>();
    cache.named_icon(&name, &candidate_refs, marker, fg, bg, 18.0)
}

fn context_menu_theme_icon_candidates(icon: &ContextMenuIcon) -> (String, Vec<String>) {
    match icon {
        ContextMenuIcon::Named(name) => {
            let name = name.trim();
            (
                name.to_string(),
                vec![name.to_string(), "application-x-executable".to_string()],
            )
        }
        ContextMenuIcon::Open => (
            "context-open".to_string(),
            ["document-open", "folder-open", "system-file-manager"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::NewWindow => icon_candidates("context-new-window", &["window-new"]),
        ContextMenuIcon::OpenWith => (
            "context-open-with".to_string(),
            [
                "preferences-desktop-default-applications",
                "application-x-executable",
                "system-run",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        ),
        ContextMenuIcon::Application => (
            "context-application".to_string(),
            [
                "application-x-executable",
                "system-run",
                "application-default-icon",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        ),
        ContextMenuIcon::Service => (
            "context-service".to_string(),
            ["configure", "preferences-system", "system-run"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::Archive => (
            "context-archive".to_string(),
            ["ark", "package-x-generic", "application-x-archive"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::CreateNew => icon_candidates("context-create-new", &["list-add"]),
        ContextMenuIcon::NewFolder => (
            "context-new-folder".to_string(),
            ["folder-new", "document-new", "folder"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::NewFile => icon_candidates("context-new-file", &["document-new"]),
        ContextMenuIcon::Edit => icon_candidates("context-edit", &["document-edit", "edit-rename"]),
        ContextMenuIcon::Remove => {
            icon_candidates("context-remove", &["list-remove", "edit-delete"])
        }
        ContextMenuIcon::Hide => {
            icon_candidates("context-hide", &["hint", "view-hidden", "visibility"])
        }
        ContextMenuIcon::Sort => (
            "context-sort".to_string(),
            ["view-sort-ascending", "view-sort-descending", "sort-name"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::View => (
            "context-view".to_string(),
            ["view-list-icons", "view-list-details", "view-list-tree"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::Rename => {
            icon_candidates("context-rename", &["edit-rename", "document-edit"])
        }
        ContextMenuIcon::Copy => icon_candidates("context-copy", &["edit-copy"]),
        ContextMenuIcon::Cut => icon_candidates("context-cut", &["edit-cut"]),
        ContextMenuIcon::Paste => icon_candidates("context-paste", &["edit-paste"]),
        ContextMenuIcon::Location => (
            "context-location".to_string(),
            ["edit-copy-path", "edit-copy", "folder-open"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::Trash => icon_candidates("context-trash", &["user-trash", "edit-delete"]),
        ContextMenuIcon::Restore => {
            icon_candidates("context-restore", &["edit-undo", "user-trash"])
        }
        ContextMenuIcon::Delete => {
            icon_candidates("context-delete", &["edit-delete", "edit-delete-shred"])
        }
        ContextMenuIcon::Properties => (
            "context-properties".to_string(),
            ["document-properties", "dialog-information"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
        ContextMenuIcon::Select => icon_candidates("context-select", &["edit-select-all"]),
        ContextMenuIcon::Refresh => icon_candidates("context-refresh", &["view-refresh"]),
        ContextMenuIcon::Place => (
            "context-place".to_string(),
            ["bookmark-new", "folder-favorites", "folder"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
    }
}

fn icon_candidates(name: &str, candidates: &[&str]) -> (String, Vec<String>) {
    (
        name.to_string(),
        candidates
            .iter()
            .map(|candidate| (*candidate).to_string())
            .collect(),
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

fn context_menu_icon_slot(
    icon: Option<ContextMenuIcon>,
    enabled: bool,
    icon_snapshots: &HashMap<ContextMenuIcon, FileIconSnapshot>,
) -> Div {
    let Some(icon) = icon else {
        return div().w(px(18.0)).h(px(18.0)).flex_none();
    };
    let snapshot = icon_snapshots
        .get(&icon)
        .cloned()
        .unwrap_or_else(|| context_menu_icon_fallback_snapshot(icon));
    let fallback = snapshot.fallback_marker.clone();
    let fallback_fg = if enabled {
        snapshot.fallback_fg
    } else {
        0x8b95a1
    };
    let fallback_bg = if enabled {
        snapshot.fallback_bg
    } else {
        0xf1f3f5
    };
    let container = div()
        .w(px(18.0))
        .h(px(18.0))
        .flex_none()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden();

    match snapshot.path {
        Some(path) => container.child(img(path).size_full().with_fallback(move || {
            context_menu_fallback_icon(fallback.clone(), fallback_fg, fallback_bg)
        })),
        None => container.child(context_menu_fallback_icon(
            fallback,
            fallback_fg,
            fallback_bg,
        )),
    }
}

fn context_menu_icon_fallback_snapshot(icon: ContextMenuIcon) -> FileIconSnapshot {
    let (marker, fg, bg) = context_menu_icon_style(&icon, true);
    FileIconSnapshot {
        icon_name: format!("{icon:?}"),
        path: None,
        fallback_marker: marker.to_string(),
        fallback_fg: fg,
        fallback_bg: bg,
    }
}

fn context_menu_fallback_icon(marker: String, fg: u32, bg: u32) -> gpui::AnyElement {
    div()
        .size_full()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(fg))
        .bg(rgb(bg))
        .child(marker)
        .into_any_element()
}

fn context_menu_icon_style(icon: &ContextMenuIcon, enabled: bool) -> (&'static str, u32, u32) {
    let (marker, fg, bg) = match icon {
        ContextMenuIcon::Named(_) => ("S", 0x0f766e, 0xe6fffb),
        ContextMenuIcon::Open => ("O", 0x1d4ed8, 0xeaf1ff),
        ContextMenuIcon::NewWindow => ("W", 0x1d4ed8, 0xeaf1ff),
        ContextMenuIcon::OpenWith => ("W", 0x4338ca, 0xeeedff),
        ContextMenuIcon::Application => ("A", 0x4f46e5, 0xeeedff),
        ContextMenuIcon::Service => ("S", 0x0f766e, 0xe6fffb),
        ContextMenuIcon::Archive => ("Z", 0x7c2d12, 0xffefe8),
        ContextMenuIcon::CreateNew => ("+", 0x0f4c81, 0xe7f1fb),
        ContextMenuIcon::NewFolder => ("+", 0x0f4c81, 0xe7f1fb),
        ContextMenuIcon::NewFile => ("F", 0x0f4c81, 0xe7f1fb),
        ContextMenuIcon::Edit => ("E", 0x6d28d9, 0xf2edff),
        ContextMenuIcon::Remove => ("-", 0xb91c1c, 0xffe8e8),
        ContextMenuIcon::Hide => ("H", 0x59636e, 0xeef1f5),
        ContextMenuIcon::Sort => ("S", 0x92400e, 0xfff3df),
        ContextMenuIcon::View => ("V", 0x1f4fbf, 0xeaf1ff),
        ContextMenuIcon::Rename => ("R", 0x6d28d9, 0xf2edff),
        ContextMenuIcon::Copy => ("C", 0x2563eb, 0xeaf1ff),
        ContextMenuIcon::Cut => ("X", 0xb45309, 0xfff3df),
        ContextMenuIcon::Paste => ("P", 0x047857, 0xe7f8ef),
        ContextMenuIcon::Location => ("L", 0x334155, 0xe8eef7),
        ContextMenuIcon::Trash => ("T", 0xb91c1c, 0xffe8e8),
        ContextMenuIcon::Restore => ("U", 0x047857, 0xe7f8ef),
        ContextMenuIcon::Delete => ("D", 0xb91c1c, 0xffe8e8),
        ContextMenuIcon::Properties => ("I", 0x374151, 0xeef1f5),
        ContextMenuIcon::Select => ("A", 0x1f4fbf, 0xeaf1ff),
        ContextMenuIcon::Refresh => ("R", 0x0f766e, 0xe6fffb),
        ContextMenuIcon::Place => ("P", 0x0f766e, 0xe6fffb),
    };
    if enabled {
        (marker, fg, bg)
    } else {
        (marker, 0x8b95a1, 0xf1f3f5)
    }
}

pub(crate) fn context_menu_actions(
    target: &ContextMenuTarget,
    clipboard_available: bool,
) -> Vec<ContextMenuItem> {
    match target {
        ContextMenuTarget::Blank {
            trash_view: true,
            trash_has_items,
            ..
        } => vec![
            context_menu_item_enabled(
                ContextMenuAction::EmptyTrash,
                "Empty Trash",
                *trash_has_items,
            ),
            context_menu_separator_before(context_menu_submenu_item(
                ContextMenuAction::SortBySubmenu,
                "Sort By",
                ContextMenuSubmenu::TrashSortBy,
            )),
            context_menu_submenu_item(
                ContextMenuAction::ViewModeSubmenu,
                "View Mode",
                ContextMenuSubmenu::ViewMode,
            ),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::SelectAll,
                "Select All",
            )),
            context_menu_item(ContextMenuAction::Refresh, "Refresh"),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )),
        ],
        ContextMenuTarget::Blank {
            trash_view: false,
            service_actions,
            ..
        } => {
            let mut actions = vec![
                context_menu_submenu_item(
                    ContextMenuAction::CreateNewSubmenu,
                    "Create New",
                    ContextMenuSubmenu::CreateNew,
                ),
                context_menu_separator_before(context_menu_item_enabled(
                    ContextMenuAction::Paste,
                    "Paste",
                    clipboard_available,
                )),
            ];
            let service_root_actions =
                context_menu_group_items(service_menu_root_actions(service_actions));
            let has_service_root_actions = !service_root_actions.is_empty();
            actions.extend(service_root_actions);
            if service_menu_has_more_actions(service_actions) {
                let more_actions = context_menu_submenu_item(
                    ContextMenuAction::ServiceMenuSubmenu,
                    "More Actions",
                    ContextMenuSubmenu::ServiceMenu,
                );
                actions.push(if has_service_root_actions {
                    more_actions
                } else {
                    context_menu_separator_before(more_actions)
                });
            }
            actions.extend([
                context_menu_separator_before(context_menu_submenu_item(
                    ContextMenuAction::SortBySubmenu,
                    "Sort By",
                    ContextMenuSubmenu::SortBy,
                )),
                context_menu_submenu_item(
                    ContextMenuAction::ViewModeSubmenu,
                    "View Mode",
                    ContextMenuSubmenu::ViewMode,
                ),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::SelectAll,
                    "Select All",
                )),
                context_menu_item(ContextMenuAction::Refresh, "Refresh"),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Properties,
                    "Properties",
                )),
            ]);
            actions
        }
        ContextMenuTarget::PlacesBlank { has_hidden_places } => {
            let mut actions = vec![context_menu_item(ContextMenuAction::AddPlace, "Add Entry")];
            actions.push(context_menu_item_enabled(
                ContextMenuAction::ShowHiddenPlaces,
                "Show Hidden Places",
                *has_hidden_places,
            ));
            actions
        }
        ContextMenuTarget::PlaceSection { .. } => {
            vec![context_menu_item(
                ContextMenuAction::HidePlaceSection,
                "Hide Section",
            )]
        }
        ContextMenuTarget::Place {
            mounted,
            trash_place: true,
            trash_has_items,
            ..
        } => vec![
            context_menu_item_enabled(ContextMenuAction::Open, "Open", *mounted),
            context_menu_item_enabled(
                ContextMenuAction::OpenInNewPane,
                "Open in New Pane",
                *mounted,
            ),
            context_menu_item_enabled(
                ContextMenuAction::OpenInNewWindow,
                "Open in New Window",
                *mounted,
            ),
            context_menu_item_enabled(
                ContextMenuAction::EmptyTrash,
                "Empty Trash",
                *trash_has_items,
            ),
            context_menu_item(ContextMenuAction::HidePlace, "Hide"),
            context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )),
        ],
        ContextMenuTarget::Place {
            mounted,
            device,
            device_ejectable,
            device_can_power_off,
            editable,
            removable,
            ..
        } => {
            let mut actions = vec![
                context_menu_item_enabled(ContextMenuAction::Open, "Open", *mounted),
                context_menu_item_enabled(
                    ContextMenuAction::OpenInNewPane,
                    "Open in New Pane",
                    *mounted,
                ),
                context_menu_item_enabled(
                    ContextMenuAction::OpenInNewWindow,
                    "Open in New Window",
                    *mounted,
                ),
            ];
            if *device {
                let mut device_actions = Vec::new();
                if *mounted {
                    device_actions.push(context_menu_item(
                        ContextMenuAction::UnmountDevice,
                        "Unmount",
                    ));
                } else {
                    device_actions.push(context_menu_item(ContextMenuAction::MountDevice, "Mount"));
                }
                if *device_ejectable {
                    device_actions.push(context_menu_item(ContextMenuAction::EjectDevice, "Eject"));
                }
                if *device_can_power_off {
                    device_actions.push(context_menu_item(
                        ContextMenuAction::SafelyRemoveDevice,
                        "Safely Remove",
                    ));
                }
                if !device_actions.is_empty() {
                    actions.extend(context_menu_group_items(device_actions));
                }
            }
            actions.extend([
                context_menu_item_enabled(ContextMenuAction::EditPlace, "Edit Entry", *editable),
                context_menu_item_enabled(
                    ContextMenuAction::RemovePlace,
                    "Remove Entry",
                    *removable,
                ),
                context_menu_item(ContextMenuAction::HidePlace, "Hide"),
                context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Properties,
                    "Properties",
                )),
            ]);
            actions
        }
        ContextMenuTarget::Item {
            trash_view: true,
            trash_can_restore,
            ..
        } => vec![
            context_menu_item_enabled(
                ContextMenuAction::RestoreFromTrash,
                "Restore to Former Location",
                *trash_can_restore,
            ),
            context_menu_item(ContextMenuAction::Copy, "Copy"),
            context_menu_item(ContextMenuAction::DeletePermanently, "Delete Permanently"),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )),
        ],
        ContextMenuTarget::Item {
            selection_count,
            service_actions,
            ..
        } if *selection_count > 1 => {
            let mut actions = vec![
                context_menu_item(ContextMenuAction::Cut, "Cut"),
                context_menu_item(ContextMenuAction::Copy, "Copy"),
            ];
            let service_root_actions =
                context_menu_group_items(service_menu_root_actions(service_actions));
            let has_service_root_actions = !service_root_actions.is_empty();
            actions.extend(service_root_actions);
            if service_menu_has_more_actions(service_actions) {
                let more_actions = context_menu_submenu_item(
                    ContextMenuAction::ServiceMenuSubmenu,
                    "More Actions",
                    ContextMenuSubmenu::ServiceMenu,
                );
                actions.push(if has_service_root_actions {
                    more_actions
                } else {
                    context_menu_separator_before(more_actions)
                });
            }
            if should_offer_compress_fallback(service_actions) {
                actions.extend(context_menu_group_items(vec![context_menu_item(
                    ContextMenuAction::CompressWithArk,
                    "Compress...",
                )]));
            }
            actions.push(context_menu_separator_before(context_menu_item(
                ContextMenuAction::Trash,
                "Move to Trash",
            )));
            actions.push(context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )));
            actions
        }
        ContextMenuTarget::Item {
            path,
            is_dir,
            mime_type,
            service_actions,
            open_with_apps,
            ..
        } => {
            let mut actions = if *is_dir {
                vec![context_menu_item(ContextMenuAction::Open, "Open")]
            } else {
                vec![context_menu_submenu_item(
                    ContextMenuAction::OpenWithSubmenu,
                    "Open With",
                    ContextMenuSubmenu::OpenWith,
                )]
            };
            if *is_dir {
                actions.push(context_menu_item(
                    ContextMenuAction::OpenInNewPane,
                    "Open in New Pane",
                ));
                actions.push(context_menu_item(
                    ContextMenuAction::OpenInNewWindow,
                    "Open in New Window",
                ));
                if !open_with_apps.is_empty() {
                    actions.push(context_menu_submenu_item(
                        ContextMenuAction::OpenWithSubmenu,
                        "Open With",
                        ContextMenuSubmenu::OpenWith,
                    ));
                }
                actions.push(context_menu_submenu_item(
                    ContextMenuAction::CreateNewSubmenu,
                    "Create New",
                    ContextMenuSubmenu::CreateNew,
                ));
            }
            actions.extend([
                context_menu_separator_before(context_menu_item(ContextMenuAction::Cut, "Cut")),
                context_menu_item(ContextMenuAction::Copy, "Copy"),
                context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            ]);
            if *is_dir {
                actions.push(context_menu_item_enabled(
                    ContextMenuAction::Paste,
                    "Paste",
                    clipboard_available,
                ));
            }
            let service_root_actions =
                context_menu_group_items(service_menu_root_actions(service_actions));
            let has_service_root_actions = !service_root_actions.is_empty();
            actions.extend(service_root_actions);
            if service_menu_has_more_actions(service_actions) {
                let more_actions = context_menu_submenu_item(
                    ContextMenuAction::ServiceMenuSubmenu,
                    "More Actions",
                    ContextMenuSubmenu::ServiceMenu,
                );
                actions.push(if has_service_root_actions {
                    more_actions
                } else {
                    context_menu_separator_before(more_actions)
                });
            }
            if should_offer_compress_fallback(service_actions)
                && (*is_dir || !is_archive_mime_or_path(mime_type.as_deref(), path))
            {
                actions.extend(context_menu_group_items(vec![context_menu_item(
                    ContextMenuAction::CompressWithArk,
                    "Compress...",
                )]));
            }
            if !*is_dir
                && is_archive_mime_or_path(mime_type.as_deref(), path)
                && should_offer_extract_fallback(service_actions)
            {
                actions.extend(context_menu_group_items(vec![
                    context_menu_item(ContextMenuAction::ExtractHereWithArk, "Extract Here"),
                    context_menu_item(ContextMenuAction::ExtractToWithArk, "Extract To..."),
                ]));
            }
            actions.extend([
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Rename,
                    "Rename",
                )),
                context_menu_item(ContextMenuAction::Trash, "Move to Trash"),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Properties,
                    "Properties",
                )),
            ]);
            actions
        }
    }
}

pub(crate) fn context_submenu_actions(
    submenu: ContextMenuSubmenu,
    target: &ContextMenuTarget,
) -> Vec<ContextMenuItem> {
    match submenu {
        ContextMenuSubmenu::CreateNew => match target {
            ContextMenuTarget::Blank {
                trash_view: false, ..
            }
            | ContextMenuTarget::Item {
                is_dir: true,
                trash_view: false,
                ..
            } => vec![
                context_menu_item(ContextMenuAction::CreateFolder, "Folder"),
                context_menu_item(ContextMenuAction::CreateFile, "Text File"),
            ],
            _ => Vec::new(),
        },
        ContextMenuSubmenu::OpenWith => match target {
            ContextMenuTarget::Item { open_with_apps, .. } => {
                open_with_menu_actions(open_with_apps)
            }
            _ => Vec::new(),
        },
        ContextMenuSubmenu::ServiceMenu => match target {
            ContextMenuTarget::Blank {
                service_actions, ..
            }
            | ContextMenuTarget::Item {
                service_actions, ..
            } => service_menu_more_actions(service_actions),
            _ => Vec::new(),
        },
        ContextMenuSubmenu::ServiceMenuGroup(group_index) => match target {
            ContextMenuTarget::Blank {
                service_actions, ..
            }
            | ContextMenuTarget::Item {
                service_actions, ..
            } => service_menu_group_actions(service_actions, group_index),
            _ => Vec::new(),
        },
        ContextMenuSubmenu::SortBy => vec![
            context_menu_item(ContextMenuAction::SortByName, "Name"),
            context_menu_item(ContextMenuAction::SortByModified, "Modified"),
            context_menu_item(ContextMenuAction::SortBySize, "Size"),
            context_menu_item(ContextMenuAction::SortAscending, "Ascending"),
            context_menu_item(ContextMenuAction::SortDescending, "Descending"),
            context_menu_item(ContextMenuAction::SortFoldersFirst, "Folders First"),
            context_menu_item(ContextMenuAction::SortHiddenLast, "Hidden Files Last"),
        ],
        ContextMenuSubmenu::TrashSortBy => vec![
            context_menu_item(ContextMenuAction::SortByName, "Name"),
            context_menu_item(ContextMenuAction::SortByOriginalPath, "Original Path"),
            context_menu_item(ContextMenuAction::SortByDeletionTime, "Deletion Time"),
            context_menu_item(ContextMenuAction::SortAscending, "Ascending"),
            context_menu_item(ContextMenuAction::SortDescending, "Descending"),
            context_menu_item(ContextMenuAction::SortFoldersFirst, "Folders First"),
            context_menu_item(ContextMenuAction::SortHiddenLast, "Hidden Files Last"),
        ],
        ContextMenuSubmenu::ViewMode => vec![
            context_menu_item(ContextMenuAction::ViewCompact, "Compact"),
            disabled_context_menu_item(ContextMenuAction::ViewIcons, "Icons"),
            disabled_context_menu_item(ContextMenuAction::ViewDetails, "Details"),
        ],
    }
}

fn context_menu_item(action: ContextMenuAction, label: impl Into<String>) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled: true,
        submenu: None,
        icon,
        separator_before: false,
    }
}

fn context_menu_item_enabled(
    action: ContextMenuAction,
    label: impl Into<String>,
    enabled: bool,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled,
        submenu: None,
        icon,
        separator_before: false,
    }
}

fn context_menu_submenu_item(
    action: ContextMenuAction,
    label: impl Into<String>,
    submenu: ContextMenuSubmenu,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled: true,
        submenu: Some(submenu),
        icon,
        separator_before: false,
    }
}

fn disabled_context_menu_item(
    action: ContextMenuAction,
    label: impl Into<String>,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled: false,
        submenu: None,
        icon,
        separator_before: false,
    }
}

fn context_menu_separator_before(mut item: ContextMenuItem) -> ContextMenuItem {
    item.separator_before = true;
    item
}

fn context_menu_group_items(mut items: Vec<ContextMenuItem>) -> Vec<ContextMenuItem> {
    if let Some(first) = items.first_mut() {
        first.separator_before = true;
    }
    items
}

fn context_menu_icon_for_action(action: &ContextMenuAction) -> Option<ContextMenuIcon> {
    match action {
        ContextMenuAction::Open | ContextMenuAction::OpenInNewPane => Some(ContextMenuIcon::Open),
        ContextMenuAction::OpenInNewWindow => Some(ContextMenuIcon::NewWindow),
        ContextMenuAction::OpenWithSubmenu => Some(ContextMenuIcon::OpenWith),
        ContextMenuAction::OpenWithApplication { .. } | ContextMenuAction::OtherApplication => {
            Some(ContextMenuIcon::Application)
        }
        ContextMenuAction::CreateNewSubmenu => Some(ContextMenuIcon::CreateNew),
        ContextMenuAction::ServiceMenuSubmenu
        | ContextMenuAction::ServiceMenuGroupSubmenu { .. }
        | ContextMenuAction::RunServiceMenuAction { .. } => Some(ContextMenuIcon::Service),
        ContextMenuAction::CompressWithArk
        | ContextMenuAction::ExtractHereWithArk
        | ContextMenuAction::ExtractToWithArk => Some(ContextMenuIcon::Archive),
        ContextMenuAction::MountDevice => Some(ContextMenuIcon::Named("media-mount".to_string())),
        ContextMenuAction::UnmountDevice => Some(ContextMenuIcon::Named("media-eject".to_string())),
        ContextMenuAction::EjectDevice => Some(ContextMenuIcon::Named("media-eject".to_string())),
        ContextMenuAction::SafelyRemoveDevice => {
            Some(ContextMenuIcon::Named("drive-removable-media".to_string()))
        }
        ContextMenuAction::AddPlace => Some(ContextMenuIcon::Place),
        ContextMenuAction::EditPlace => Some(ContextMenuIcon::Edit),
        ContextMenuAction::RemovePlace => Some(ContextMenuIcon::Remove),
        ContextMenuAction::HidePlace
        | ContextMenuAction::HidePlaceSection
        | ContextMenuAction::ShowHiddenPlaces => Some(ContextMenuIcon::Hide),
        ContextMenuAction::SortBySubmenu
        | ContextMenuAction::SortByName
        | ContextMenuAction::SortByModified
        | ContextMenuAction::SortBySize
        | ContextMenuAction::SortByOriginalPath
        | ContextMenuAction::SortByDeletionTime
        | ContextMenuAction::SortAscending
        | ContextMenuAction::SortDescending
        | ContextMenuAction::SortFoldersFirst
        | ContextMenuAction::SortHiddenLast => Some(ContextMenuIcon::Sort),
        ContextMenuAction::ViewModeSubmenu
        | ContextMenuAction::ViewCompact
        | ContextMenuAction::ViewIcons
        | ContextMenuAction::ViewDetails => Some(ContextMenuIcon::View),
        ContextMenuAction::Rename => Some(ContextMenuIcon::Rename),
        ContextMenuAction::Copy => Some(ContextMenuIcon::Copy),
        ContextMenuAction::CopyLocation => Some(ContextMenuIcon::Location),
        ContextMenuAction::Cut => Some(ContextMenuIcon::Cut),
        ContextMenuAction::Trash | ContextMenuAction::EmptyTrash => Some(ContextMenuIcon::Trash),
        ContextMenuAction::RestoreFromTrash => Some(ContextMenuIcon::Restore),
        ContextMenuAction::DeletePermanently => Some(ContextMenuIcon::Delete),
        ContextMenuAction::Properties => Some(ContextMenuIcon::Properties),
        ContextMenuAction::CreateFolder => Some(ContextMenuIcon::NewFolder),
        ContextMenuAction::CreateFile => Some(ContextMenuIcon::NewFile),
        ContextMenuAction::Paste => Some(ContextMenuIcon::Paste),
        ContextMenuAction::SelectAll => Some(ContextMenuIcon::Select),
        ContextMenuAction::Refresh => Some(ContextMenuIcon::Refresh),
    }
}

fn open_with_menu_actions(apps: &[MimeApplication]) -> Vec<ContextMenuItem> {
    let apps = dedup_open_with_apps(apps);
    let mut actions = if apps.is_empty() {
        vec![disabled_context_menu_item(
            ContextMenuAction::OpenWithSubmenu,
            "No Applications",
        )]
    } else {
        apps.into_iter()
            .map(|app| {
                let mut item = context_menu_item(
                    ContextMenuAction::OpenWithApplication {
                        desktop_id: app.id.clone(),
                    },
                    app.name.clone(),
                );
                if let Some(icon) = app.icon.as_ref().filter(|icon| !icon.trim().is_empty()) {
                    item.icon = Some(ContextMenuIcon::Named(icon.trim().to_string()));
                }
                item
            })
            .collect::<Vec<_>>()
    };
    actions.push(context_menu_item(
        ContextMenuAction::OtherApplication,
        "Other Application...",
    ));
    actions
}

fn dedup_open_with_apps(apps: &[MimeApplication]) -> Vec<&MimeApplication> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let mut deduped = Vec::new();
    for app in apps {
        let id = app.id.to_ascii_lowercase();
        let name = app.name.trim().to_ascii_lowercase();
        if seen_ids.insert(id) && seen_names.insert(name) {
            deduped.push(app);
        }
    }
    deduped
}

fn service_menu_root_actions(actions: &[ServiceMenuAction]) -> Vec<ContextMenuItem> {
    actions
        .iter()
        .filter(|action| service_menu_action_promoted(action, actions.len()))
        .map(service_menu_action_item)
        .collect()
}

fn service_menu_has_more_actions(actions: &[ServiceMenuAction]) -> bool {
    actions
        .iter()
        .any(|action| !service_menu_action_promoted(action, actions.len()))
}

fn service_menu_more_actions(actions: &[ServiceMenuAction]) -> Vec<ContextMenuItem> {
    if actions.is_empty() {
        return vec![disabled_context_menu_item(
            ContextMenuAction::ServiceMenuSubmenu,
            "No Actions",
        )];
    }
    let more_actions = service_menu_more_action_refs(actions);
    if more_actions.is_empty() {
        return vec![disabled_context_menu_item(
            ContextMenuAction::ServiceMenuSubmenu,
            "No More Actions",
        )];
    }

    let (ungrouped, groups) = service_menu_partition_grouped_actions(more_actions);
    let mut items = ungrouped
        .into_iter()
        .map(service_menu_action_item)
        .collect::<Vec<_>>();
    for (group_index, (label, _)) in groups.iter().enumerate() {
        let mut group_item = context_menu_submenu_item(
            ContextMenuAction::ServiceMenuGroupSubmenu { group_index },
            label.clone(),
            ContextMenuSubmenu::ServiceMenuGroup(group_index),
        );
        group_item.separator_before = !items.is_empty() && group_index == 0;
        items.push(group_item);
    }
    items
}

fn service_menu_group_actions(
    actions: &[ServiceMenuAction],
    group_index: usize,
) -> Vec<ContextMenuItem> {
    let more_actions = service_menu_more_action_refs(actions);
    let (_, groups) = service_menu_partition_grouped_actions(more_actions);
    let Some((_, group_actions)) = groups.into_iter().nth(group_index) else {
        return vec![disabled_context_menu_item(
            ContextMenuAction::ServiceMenuGroupSubmenu { group_index },
            "No Actions",
        )];
    };
    group_actions
        .into_iter()
        .map(service_menu_action_item)
        .collect()
}

fn service_menu_more_action_refs(actions: &[ServiceMenuAction]) -> Vec<&ServiceMenuAction> {
    actions
        .iter()
        .filter(|action| !service_menu_action_promoted(action, actions.len()))
        .collect()
}

fn service_menu_partition_grouped_actions(
    actions: Vec<&ServiceMenuAction>,
) -> (
    Vec<&ServiceMenuAction>,
    Vec<(String, Vec<&ServiceMenuAction>)>,
) {
    let mut grouped: Vec<(String, Vec<&ServiceMenuAction>)> = Vec::new();
    let ungrouped = actions
        .iter()
        .copied()
        .filter(|action| action.submenu.is_none())
        .collect::<Vec<_>>();

    for action in actions
        .into_iter()
        .filter(|action| action.submenu.is_some())
    {
        let group = action.submenu.as_deref().unwrap_or_default().to_string();
        if let Some((_, group_actions)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(&group))
        {
            group_actions.push(action);
        } else {
            grouped.push((group, vec![action]));
        }
    }

    (ungrouped, grouped)
}

fn service_menu_action_promoted(action: &ServiceMenuAction, action_count: usize) -> bool {
    if action.priority == ServiceMenuPriority::TopLevel {
        return true;
    }
    if action.submenu.is_some() {
        return false;
    }
    if action_count <= 4 {
        return true;
    }
    let label = action.label.to_ascii_lowercase();
    [
        "compress", "extract", "archive", "terminal", "send to", "copy to", "move to",
    ]
    .iter()
    .any(|keyword| label.contains(keyword))
}

fn should_offer_compress_fallback(actions: &[ServiceMenuAction]) -> bool {
    !actions.iter().any(service_menu_action_is_compress)
}

fn service_menu_action_is_compress(action: &ServiceMenuAction) -> bool {
    let label = action.label.to_ascii_lowercase();
    let id = action.id.to_ascii_lowercase();
    label.contains("compress")
        || id.contains("compress")
        || label.contains("create archive")
        || id.contains("create-archive")
        || id.contains("create_archive")
}

fn should_offer_extract_fallback(actions: &[ServiceMenuAction]) -> bool {
    !actions.iter().any(service_menu_action_is_extract)
}

fn service_menu_action_is_extract(action: &ServiceMenuAction) -> bool {
    let label = action.label.to_ascii_lowercase();
    let id = action.id.to_ascii_lowercase();
    label.contains("extract")
        || id.contains("extract")
        || label.contains("unarchive")
        || id.contains("unarchive")
}

fn service_menu_action_item(action: &ServiceMenuAction) -> ContextMenuItem {
    let mut item = context_menu_item(
        ContextMenuAction::RunServiceMenuAction {
            action_id: action.id.clone(),
        },
        action.label.clone(),
    );
    if let Some(icon) = action.icon.as_ref().filter(|icon| !icon.trim().is_empty()) {
        item.icon = Some(ContextMenuIcon::Named(icon.trim().to_string()));
    }
    item
}

pub(crate) fn context_menu_overlay_layout(
    position: ViewPoint,
    action_count: usize,
    active_submenu: Option<ContextMenuOpenSubmenu>,
    submenu_count: usize,
    nested_submenu_count: usize,
    viewport_width: f32,
    viewport_height: f32,
) -> ContextMenuOverlayLayout {
    let root_width = context_menu_width_for_viewport(viewport_width);
    let root_height = context_menu_height(action_count);
    let root_max_height = context_menu_max_height_for_viewport(viewport_height).min(root_height);
    let root = ContextMenuOverlayRect {
        x: popup_menu_axis(position.x, root_width, viewport_width),
        y: popup_menu_axis(position.y, root_max_height, viewport_height),
        width: root_width,
        max_height: root_max_height,
    };
    let submenu = active_submenu.map(|open| {
        cascading_menu_rect(
            root,
            open.parent_index,
            submenu_count,
            viewport_width,
            viewport_height,
        )
    });
    let nested_submenu = active_submenu
        .and_then(|open| open.nested)
        .zip(submenu)
        .map(|(nested, parent)| {
            cascading_menu_rect(
                parent,
                nested.parent_index,
                nested_submenu_count,
                viewport_width,
                viewport_height,
            )
        });

    ContextMenuOverlayLayout {
        root,
        submenu,
        nested_submenu,
    }
}

fn cascading_menu_rect(
    parent: ContextMenuOverlayRect,
    parent_index: usize,
    child_count: usize,
    viewport_width: f32,
    viewport_height: f32,
) -> ContextMenuOverlayRect {
    let width = context_menu_width_for_viewport(viewport_width);
    let height = context_menu_height(child_count);
    let max_height = context_menu_max_height_for_viewport(viewport_height).min(height);
    let right_x = parent.x + parent.width - 1.0;
    let left_x = parent.x - width + 1.0;
    let right_edge_limit = (viewport_width - CONTEXT_MENU_VIEWPORT_MARGIN).max(0.0);
    let x = if right_x + width <= right_edge_limit {
        right_x
    } else {
        left_x
    };
    let parent_y =
        parent.y + CONTEXT_MENU_VERTICAL_PADDING + parent_index as f32 * CONTEXT_MENU_ROW_HEIGHT;
    ContextMenuOverlayRect {
        x: clamp_menu_axis(x, width, viewport_width),
        y: clamp_menu_axis(parent_y, max_height, viewport_height),
        width,
        max_height,
    }
}

fn context_menu_height(row_count: usize) -> f32 {
    CONTEXT_MENU_VERTICAL_PADDING * 2.0 + row_count as f32 * CONTEXT_MENU_ROW_HEIGHT
}

fn context_menu_width_for_viewport(viewport_width: f32) -> f32 {
    (viewport_width - CONTEXT_MENU_VIEWPORT_MARGIN * 2.0)
        .max(1.0)
        .min(CONTEXT_MENU_WIDTH)
}

fn context_menu_max_height_for_viewport(viewport_height: f32) -> f32 {
    (viewport_height - CONTEXT_MENU_VIEWPORT_MARGIN * 2.0).max(1.0)
}

fn clamp_menu_axis(position: f32, size: f32, viewport_size: f32) -> f32 {
    let min = CONTEXT_MENU_VIEWPORT_MARGIN.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - CONTEXT_MENU_VIEWPORT_MARGIN).max(min);
    position.clamp(min, max)
}

fn popup_menu_axis(anchor: f32, size: f32, viewport_size: f32) -> f32 {
    let min = CONTEXT_MENU_VIEWPORT_MARGIN.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - CONTEXT_MENU_VIEWPORT_MARGIN).max(min);
    let forward = anchor.clamp(min, max);
    if anchor + size <= viewport_size - CONTEXT_MENU_VIEWPORT_MARGIN {
        return forward;
    }
    let flipped = anchor - size;
    if flipped >= min {
        return flipped.min(max);
    }
    forward
}
