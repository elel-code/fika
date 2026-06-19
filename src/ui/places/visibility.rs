use std::collections::BTreeSet;
use std::path::PathBuf;

use fika_core::PaneId;

use crate::FikaApp;

use super::PlaceEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HidePlaceResult {
    Hidden { label: String },
    CannotHide,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HidePlaceSectionResult {
    Hidden { group: &'static str },
    CannotHide,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShowHiddenPlacesResult {
    Shown,
    NothingHidden,
}

impl HidePlaceResult {
    pub(crate) fn status_message(self) -> String {
        match self {
            HidePlaceResult::Hidden { label } => format!("Hidden place {label}"),
            HidePlaceResult::CannotHide => "Place cannot be hidden".to_string(),
        }
    }
}

impl HidePlaceSectionResult {
    pub(crate) fn status_message(self) -> String {
        match self {
            HidePlaceSectionResult::Hidden { group } => {
                format!("Hidden places section {group}")
            }
            HidePlaceSectionResult::CannotHide => "Place section cannot be hidden".to_string(),
        }
    }
}

impl ShowHiddenPlacesResult {
    pub(crate) fn status_message(self) -> &'static str {
        match self {
            ShowHiddenPlacesResult::Shown => "Showing hidden places",
            ShowHiddenPlacesResult::NothingHidden => "No hidden places",
        }
    }
}

impl FikaApp {
    pub(crate) fn hide_place(&mut self, pane_id: PaneId, path: PathBuf) {
        let message = hide_place(&self.places, &mut self.hidden_places, path).status_message();
        self.set_pane_status(pane_id, message);
    }

    pub(crate) fn hide_place_section(&mut self, pane_id: PaneId, group: &'static str) {
        let message = hide_place_section(&self.places, &mut self.hidden_place_sections, group)
            .status_message();
        self.set_pane_status(pane_id, message);
    }

    pub(crate) fn show_hidden_places(&mut self, pane_id: PaneId) {
        let message = show_hidden_places(&mut self.hidden_places, &mut self.hidden_place_sections)
            .status_message();
        self.set_pane_status(pane_id, message);
    }
}

pub(crate) fn hide_place(
    places: &[PlaceEntry],
    hidden_places: &mut BTreeSet<PathBuf>,
    path: PathBuf,
) -> HidePlaceResult {
    let Some(place) = places.iter().find(|place| place.path == path) else {
        return HidePlaceResult::CannotHide;
    };
    let label = place.label.clone();
    hidden_places.insert(path);
    HidePlaceResult::Hidden { label }
}

pub(crate) fn hide_place_section(
    places: &[PlaceEntry],
    hidden_place_sections: &mut BTreeSet<&'static str>,
    group: &'static str,
) -> HidePlaceSectionResult {
    if group.is_empty() || !places.iter().any(|place| place.group == group) {
        return HidePlaceSectionResult::CannotHide;
    }
    hidden_place_sections.insert(group);
    HidePlaceSectionResult::Hidden { group }
}

pub(crate) fn show_hidden_places(
    hidden_places: &mut BTreeSet<PathBuf>,
    hidden_place_sections: &mut BTreeSet<&'static str>,
) -> ShowHiddenPlacesResult {
    if hidden_places.is_empty() && hidden_place_sections.is_empty() {
        return ShowHiddenPlacesResult::NothingHidden;
    }
    hidden_places.clear();
    hidden_place_sections.clear();
    ShowHiddenPlacesResult::Shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_place_tracks_known_place_without_removing_it() {
        let user = PathBuf::from("/home/yk/User");
        let places = vec![
            place("", "Home", "/home/yk", false),
            place("", "User", "/home/yk/User", true),
        ];
        let mut hidden_places = BTreeSet::new();

        assert_eq!(
            hide_place(&places, &mut hidden_places, user.clone()),
            HidePlaceResult::Hidden {
                label: "User".to_string()
            }
        );

        assert!(hidden_places.contains(&user));
        assert_eq!(places.len(), 2);
        assert_eq!(
            hide_place(
                &places,
                &mut hidden_places,
                PathBuf::from("/home/yk/Missing")
            ),
            HidePlaceResult::CannotHide
        );
    }

    #[test]
    fn hide_place_section_refuses_default_or_unknown_sections() {
        let places = vec![
            place("", "Home", "/home/yk", false),
            place("Devices", "Root", "/", false),
        ];
        let mut hidden_sections = BTreeSet::new();

        assert_eq!(
            hide_place_section(&places, &mut hidden_sections, ""),
            HidePlaceSectionResult::CannotHide
        );
        assert_eq!(
            hide_place_section(&places, &mut hidden_sections, "Network"),
            HidePlaceSectionResult::CannotHide
        );
        assert_eq!(
            hide_place_section(&places, &mut hidden_sections, "Devices"),
            HidePlaceSectionResult::Hidden { group: "Devices" }
        );
        assert!(hidden_sections.contains("Devices"));
    }

    #[test]
    fn show_hidden_places_clears_hidden_places_and_sections() {
        let mut hidden_places = BTreeSet::from([PathBuf::from("/home/yk/User")]);
        let mut hidden_sections = BTreeSet::from(["Devices"]);

        assert_eq!(
            show_hidden_places(&mut hidden_places, &mut hidden_sections),
            ShowHiddenPlacesResult::Shown
        );
        assert!(hidden_places.is_empty());
        assert!(hidden_sections.is_empty());
        assert_eq!(
            show_hidden_places(&mut hidden_places, &mut hidden_sections),
            ShowHiddenPlacesResult::NothingHidden
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
