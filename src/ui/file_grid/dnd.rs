use std::path::{Path, PathBuf};
use std::sync::Arc;

use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{
    Context, Div, ExternalPaths, IntoElement, MouseMoveEvent, ParentElement, Render, Stateful,
    Styled, WeakEntity, Window, div, px, rgb,
};

use crate::FikaApp;
use crate::ui::drag_drop::{
    DragPreviewLayout, FileTransferMode, ItemDragPayload, PathListDropTargetKind,
    PathListDropTargetUpdate, drag_preview_layout_for_cursor_offset, item_drop_reject_reason,
    refresh_active_drag_cursor_for_drop_menu, refresh_active_drag_cursor_for_transfer_mode,
    refresh_active_drag_cursor_not_allowed,
};
use crate::ui::icons::{FileIconSnapshot, cached_icon_or_fallback};
use crate::ui::places::PlaceDrag;

use super::{DetailsPaintSnapshot, ItemPaintSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemDrag {
    pub(super) pane_id: PaneId,
    pub(super) path: Arc<Path>,
    pub(super) name: Arc<str>,
    pub(super) icon: FileIconSnapshot,
    pub(super) selected: bool,
    pub(super) selection_count: usize,
}

impl ItemDrag {
    pub(crate) fn payload(&self) -> ItemDragPayload {
        ItemDragPayload {
            source_pane: self.pane_id,
            source_path: self.path.as_ref().to_path_buf(),
            source_selected: self.selected,
        }
    }
}

pub(super) fn item_drag_from_item_snapshot(pane_id: PaneId, item: &ItemPaintSnapshot) -> ItemDrag {
    let content = item.content.as_ref();
    ItemDrag {
        pane_id,
        path: content.drag_path.clone(),
        name: content.name.clone(),
        icon: content.icon.clone(),
        selected: item.visual.selected,
        selection_count: item.visual.selection_count,
    }
}

pub(super) fn item_drag_from_details_snapshot(
    pane_id: PaneId,
    item: &DetailsPaintSnapshot,
) -> ItemDrag {
    ItemDrag {
        pane_id,
        path: item.content.path.clone(),
        name: item.content.name.clone(),
        icon: item.content.icon.clone(),
        selected: item.visual.selected,
        selection_count: item.visual.selection_count,
    }
}

pub(super) struct DragPreview {
    source_pane: PaneId,
    app: WeakEntity<FikaApp>,
    icon: FileIconSnapshot,
    label: String,
    count: usize,
    layout: DragPreviewLayout,
}

const DRAG_PREVIEW_MIN_WIDTH: f32 = 220.0;
const DRAG_PREVIEW_MIN_HEIGHT: f32 = 36.0;

pub(super) fn item_drag_preview(
    drag: &ItemDrag,
    cursor_offset: gpui::Point<gpui::Pixels>,
    app: WeakEntity<FikaApp>,
) -> DragPreview {
    DragPreview {
        source_pane: drag.pane_id,
        app,
        icon: drag.icon.clone(),
        label: drag_preview_label(drag.name.as_ref(), drag.selected, drag.selection_count),
        count: drag.selection_count,
        layout: drag_preview_layout_for_cursor_offset(
            cursor_offset,
            DRAG_PREVIEW_MIN_WIDTH,
            DRAG_PREVIEW_MIN_HEIGHT + 6.0,
        ),
    }
}

pub(super) fn drag_preview_label(name: &str, selected: bool, selection_count: usize) -> String {
    if selected && selection_count > 1 {
        format!("{selection_count} items")
    } else {
        name.to_string()
    }
}

pub(super) fn install_item_drag_start_shell(
    shell: Stateful<Div>,
    drag_value: ItemDrag,
    app: WeakEntity<FikaApp>,
) -> Stateful<Div> {
    // GPUI still owns drag initiation; this shell is the remaining platform
    // boundary until custom elements can start drags directly.
    shell.on_drag(drag_value, move |drag, cursor_offset, _, cx| {
        let _ = app.update(cx, |this, _cx| {
            this.begin_item_drag(drag.payload());
            debug_dnd_log(|| {
                format!(
                    "item-start pane={} path={} selected={} selection_count={}",
                    drag.pane_id.0,
                    drag.path.display(),
                    drag.selected,
                    drag.selection_count
                )
            });
        });
        cx.new(|_| item_drag_preview(drag, cursor_offset, app.clone()))
    })
}

pub(super) fn install_active_item_drag_mouse_tracker(
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    window: &mut Window,
) {
    let move_app = app.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if !phase.capture() {
            return;
        }
        update_active_item_drag_drop_target_from_position(
            pane_id,
            event.position,
            &move_app,
            window,
            cx,
            "window",
        );
    });
}

