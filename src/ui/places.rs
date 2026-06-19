mod autosmoke;
mod devices;
mod drag;
mod event_layer;
mod icon_view;
mod interaction;
mod model;
mod paint_slots;
mod perf;
mod projection;
mod sidebar;
mod snapshot;
mod style;
mod user;
mod visibility;
mod visual;

pub(crate) use autosmoke::{
    PlacesAutosmokeScenario, PlacesLayoutAutosmokeState, start_places_autosmoke,
};
pub(crate) use drag::PlaceDrag;
#[cfg(test)]
pub(crate) use model::{
    DEVICES_GROUP, NETWORK_GROUP, REMOVABLE_DEVICES_GROUP, active_place_index,
    build_places_with_devices, place_is_mounted,
};
pub(crate) use model::{PlaceEntry, build_places, default_place_label, read_live_device_snapshot};
pub(crate) use paint_slots::PlacePaintSlotCache;
pub(crate) use sidebar::places_sidebar;
#[cfg(test)]
pub(crate) use sidebar::places_sidebar_width_from_drag;
pub(crate) use sidebar::{
    PLACES_SIDEBAR_DEFAULT_WIDTH, PlacesSidebarResizeDrag, clamp_places_sidebar_width,
    places_panel_button, places_panel_icon_snapshot, places_sidebar_splitter,
};
pub(crate) use snapshot::{PlaceIcon, PlaceSnapshot};
pub(crate) use user::{
    MoveUserPlaceResult, add_user_place_from_dropped_paths, commit_user_place_draft,
    move_user_place_to_insert_index, remove_user_place, user_place_insert_index,
};
pub(crate) use visual::PlacesRowTextShapeCache;
