use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fika_core::{
    DeviceInfo, file_ops, home_dir, is_network_path, network_path_display_name, network_root_path,
};

use super::PlaceIcon;
use crate::ui::icons::{FileIconCache, FileIconSnapshot};

pub(crate) const NETWORK_GROUP: &str = "Network";
pub(crate) const DEVICES_GROUP: &str = "Devices";
pub(crate) const REMOVABLE_DEVICES_GROUP: &str = "Removable Devices";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceEntry {
    pub(crate) group: &'static str,
    pub(crate) marker: &'static str,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) device_id: Option<String>,
    pub(crate) device_mounted: bool,
    pub(crate) editable: bool,
    pub(crate) removable: bool,
    pub(crate) device_ejectable: bool,
    pub(crate) device_can_power_off: bool,
}

pub(crate) fn build_places(user_places_path: &Path) -> Vec<PlaceEntry> {
    build_places_with_devices(user_places_path, &[])
}

pub(crate) async fn read_live_device_snapshot() -> Vec<DeviceInfo> {
    fika_core::read_devices().await.unwrap_or_default()
}

pub(crate) fn build_places_with_devices(
    user_places_path: &Path,
    devices: &[DeviceInfo],
) -> Vec<PlaceEntry> {
    let home = home_dir();
    let mut places = Vec::new();
    push_place(&mut places, "", "H", "Home", home.clone());
    push_existing_place(&mut places, "", "Desk", "Desktop", home.join("Desktop"));
    push_existing_place(&mut places, "", "Doc", "Documents", home.join("Documents"));
    push_existing_place(&mut places, "", "Down", "Downloads", home.join("Downloads"));
    push_existing_place(&mut places, "", "Mus", "Music", home.join("Music"));
    push_existing_place(&mut places, "", "Pic", "Pictures", home.join("Pictures"));
    push_existing_place(&mut places, "", "Vid", "Videos", home.join("Videos"));
    push_place(&mut places, "", "Tr", "Trash", file_ops::trash_files_dir());
    let built_in_paths = places
        .iter()
        .map(|place| place.path.clone())
        .chain(std::iter::once(PathBuf::from("/")))
        .chain(std::iter::once(network_root_path()))
        .collect::<BTreeSet<_>>();
    let mut network_places = Vec::new();
    for place in fika_core::load_user_places(user_places_path).unwrap_or_default() {
        if !built_in_paths.contains(&place.path) {
            if is_network_path(&place.path) {
                network_places.push(place);
            } else {
                push_user_place(&mut places, place.label, place.path);
            }
        }
    }
    let place_order_path = fika_core::place_order_path_for_user_places_path(user_places_path);
    let place_order = fika_core::load_place_order(&place_order_path).unwrap_or_default();
    apply_primary_place_order(&mut places, &place_order);
    push_place(
        &mut places,
        NETWORK_GROUP,
        "Net",
        fika_core::NETWORK_ROOT_LABEL,
        network_root_path(),
    );
    for place in network_places {
        push_network_place(&mut places, place.label, place.path);
    }
    append_removable_device_places(&mut places, devices);
    push_place(&mut places, DEVICES_GROUP, "/", "Root", PathBuf::from("/"));
    places
}

fn apply_primary_place_order(places: &mut Vec<PlaceEntry>, order: &[PathBuf]) {
    if order.is_empty() {
        return;
    }

    let first_grouped = places
        .iter()
        .position(|place| !place.group.is_empty())
        .unwrap_or(places.len());
    let mut primary_places = places.drain(..first_grouped).collect::<Vec<_>>();
    let mut ordered_places = Vec::with_capacity(primary_places.len());

    for path in order {
        if let Some(index) = primary_places
            .iter()
            .position(|place| place.path.as_path() == path.as_path())
        {
            ordered_places.push(primary_places.remove(index));
        }
    }
    ordered_places.append(&mut primary_places);
    places.splice(0..0, ordered_places);
}

fn append_removable_device_places(places: &mut Vec<PlaceEntry>, devices: &[DeviceInfo]) {
    let existing_paths = places
        .iter()
        .map(|place| place.path.clone())
        .collect::<BTreeSet<_>>();
    let mut entries = removable_device_place_entries(devices, &existing_paths);
    places.append(&mut entries);
}

pub(crate) fn removable_device_place_entries(
    devices: &[DeviceInfo],
    existing_paths: &BTreeSet<PathBuf>,
) -> Vec<PlaceEntry> {
    let mut seen_paths = existing_paths.clone();
    let mut entries = devices
        .iter()
        .filter(|device| device.removable)
        .filter_map(|device| {
            let path = device
                .mount_point
                .clone()
                .unwrap_or_else(|| PathBuf::from(&device.id));
            if !seen_paths.insert(path.clone()) {
                return None;
            }
            let label = device
                .label
                .clone()
                .filter(|label| !label.trim().is_empty())
                .unwrap_or_else(|| default_place_label(&path));
            Some(PlaceEntry {
                group: REMOVABLE_DEVICES_GROUP,
                marker: "D",
                label,
                path,
                device_id: Some(device.id.clone()),
                device_mounted: device.mounted,
                editable: false,
                removable: false,
                device_ejectable: device.ejectable,
                device_can_power_off: device.can_power_off,
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.path.cmp(&right.path))
    });
    entries
}

fn push_existing_place(
    places: &mut Vec<PlaceEntry>,
    group: &'static str,
    marker: &'static str,
    label: &'static str,
    path: PathBuf,
) {
    if path.is_dir() {
        push_place(places, group, marker, label, path);
    }
}

fn push_place(
    places: &mut Vec<PlaceEntry>,
    group: &'static str,
    marker: &'static str,
    label: &'static str,
    path: PathBuf,
) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(PlaceEntry {
        group,
        marker,
        label: label.to_string(),
        path,
        device_id: None,
        device_mounted: true,
        editable: false,
        removable: false,
        device_ejectable: false,
        device_can_power_off: false,
    });
}

