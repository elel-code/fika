mod dropped;
mod edit;
mod entry;
mod ordering;
mod persistence;
mod removal;

use std::path::{Path, PathBuf};

use fika_core::PaneId;

use crate::FikaApp;

pub(crate) use edit::commit_user_place_draft;
pub(crate) use ordering::{
    MoveUserPlaceResult, move_user_place_to_insert_index, user_place_insert_index,
};

impl FikaApp {
    pub(crate) fn user_places(&self) -> Vec<fika_core::UserPlace> {
        persistence::user_places(&self.places)
    }

    pub(crate) fn save_user_places(&self) -> Result<(), String> {
        fika_core::save_user_places(&self.user_places_path, &self.user_places())?;
        let place_order_path =
            fika_core::place_order_path_for_user_places_path(&self.user_places_path);
        fika_core::save_place_order(
            &place_order_path,
            &persistence::primary_place_order(&self.places),
        )
    }

    pub(crate) fn remove_place(&mut self, pane_id: PaneId, path: &Path) {
        let result = removal::remove_user_place(&mut self.places, path);
        let Some(removed_path) = result.removed_path() else {
            self.set_pane_status(pane_id, result.status_message());
            return;
        };
        if self
            .place_draft
            .as_ref()
            .and_then(|draft| draft.editing_path.as_deref())
            == Some(removed_path)
        {
            self.place_draft = None;
        }
        self.hidden_places.remove(removed_path);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, result.status_message());
    }

    pub(crate) fn insert_place_from_dropped_paths(
        &mut self,
        pane_id: PaneId,
        paths: Vec<PathBuf>,
        index: usize,
    ) {
        let result = dropped::add_user_place_from_dropped_paths(&mut self.places, &paths, index);
        let message = result.status_message();
        if !result.added() {
            self.set_pane_status(pane_id, message);
            return;
        }
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, message);
    }
}