pub(super) fn install_file_grid_path_drop_shell(
    shell: Stateful<Div>,
    pane_id: PaneId,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    shell
        .on_drag_move::<ItemDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                handle_file_grid_item_drag_move(this, pane_id, event, window, cx);
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                handle_file_grid_external_drag_move(this, pane_id, event, window, cx);
            },
        ))
        .on_drag_move::<PlaceDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                handle_file_grid_place_drag_move(this, pane_id, event, window, cx);
            },
        ))
        .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
            handle_file_grid_item_drop(this, pane_id, drag, window, cx);
        }))
        .on_drop::<ExternalPaths>(cx.listener(
            move |this, external_paths: &ExternalPaths, window, cx| {
                handle_file_grid_external_drop(this, pane_id, external_paths, window, cx);
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, window, cx| {
            handle_file_grid_place_drop(this, pane_id, drag, window, cx);
        }))
}

pub(super) fn install_directory_drop_target_shell(
    shell: Stateful<Div>,
    pane_id: PaneId,
    target_dir: Arc<Path>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let item_target_dir = target_dir.clone();
    let external_target_dir = target_dir.clone();
    let place_target_dir = target_dir;
    shell
        .on_drag_move::<ItemDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                let source_paths = this.item_drag_source_paths(&event.drag(cx).payload());
                handle_file_grid_directory_path_list_drag_move(
                    this,
                    pane_id,
                    contains,
                    &source_paths,
                    item_target_dir.as_ref(),
                    window,
                    cx,
                );
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                let source_paths = this.external_drag_source_paths(event.drag(cx).paths());
                handle_file_grid_directory_path_list_drag_move(
                    this,
                    pane_id,
                    contains,
                    &source_paths,
                    external_target_dir.as_ref(),
                    window,
                    cx,
                );
            },
        ))
        .on_drag_move::<PlaceDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                let source_path = event.drag(cx).path();
                let source_paths = std::slice::from_ref(&source_path);
                handle_file_grid_directory_path_list_drag_move(
                    this,
                    pane_id,
                    contains,
                    source_paths,
                    place_target_dir.as_ref(),
                    window,
                    cx,
                );
            },
        ))
}

impl Render for DragPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // GPUI may not dispatch pane-level drag-move callbacks for same-window
        // item self-drags after the drag starts, but it does repaint the drag
        // preview as the pointer moves. Use that stable repaint path to keep
        // retained pane hit testing and directory hover state current.
        update_active_item_drag_drop_target_from_position(
            self.source_pane,
            window.mouse_position(),
            &self.app,
            window,
            cx,
            "preview",
        );

        let left = self.layout.content_origin_x;
        let top = self.layout.content_origin_y;
        let icon = self.icon.clone();
        let show_count = self.count > 1;
        let count = self.count;
        div()
            .relative()
            .w(px(self.layout.surface_width))
            .h(px(self.layout.surface_height))
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .h(px(DRAG_PREVIEW_MIN_HEIGHT))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x94a3b8))
                    .bg(rgb(0xffffff))
                    .shadow_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(0x1f2937))
                    .child(
                        div()
                            .relative()
                            .w(px(26.0))
                            .h(px(26.0))
                            .rounded_sm()
                            .overflow_hidden()
                            .child(item_drag_icon_or_fallback(icon))
                            .when(show_count, |icon| {
                                icon.child(
                                    div()
                                        .absolute()
                                        .right(px(-1.0))
                                        .bottom(px(-1.0))
                                        .min_w(px(14.0))
                                        .h(px(14.0))
                                        .px(px(3.0))
                                        .rounded_full()
                                        .bg(rgb(0xd97706))
                                        .text_xs()
                                        .text_color(rgb(0xffffff))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(count.to_string()),
                                )
                            }),
                    )
                    .child(div().max_w(px(170.0)).truncate().child(self.label.clone())),
            )
    }
}

