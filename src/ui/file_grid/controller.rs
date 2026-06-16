use std::path::PathBuf;

use fika_core::PaneId;
use gpui::{Context, NavigationDirection};

use crate::FikaApp;

use super::FileGridMode;

pub(super) fn handle_pane_navigation_mouse_down(
    app: &mut FikaApp,
    pane_id: PaneId,
    direction: NavigationDirection,
) {
    app.panes.focus(pane_id);
    match direction {
        NavigationDirection::Back => app.go_back(pane_id),
        NavigationDirection::Forward => app.go_forward(pane_id),
    }
}

pub(super) fn handle_file_grid_wheel(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::ScrollWheelEvent,
    cx: &mut Context<FikaApp>,
) {
    if wheel_modifiers_request_zoom(event.modifiers) {
        app.finish_rubber_band(pane_id);
        app.zoom_pane_from_wheel(pane_id, event.delta);
        cx.stop_propagation();
        cx.notify();
    } else if app.scroll_pane_from_wheel(pane_id, event.delta) {
        cx.stop_propagation();
        cx.notify();
    }
}

fn wheel_modifiers_request_zoom(modifiers: gpui::Modifiers) -> bool {
    modifiers.control || modifiers.secondary()
}

pub(super) fn handle_item_mouse_down(
    app: &mut FikaApp,
    pane_id: PaneId,
    path: PathBuf,
    is_dir: bool,
    mode: FileGridMode,
    event: &gpui::MouseDownEvent,
    cx: &mut Context<FikaApp>,
) -> bool {
    app.dismiss_context_menu();
    app.panes.focus(pane_id);

    let extend = event.modifiers.shift;
    let toggle = event.modifiers.secondary();
    let double_click = event.click_count >= 2;
    let is_dir = app.item_path_is_directory(pane_id, &path, is_dir);
    match mode {
        FileGridMode::Manager => {
            if double_click && is_dir {
                app.open_directory_from_item(pane_id, path, true);
            } else if double_click {
                app.open_default_application_for_item(pane_id, path, cx);
            } else if !double_click {
                if extend {
                    app.select_range_to(pane_id, path);
                } else if toggle {
                    app.toggle_selection(pane_id, path);
                } else {
                    app.select_only(pane_id, path);
                }
            }
        }
        FileGridMode::Chooser {
            directories,
            multiple,
        } => {
            if double_click && is_dir {
                app.open_directory_from_item(pane_id, path, true);
            } else if directories {
                if !is_dir {
                    return true;
                }
                if !double_click {
                    if extend {
                        app.select_range_to(pane_id, path);
                    } else if toggle || multiple {
                        app.toggle_selection(pane_id, path);
                    } else {
                        app.select_only(pane_id, path);
                    }
                }
            } else if is_dir {
                if !double_click {
                    app.select_only(pane_id, path);
                }
            } else if double_click && !multiple {
                app.choose_path(path);
            } else if !double_click {
                if extend {
                    app.select_range_to(pane_id, path);
                } else if toggle || multiple {
                    app.toggle_selection(pane_id, path);
                } else {
                    app.select_only(pane_id, path);
                }
            }
        }
    }
    true
}

#[cfg(test)]
pub(super) fn item_mouse_down_opens_directory(
    is_dir: bool,
    _mode: FileGridMode,
    click_count: usize,
) -> bool {
    is_dir && click_count >= 2
}