pub(crate) fn push_user_place(places: &mut Vec<PlaceEntry>, label: String, path: PathBuf) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(PlaceEntry {
        group: "",
        marker: "B",
        label,
        path,
        device_id: None,
        device_mounted: true,
        editable: true,
        removable: true,
        device_ejectable: false,
        device_can_power_off: false,
    });
}

pub(crate) fn push_network_place(places: &mut Vec<PlaceEntry>, label: String, path: PathBuf) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(PlaceEntry {
        group: NETWORK_GROUP,
        marker: "Net",
        label,
        path,
        device_id: None,
        device_mounted: true,
        editable: true,
        removable: true,
        device_ejectable: false,
        device_can_power_off: false,
    });
}

pub(crate) fn place_icon_for(place: &PlaceEntry, trash_place: bool) -> PlaceIcon {
    if trash_place {
        return PlaceIcon::Trash;
    }
    if place_is_network(place) {
        return PlaceIcon::Network;
    }
    if place.path == PathBuf::from("/") {
        return PlaceIcon::Root;
    }
    if place.group == REMOVABLE_DEVICES_GROUP {
        return PlaceIcon::Device;
    }
    match place.label.as_str() {
        "Home" => PlaceIcon::Home,
        "Desktop" => PlaceIcon::Desktop,
        "Documents" => PlaceIcon::Documents,
        "Downloads" => PlaceIcon::Downloads,
        "Music" => PlaceIcon::Music,
        "Pictures" => PlaceIcon::Pictures,
        "Videos" => PlaceIcon::Videos,
        _ if place.editable || place.removable => PlaceIcon::Bookmark,
        _ => PlaceIcon::Folder,
    }
}

pub(crate) fn place_icon_snapshot(cache: &mut FileIconCache, icon: PlaceIcon) -> FileIconSnapshot {
    let (name, candidates, marker, fg, bg) = match icon {
        PlaceIcon::Home => (
            "place-home",
            &["user-home", "go-home", "folder-home", "folder"][..],
            "H",
            0x1f4fbf,
            0xeaf1ff,
        ),
        PlaceIcon::Desktop => (
            "place-desktop",
            &["user-desktop", "computer", "video-display", "folder"][..],
            "D",
            0x4f46e5,
            0xeeedff,
        ),
        PlaceIcon::Documents => (
            "place-documents",
            &["folder-documents", "x-office-document", "folder"][..],
            "D",
            0x2563eb,
            0xeaf1ff,
        ),
        PlaceIcon::Downloads => (
            "place-downloads",
            &["folder-download", "folder-downloads", "folder"][..],
            "DL",
            0x047857,
            0xe7f8ef,
        ),
        PlaceIcon::Music => (
            "place-music",
            &["folder-music", "audio-x-generic", "folder"][..],
            "M",
            0x9f1239,
            0xffe8ef,
        ),
        PlaceIcon::Pictures => (
            "place-pictures",
            &["folder-pictures", "image-x-generic", "folder"][..],
            "P",
            0x7c3aed,
            0xf2edff,
        ),
        PlaceIcon::Videos => (
            "place-videos",
            &["folder-videos", "video-x-generic", "folder"][..],
            "V",
            0xb45309,
            0xfff3df,
        ),
        PlaceIcon::Trash => (
            "place-trash",
            &["user-trash", "user-trash-full", "trash-empty"][..],
            "T",
            0x374151,
            0xeef1f5,
        ),
        PlaceIcon::Root => (
            "place-root",
            &["drive-harddisk-root", "drive-harddisk", "computer"][..],
            "/",
            0x334155,
            0xe8eef7,
        ),
        PlaceIcon::Network => (
            "place-network",
            &[
                "folder-remote",
                "network-workgroup",
                "network-server",
                "folder",
            ][..],
            "N",
            0x0369a1,
            0xe0f2fe,
        ),
        PlaceIcon::Device => (
            "place-device",
            &[
                "drive-removable-media-usb",
                "drive-removable-media",
                "drive-harddisk-usb",
                "drive-harddisk",
            ][..],
            "D",
            0x0f766e,
            0xe6fffb,
        ),
        PlaceIcon::Bookmark => (
            "place-bookmark",
            &["folder-favorites", "bookmark-new", "folder"][..],
            "B",
            0x0f766e,
            0xe6fffb,
        ),
        PlaceIcon::Folder => (
            "place-folder",
            &["folder", "inode-directory"][..],
            "F",
            0x0f4c81,
            0xe7f1fb,
        ),
    };
    cache.named_icon(name, candidates, marker, fg, bg, 22.0)
}

pub(crate) fn default_place_label(path: &Path) -> String {
    if let Some(name) = network_path_display_name(path) {
        return name;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn active_place_index(places: &[PlaceEntry], current_dir: &Path) -> Option<usize> {
    places
        .iter()
        .enumerate()
        .filter(|(_, place)| place_is_mounted(place))
        .filter(|(_, place)| current_dir == place.path || current_dir.starts_with(&place.path))
        .max_by_key(|(_, place)| place.path.components().count())
        .map(|(index, _)| index)
}

pub(crate) fn place_is_mounted(place: &PlaceEntry) -> bool {
    place.group != REMOVABLE_DEVICES_GROUP || place.device_mounted
}

pub(crate) fn place_is_network(place: &PlaceEntry) -> bool {
    is_network_path(&place.path)
}
