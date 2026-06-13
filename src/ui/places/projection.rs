use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fika_core::file_ops;

use crate::ui::drag_drop::{
    PlaceDropTarget, place_drop_target_matches_insert, place_drop_target_mode_for_place,
};
use crate::ui::icons::FileIconCache;

use super::model::{
    REMOVABLE_DEVICES_GROUP, active_place_index, place_icon_for, place_icon_snapshot,
    place_is_mounted, place_is_network_root,
};
use super::{PlaceEntry, PlaceSnapshot};

pub(crate) fn place_snapshots_for(
    places: &[PlaceEntry],
    current_dir: Option<&Path>,
    hidden_place_sections: &BTreeSet<&'static str>,
    hidden_places: &BTreeSet<PathBuf>,
    place_drop_target: Option<&PlaceDropTarget>,
    trash_has_items: bool,
    file_icons: &mut FileIconCache,
) -> Vec<PlaceSnapshot> {
    let active_index = current_dir.and_then(|path| active_place_index(places, path));
    let last_index = places.len().saturating_sub(1);

    places
        .iter()
        .enumerate()
        .filter(|(_, place)| {
            !hidden_place_sections.contains(place.group) && !hidden_places.contains(&place.path)
        })
        .map(|(index, place)| {
            let trash_place = file_ops::is_trash_files_dir(&place.path);
            let network = place_is_network_root(place);
            let mounted = place_is_mounted(place);
            let device = place.group == REMOVABLE_DEVICES_GROUP;
            let place_icon = place_icon_for(place, trash_place);
            let icon = place_icon_snapshot(file_icons, place_icon);
            PlaceSnapshot {
                index,
                group: place.group,
                icon,
                label: place.label.clone(),
                path: place.path.clone(),
                mounted,
                device,
                network,
                device_ejectable: place.device_ejectable,
                device_can_power_off: place.device_can_power_off,
                active: active_index == Some(index),
                drop_target: (mounted && !network)
                    .then(|| place_drop_target_mode_for_place(place_drop_target, &place.path))
                    .flatten(),
                insert_before: place_drop_target_matches_insert(place_drop_target, index),
                insert_after: index == last_index
                    && place_drop_target_matches_insert(place_drop_target, places.len()),
                trash_place,
                trash_has_items: trash_place && trash_has_items,
                editable: place.editable,
                removable: place.removable,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::FileTransferMode;

    #[test]
    fn place_snapshots_apply_active_hidden_and_drop_projection() {
        let home = PathBuf::from("/tmp/fika-places-projection/home");
        let docs = home.join("Documents");
        let device = PathBuf::from("/run/media/fika/USB");
        let places = vec![
            place("", "Home", home.clone(), false),
            place("", "Documents", docs.clone(), true),
            PlaceEntry {
                group: REMOVABLE_DEVICES_GROUP,
                marker: "D",
                label: "USB".to_string(),
                path: device.clone(),
                editable: false,
                removable: false,
                device_ejectable: true,
                device_can_power_off: true,
            },
        ];
        let hidden_sections = BTreeSet::new();
        let hidden_places = BTreeSet::from([docs.clone()]);
        let drop_target = PlaceDropTarget::Place {
            path: device.clone(),
            mode: FileTransferMode::Copy,
        };
        let mut icons = FileIconCache::default();

        let snapshots = place_snapshots_for(
            &places,
            Some(&home.join("Downloads")),
            &hidden_sections,
            &hidden_places,
            Some(&drop_target),
            false,
            &mut icons,
        );

        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "USB"]
        );
        assert!(snapshots[0].active);
        assert_eq!(snapshots[1].drop_target, Some(FileTransferMode::Copy));
        assert!(snapshots[1].device);
        assert!(snapshots[1].device_ejectable);
        assert!(snapshots[1].device_can_power_off);
    }

    #[test]
    fn trash_snapshot_uses_app_owned_emptiness_state() {
        let trash = file_ops::trash_files_dir();
        let places = vec![place("", "Trash", trash, false)];
        let mut icons = FileIconCache::default();

        let empty = place_snapshots_for(
            &places,
            None,
            &BTreeSet::new(),
            &BTreeSet::new(),
            None,
            false,
            &mut icons,
        );
        let non_empty = place_snapshots_for(
            &places,
            None,
            &BTreeSet::new(),
            &BTreeSet::new(),
            None,
            true,
            &mut icons,
        );

        assert!(empty[0].trash_place);
        assert!(!empty[0].trash_has_items);
        assert!(non_empty[0].trash_place);
        assert!(non_empty[0].trash_has_items);
    }

    fn place(group: &'static str, label: &str, path: PathBuf, editable: bool) -> PlaceEntry {
        PlaceEntry {
            group,
            marker: "P",
            label: label.to_string(),
            path,
            editable,
            removable: editable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }
}
