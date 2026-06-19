mod dropped;
mod edit;
mod entry;
mod ordering;
mod persistence;
mod removal;

use crate::FikaApp;

pub(crate) use dropped::add_user_place_from_dropped_paths;
pub(crate) use edit::commit_user_place_draft;
pub(crate) use ordering::{
    MoveUserPlaceResult, move_user_place_to_insert_index, user_place_insert_index,
};
pub(crate) use removal::remove_user_place;

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
}