fn item_drag_icon_or_fallback(icon: FileIconSnapshot) -> gpui::AnyElement {
    let marker = icon.fallback_marker.clone();
    let fg = icon.fallback_fg;
    let bg = icon.fallback_bg;
    cached_icon_or_fallback(&icon, move || {
        div()
            .size_full()
            .rounded_sm()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(fg))
            .bg(rgb(bg))
            .child(marker.as_ref().to_string())
            .into_any_element()
    })
}

fn debug_dnd_log(message: impl FnOnce() -> String) {
    if crate::dnd_debug_enabled() {
        eprintln!("[fika dnd] {}", message());
    }
}

fn debug_paths(paths: &[PathBuf]) -> String {
    const MAX_PATHS: usize = 3;
    let mut rendered = paths
        .iter()
        .take(MAX_PATHS)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if paths.len() > MAX_PATHS {
        rendered.push(format!("+{} more", paths.len() - MAX_PATHS));
    }
    rendered.join(",")
}

fn update_active_item_drag_drop_target_from_position(
    source_pane: PaneId,
    position: gpui::Point<gpui::Pixels>,
    app: &WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut gpui::App,
    via: &'static str,
) -> bool {
    let Some((target_pane, update, source_paths)) = app
        .update(cx, |this, cx| {
            let Some((target_pane, update, source_paths)) = this
                .update_active_item_drag_drop_target_from_window_position(source_pane, position)
            else {
                return None;
            };
            if update.accepted() {
                this.refresh_drop_target_lease(cx);
            }
            if update.changed {
                cx.notify();
            }
            Some((target_pane, update, source_paths))
        })
        .unwrap_or(None)
    else {
        return false;
    };

    debug_dnd_log(|| {
        format!(
            "active-item-move via={} source_pane={} target_pane={} pos=({:.1},{:.1}) kind={:?} changed={} sources={}",
            via,
            source_pane.0,
            target_pane
                .map(|pane_id| pane_id.0.to_string())
                .unwrap_or_else(|| "none".to_string()),
            position.x.as_f32(),
            position.y.as_f32(),
            update.kind,
            update.changed,
            debug_paths(&source_paths)
        )
    });

    if target_pane.is_some() {
        match update.kind {
            Some(_) => set_active_drag_cursor_style(gpui::CursorStyle::ContextualMenu, window, cx),
            None => {
                set_active_drag_cursor_style(gpui::CursorStyle::OperationNotAllowed, window, cx)
            }
        }
    }
    if update.changed {
        window.refresh();
    }
    true
}

fn set_active_drag_cursor_style(style: gpui::CursorStyle, window: &mut Window, cx: &mut gpui::App) {
    if cx.active_drag_cursor_style() != Some(style) {
        cx.set_active_drag_cursor_style(style, window);
    }
}

fn handle_file_grid_item_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<ItemDrag>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let payload = event.drag(cx).payload();
    let source_paths = app.item_drag_source_paths(&payload);
    let update = handle_file_grid_path_list_drag_move(
        app,
        pane_id,
        contains,
        event.event.position,
        &source_paths,
        window,
        cx,
    );
    debug_dnd_log(|| {
        format!(
            "viewport-item-move pane={} contains={} pos=({:.1},{:.1}) kind={:?} changed={} sources={}",
            pane_id.0,
            contains,
            event.event.position.x.as_f32(),
            event.event.position.y.as_f32(),
            update.kind,
            update.changed,
            debug_paths(&source_paths)
        )
    });
}

