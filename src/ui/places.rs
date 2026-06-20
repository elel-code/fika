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
    build_places_with_devices, default_place_label, place_is_mounted,
};
pub(crate) use model::{PlaceEntry, build_places, read_live_device_snapshot};
pub(crate) use paint_slots::PlacePaintSlotCache;
pub(crate) use sidebar::places_sidebar;
#[cfg(test)]
pub(crate) use sidebar::places_sidebar_width_from_drag;
pub(crate) use sidebar::{
    PLACES_SIDEBAR_DEFAULT_WIDTH, PlacesSidebarResizeDrag, clamp_places_sidebar_width,
    places_panel_button, places_panel_icon_snapshot, places_sidebar_splitter,
    places_theme_icon_cache_requests,
};
pub(crate) use snapshot::{PlaceIcon, PlaceSnapshot};
pub(crate) use visual::PlacesRowTextShapeCache;
