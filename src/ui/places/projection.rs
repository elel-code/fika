use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fika_core::file_ops;

use crate::ui::drag_drop::{PlaceDropTarget, place_drop_target_matches_place};
use crate::ui::icons::FileIconCache;

use super::model::{
    REMOVABLE_DEVICES_GROUP, active_place_index, place_icon_for, place_icon_snapshot,
    place_is_mounted, place_is_network,
};
use super::{PlaceEntry, PlaceSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceInsertIndicatorProjection {
    Before(usize),
    After(usize),
}

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
    let insert_indicator = place_insert_indicator_projection(places, place_drop_target);

    places
        .iter()
        .enumerate()
        .filter(|(_, place)| {
            !hidden_place_sections.contains(place.group) && !hidden_places.contains(&place.path)
        })
        .map(|(index, place)| {
            let trash_place = file_ops::is_trash_files_dir(&place.path);
            let network = place_is_network(place);
            let mounted = place_is_mounted(place);
            let device = place.group == REMOVABLE_DEVICES_GROUP;
            let place_icon = place_icon_for(place, trash_place);
            let icon = place_icon_snapshot(file_icons, place_icon);
            let insert_before =
                insert_indicator == Some(PlaceInsertIndicatorProjection::Before(index));
            let insert_after =
                insert_indicator == Some(PlaceInsertIndicatorProjection::After(index));
            PlaceSnapshot {
                index,
                group: place.group,
                icon,
                label: place.label.clone(),
                path: place.path.clone(),
                device_id: place.device_id.clone(),
                mounted,
                device,
                network,
                device_ejectable: place.device_ejectable,
                device_can_power_off: place.device_can_power_off,
                active: active_index == Some(index),
                drop_target: mounted
                    && !network
                    && place_drop_target_matches_place(place_drop_target, &place.path),
                insert_before,
                insert_after,
                trash_place,
                trash_has_items: trash_place && trash_has_items,
                editable: place.editable,
                removable: place.removable,
            }
        })
        .collect()
}

fn place_insert_indicator_projection(
    places: &[PlaceEntry],
    place_drop_target: Option<&PlaceDropTarget>,
) -> Option<PlaceInsertIndicatorProjection> {
    let Some(PlaceDropTarget::Insert { index }) = place_drop_target else {
        return None;
    };
    let index = *index;
    if places.is_empty() {
        return None;
    }
    if index < places.len() && places[index].group.is_empty() {
        return Some(PlaceInsertIndicatorProjection::Before(index));
    }
    if index > 0 && index <= places.len() && places[index - 1].group.is_empty() {
        return Some(PlaceInsertIndicatorProjection::After(index - 1));
    }
    if index < places.len() {
        return Some(PlaceInsertIndicatorProjection::Before(index));
    }
    Some(PlaceInsertIndicatorProjection::After(places.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

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
                device_id: Some("gio:test:usb".to_string()),
                device_mounted: true,
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
        assert!(snapshots[1].drop_target);
        assert!(snapshots[1].device);
        assert!(snapshots[1].device_ejectable);
        assert!(snapshots[1].device_can_power_off);
    }

    #[test]
    fn place_snapshots_keep_insert_indicator_separate_from_row_drop_target() {
        let home = PathBuf::from("/tmp/fika-places-insert-home");
        let docs = home.join("Documents");
        let places = vec![
            place("", "Home", home.clone(), false),
            place("", "Documents", docs.clone(), true),
        ];
        let mut icons = FileIconCache::default();

        let snapshots = place_snapshots_for(
            &places,
            Some(&home),
            &BTreeSet::new(),
            &BTreeSet::new(),
            Some(&PlaceDropTarget::Insert { index: 1 }),
            false,
            &mut icons,
        );

        assert!(!snapshots[0].drop_target);
        assert!(snapshots[1].insert_before);
        assert!(!snapshots[1].drop_target);
    }

    #[test]
    fn place_insert_at_user_block_end_renders_after_last_user_place() {
        let home = PathBuf::from("/tmp/fika-places-insert-end-home");
        let docs = home.join("Documents");
        let network = fika_core::network_root_path();
        let places = vec![
            place("", "Home", home.clone(), false),
            place("", "Documents", docs.clone(), true),
            place("Network", "Network", network.clone(), false),
        ];
        let mut icons = FileIconCache::default();

        let snapshots = place_snapshots_for(
            &places,
            Some(&home),
            &BTreeSet::new(),
            &BTreeSet::new(),
            Some(&PlaceDropTarget::Insert { index: 2 }),
            false,
            &mut icons,
        );

        assert!(snapshots[1].insert_after);
        assert!(!snapshots[2].insert_before);
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
            device_id: None,
            device_mounted: true,
            editable,
            removable: editable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }
}
