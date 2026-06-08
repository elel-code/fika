use crate::app::async_bridge::AsyncBridge;
use crate::app::context_service_menu;
use crate::app::item_view::ItemViewControllerAction;
use crate::app::state::AppState;
use crate::{
    AppWindow, clear_selection_for_slot, open_path_for_slot, select_path_for_slot,
    select_rect_for_slot,
};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) struct PaneController<'a> {
    ui: &'a AppWindow,
    state: &'a Rc<RefCell<AppState>>,
    bridge: &'a AsyncBridge,
}

impl<'a> PaneController<'a> {
    pub(crate) fn new(
        ui: &'a AppWindow,
        state: &'a Rc<RefCell<AppState>>,
        bridge: &'a AsyncBridge,
    ) -> Self {
        Self { ui, state, bridge }
    }

    pub(crate) fn apply_item_view_controller_action(
        &self,
        slot: i32,
        action: ItemViewControllerAction,
    ) {
        match action {
            ItemViewControllerAction::None => {}
            ItemViewControllerAction::ActivatePath { path } => {
                open_path_for_slot(self.ui, self.state, slot, path.as_str(), self.bridge);
            }
            ItemViewControllerAction::RequestContextMenu {
                entry,
                select_path,
                abs_x,
                abs_y,
            } => {
                if let Some(path) = select_path.as_deref() {
                    select_path_for_slot(self.ui, self.state, slot, path, false, false);
                }

                let service_menu_paths =
                    context_service_menu::item_paths(self.state, slot, entry.path.as_str());
                context_service_menu::refresh_actions_async(
                    self.ui,
                    self.state,
                    self.bridge,
                    slot,
                    service_menu_paths,
                );
                self.ui.invoke_route_pane_request_context_menu(
                    slot,
                    entry.path,
                    entry.name,
                    entry.size,
                    entry.modified,
                    entry.is_dir,
                    abs_x,
                    abs_y,
                );
            }
            ItemViewControllerAction::ClearSelection => {
                clear_selection_for_slot(self.ui, self.state, slot);
            }
            ItemViewControllerAction::SelectPath {
                path,
                toggle,
                range,
            } => select_path_for_slot(self.ui, self.state, slot, path.as_str(), toggle, range),
            ItemViewControllerAction::SelectRect { rect, toggle } => {
                select_rect_for_slot(self.ui, self.state, slot, rect, toggle);
            }
        }
    }
}
