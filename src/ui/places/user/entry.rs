use std::path::PathBuf;

use super::super::PlaceEntry;

pub(super) fn user_place_entry(label: String, path: PathBuf) -> PlaceEntry {
    PlaceEntry {
        group: "",
        marker: "B",
        label,
        path,
        editable: true,
        removable: true,
        device_ejectable: false,
        device_can_power_off: false,
    }
}
