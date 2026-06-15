use fika_core::UserPlace;

use super::super::PlaceEntry;

pub(crate) fn user_places(places: &[PlaceEntry]) -> Vec<UserPlace> {
    places
        .iter()
        .filter(|place| place.editable && place.removable)
        .map(|place| UserPlace::new(place.label.clone(), place.path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn user_places_exports_only_editable_removable_entries() {
        let places = vec![
            place("", "Home", "/home/yk", false),
            place("", "Alpha", "/home/yk/Alpha", true),
            place("Devices", "Root", "/", false),
        ];

        assert_eq!(
            user_places(&places),
            vec![UserPlace::new(
                "Alpha".to_string(),
                PathBuf::from("/home/yk/Alpha")
            )]
        );
    }

    fn place(group: &'static str, label: &str, path: &str, editable: bool) -> PlaceEntry {
        PlaceEntry {
            group,
            marker: "P",
            label: label.to_string(),
            path: PathBuf::from(path),
            device_id: None,
            device_mounted: true,
            editable,
            removable: editable,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }
}
