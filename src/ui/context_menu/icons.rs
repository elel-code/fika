use std::collections::HashMap;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Div, ParentElement, Styled, div, px, rgb};

use crate::ui::icons::{FileIconCache, FileIconSnapshot, cached_icon_or_fallback};

use super::{
    ContextMenuIcon, ContextMenuItem, ContextMenuState, context_menu_actions,
    context_submenu_actions,
};

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
        ContextMenuIcon::Link => {
            icon_candidates("context-link", &["insert-link", "emblem-symbolic-link"])
        }
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

pub(super) fn context_menu_icon_slot(
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

    container.child(cached_icon_or_fallback(&snapshot, move || {
        context_menu_fallback_icon(fallback.clone(), fallback_fg, fallback_bg)
    }))
}

fn context_menu_icon_fallback_snapshot(icon: ContextMenuIcon) -> FileIconSnapshot {
    let (marker, fg, bg) = context_menu_icon_style(&icon, true);
    FileIconSnapshot {
        icon_name: Arc::from(format!("{icon:?}")),
        path: None,
        fallback_marker: Arc::from(marker),
        fallback_fg: fg,
        fallback_bg: bg,
    }
}

fn context_menu_fallback_icon(marker: Arc<str>, fg: u32, bg: u32) -> gpui::AnyElement {
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
        .child(marker.as_ref().to_string())
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
        ContextMenuIcon::Link => ("L", 0x7c3aed, 0xf2edff),
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