fn handle_file_grid_external_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<ExternalPaths>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let source_paths = app.external_drag_source_paths(event.drag(cx).paths());
    let _ = handle_file_grid_path_list_drag_move(
        app,
        pane_id,
        contains,
        event.event.position,
        &source_paths,
        window,
        cx,
    );
}

fn handle_file_grid_path_list_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    contains: bool,
    position: gpui::Point<gpui::Pixels>,
    source_paths: &[PathBuf],
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> PathListDropTargetUpdate {
    let update = if contains {
        app.update_dragged_paths_drop_target_from_window_position(pane_id, position, source_paths)
    } else {
        PathListDropTargetUpdate {
            changed: app.clear_item_drop_target_for_pane(pane_id),
            kind: None,
        }
    };
    if contains {
        if update.accepted() {
            refresh_active_drag_cursor_for_drop_menu(window, cx);
            app.refresh_drop_target_lease(cx);
        } else {
            refresh_active_drag_cursor_not_allowed(window, cx);
        }
    }
    if update.changed {
        cx.notify();
    }
    if contains {
        cx.stop_propagation();
    }
    update
}

fn handle_file_grid_directory_path_list_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    contains: bool,
    source_paths: &[PathBuf],
    target_dir: &Path,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    if !contains {
        // The viewport-level retained hit test owns blank/leave transitions.
        // Per-directory shells only assert positive hits; clearing here can
        // race against the viewport setting a directory target when GPUI shell
        // bounds and retained item geometry differ.
        return;
    }

    let changed = app.set_dragged_paths_drop_target_for_directory(
        pane_id,
        source_paths,
        target_dir.to_path_buf(),
    );
    debug_dnd_log(|| {
        format!(
            "directory-shell-hit pane={} target={} changed={} sources={}",
            pane_id.0,
            target_dir.display(),
            changed,
            debug_paths(source_paths)
        )
    });
    if item_drop_reject_reason(source_paths, target_dir).is_none() {
        refresh_active_drag_cursor_for_drop_menu(window, cx);
        app.refresh_drop_target_lease(cx);
    } else {
        refresh_active_drag_cursor_not_allowed(window, cx);
    }
    if changed {
        cx.notify();
    }
    cx.stop_propagation();
}

fn handle_file_grid_place_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<PlaceDrag>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let source_path = event.drag(cx).path();
    let source_paths = std::slice::from_ref(&source_path);
    let update = if contains {
        app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            event.event.position,
            source_paths,
        )
    } else {
        PathListDropTargetUpdate {
            changed: app.clear_item_drop_target_for_pane(pane_id),
            kind: None,
        }
    };
    debug_dnd_log(|| {
        format!(
            "viewport-place-move pane={} contains={} pos=({:.1},{:.1}) kind={:?} changed={} source={}",
            pane_id.0,
            contains,
            event.event.position.x.as_f32(),
            event.event.position.y.as_f32(),
            update.kind,
            update.changed,
            source_path.display()
        )
    });
    if contains {
        match update.kind {
            Some(PathListDropTargetKind::Directory) => {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
                app.refresh_drop_target_lease(cx);
            }
            Some(PathListDropTargetKind::Pane) => {
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Move, window, cx);
                app.refresh_drop_target_lease(cx);
            }
            None => {
                refresh_active_drag_cursor_not_allowed(window, cx);
            }
        }
    }
    if update.changed {
        cx.notify();
    }
    if contains {
        cx.stop_propagation();
    }
}

fn handle_file_grid_item_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    drag: &ItemDrag,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    let payload = drag.payload();
    app.drop_item_drag_to_position_in_pane(pane_id, payload, position, cx);
    cx.stop_propagation();
    cx.notify();
}

fn handle_file_grid_external_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    external_paths: &ExternalPaths,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    app.drop_external_paths_to_position_in_pane(
        pane_id,
        external_paths.paths().to_vec(),
        position,
        cx,
    );
    cx.stop_propagation();
    cx.notify();
}

fn handle_file_grid_place_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    drag: &PlaceDrag,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    app.drop_place_drag_to_position_in_pane(pane_id, drag.path(), position, cx);
    cx.stop_propagation();
    cx.notify();
}
