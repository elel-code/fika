mod autosmoke;
mod devices;
mod drag;
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

pub(crate) use autosmoke::{PlacesAutosmokeAction, PlacesAutosmokeScenario};
pub(crate) use devices::replace_removable_device_places;
pub(crate) use drag::PlaceDrag;
#[cfg(test)]
pub(crate) use model::{
    DEVICES_GROUP, NETWORK_GROUP, REMOVABLE_DEVICES_GROUP, active_place_index,
    build_places_with_devices, place_is_mounted,
};
pub(crate) use model::{PlaceEntry, build_places, default_place_label, read_live_device_snapshot};
pub(crate) use paint_slots::{PlacePaintSlotCache, PlacePaintSlotPerfLog};
pub(crate) use perf::{
    PlacesSnapshotPerfLog, emit_place_paint_slot_perf_log, emit_places_snapshot_perf_log,
    places_perf_enabled, places_section_count,
};
pub(crate) use projection::place_snapshots_for;
pub(crate) use sidebar::places_sidebar;
pub(crate) use sidebar::{
    PLACES_SIDEBAR_DEFAULT_WIDTH, PlacesSidebarResizeDrag, clamp_places_sidebar_width,
    places_panel_button, places_panel_icon_snapshot, places_sidebar_splitter,
    places_sidebar_width_from_drag,
};
pub(crate) use snapshot::{PlaceIcon, PlaceSnapshot};
pub(crate) use user::{
    MoveUserPlaceResult, add_user_place_from_dropped_paths, commit_user_place_draft,
    move_user_place_to_insert_index, primary_place_order, remove_user_place,
    user_place_insert_index, user_places,
};
pub(crate) use visibility::{hide_place, hide_place_section, show_hidden_places};
pub(crate) use visual::PlacesRowTextShapeCache;
