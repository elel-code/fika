mod overlay;
mod state;

use fika_core::PaneId;

use crate::FikaApp;

pub(crate) use overlay::place_draft_overlay;
pub(crate) use state::{
    PlaceDraft, PlaceDraftField, PlaceDraftInputResult, apply_place_input_action,
    clear_place_draft_for_pane, set_place_draft_focus,
};

impl FikaApp {
    pub(crate) fn clear_place_draft_for_pane(&mut self, pane_id: PaneId) {
        clear_place_draft_for_pane(&mut self.place_draft, pane_id);
    }

    pub(crate) fn dismiss_place_draft(&mut self) {
        self.place_draft = None;
    }

    pub(crate) fn set_place_draft_focus(&mut self, field: PlaceDraftField) {
        set_place_draft_focus(&mut self.place_draft, field);
    }
}
