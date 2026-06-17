use std::env;
use std::path::PathBuf;
use std::time::Duration;

use crate::ui::icons::FileIconSnapshot;

use super::snapshot::PlaceSnapshot;

const AUTOSMOKE_PLACES_ENV: &str = "FIKA_AUTOSMOKE_PLACES";
const OVERFLOW_PLACE_COUNT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlacesAutosmokeScenario {
    DropTargets,
    Overflow,
    Layout,
    HitTest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlacesAutosmokeAction {
    Snapshot { label: &'static str },
    TargetFirstPlace { label: &'static str },
    TargetInsertStart { label: &'static str },
    TargetInsertEnd { label: &'static str },
    ClearTargets { label: &'static str },
    CaptureLayout { label: &'static str },
    HideSidebar { label: &'static str },
    ShowSidebar { label: &'static str },
    ResizeSidebar { label: &'static str },
    ResetSidebar { label: &'static str },
    RestoreLayout { label: &'static str },
    VerifyLayoutSettings { label: &'static str },
    HitTest { label: &'static str },
}

impl PlacesAutosmokeScenario {
    pub(crate) fn from_env() -> Option<Self> {
        places_autosmoke_scenario_from_value(&env::var(AUTOSMOKE_PLACES_ENV).ok()?)
    }

    pub(crate) fn start_delay(self) -> Duration {
        let _ = self;
        Duration::from_millis(1200)
    }

    pub(crate) fn action_delay(self) -> Duration {
        match self {
            Self::DropTargets | Self::Overflow => Duration::from_millis(160),
            Self::Layout => Duration::from_millis(260),
            Self::HitTest => Duration::from_millis(160),
        }
    }

    pub(crate) fn actions(self) -> Vec<PlacesAutosmokeAction> {
        match self {
            Self::DropTargets => vec![
                PlacesAutosmokeAction::Snapshot { label: "initial" },
                PlacesAutosmokeAction::TargetFirstPlace {
                    label: "target-first-place",
                },
                PlacesAutosmokeAction::Snapshot {
                    label: "after-place-target",
                },
                PlacesAutosmokeAction::TargetInsertStart {
                    label: "target-insert-start",
                },
                PlacesAutosmokeAction::Snapshot {
                    label: "after-insert-start",
                },
                PlacesAutosmokeAction::TargetInsertEnd {
                    label: "target-insert-end",
                },
                PlacesAutosmokeAction::Snapshot {
                    label: "after-insert-end",
                },
                PlacesAutosmokeAction::ClearTargets {
                    label: "clear-targets",
                },
                PlacesAutosmokeAction::Snapshot {
                    label: "after-clear",
                },
            ],
            Self::Overflow => vec![PlacesAutosmokeAction::Snapshot { label: "overflow" }],
            Self::Layout => vec![
                PlacesAutosmokeAction::CaptureLayout {
                    label: "layout-initial",
                },
                PlacesAutosmokeAction::HideSidebar {
                    label: "layout-hide",
                },
                PlacesAutosmokeAction::ShowSidebar {
                    label: "layout-show",
                },
                PlacesAutosmokeAction::ResizeSidebar {
                    label: "layout-resize",
                },
                PlacesAutosmokeAction::ResetSidebar {
                    label: "layout-reset",
                },
                PlacesAutosmokeAction::RestoreLayout {
                    label: "layout-restore",
                },
                PlacesAutosmokeAction::VerifyLayoutSettings {
                    label: "layout-verify-saved",
                },
            ],
            Self::HitTest => vec![PlacesAutosmokeAction::HitTest {
                label: "retained-hit-test",
            }],
        }
    }

    pub(crate) fn append_extra_snapshots(self, snapshots: &mut Vec<PlaceSnapshot>) {
        if !matches!(self, Self::Overflow) {
            return;
        }
        append_overflow_test_places(snapshots);
    }
}

fn places_autosmoke_scenario_from_value(value: &str) -> Option<PlacesAutosmokeScenario> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "targets" | "drop-targets" | "drop_targets" => {
            Some(PlacesAutosmokeScenario::DropTargets)
        }
        "overflow" | "scroll" | "scroll-overflow" | "scroll_overflow" => {
            Some(PlacesAutosmokeScenario::Overflow)
        }
        "layout" | "sidebar" | "sidebar-layout" | "sidebar_layout" => {
            Some(PlacesAutosmokeScenario::Layout)
        }
        "hit-test" | "hit_test" | "hittest" | "retained-hit-test" | "retained_hit_test" => {
            Some(PlacesAutosmokeScenario::HitTest)
        }
        _ => None,
    }
}

fn append_overflow_test_places(snapshots: &mut Vec<PlaceSnapshot>) {
    let start_index = snapshots
        .iter()
        .map(|place| place.index)
        .max()
        .map_or(0, |index| index + 1);
    for offset in 0..OVERFLOW_PLACE_COUNT {
        snapshots.push(PlaceSnapshot {
            index: start_index + offset,
            group: "Autosmoke",
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: format!("Autosmoke {:02}", offset + 1),
            path: PathBuf::from(format!("/tmp/fika-places-autosmoke-{offset:02}")),
            device_id: None,
            mounted: true,
            device: false,
            network: false,
            device_ejectable: false,
            device_can_power_off: false,
            active: false,
            drop_target: false,
            insert_before: false,
            insert_after: false,
            trash_place: false,
            trash_has_items: false,
            editable: false,
            removable: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_places_autosmoke_scenario_values() {
        assert_eq!(
            places_autosmoke_scenario_from_value("targets"),
            Some(PlacesAutosmokeScenario::DropTargets)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("drop-targets"),
            Some(PlacesAutosmokeScenario::DropTargets)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("1"),
            Some(PlacesAutosmokeScenario::DropTargets)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("overflow"),
            Some(PlacesAutosmokeScenario::Overflow)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("scroll-overflow"),
            Some(PlacesAutosmokeScenario::Overflow)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("layout"),
            Some(PlacesAutosmokeScenario::Layout)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("sidebar-layout"),
            Some(PlacesAutosmokeScenario::Layout)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("hit-test"),
            Some(PlacesAutosmokeScenario::HitTest)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("retained_hit_test"),
            Some(PlacesAutosmokeScenario::HitTest)
        );
        assert_eq!(places_autosmoke_scenario_from_value("off"), None);
    }

    #[test]
    fn drop_target_scenario_contains_snapshot_and_target_actions() {
        let actions = PlacesAutosmokeScenario::DropTargets.actions();

        assert_eq!(actions.len(), 9);
        assert!(matches!(actions[0], PlacesAutosmokeAction::Snapshot { .. }));
        assert!(matches!(
            actions[1],
            PlacesAutosmokeAction::TargetFirstPlace { .. }
        ));
        assert!(matches!(
            actions[3],
            PlacesAutosmokeAction::TargetInsertStart { .. }
        ));
        assert!(matches!(
            actions[5],
            PlacesAutosmokeAction::TargetInsertEnd { .. }
        ));
        assert!(matches!(
            actions[7],
            PlacesAutosmokeAction::ClearTargets { .. }
        ));
    }

    #[test]
    fn overflow_scenario_appends_non_persistent_test_places() {
        let mut snapshots = Vec::new();
        PlacesAutosmokeScenario::Overflow.append_extra_snapshots(&mut snapshots);

        assert_eq!(snapshots.len(), OVERFLOW_PLACE_COUNT);
        assert_eq!(snapshots[0].group, "Autosmoke");
        assert_eq!(snapshots[0].index, 0);
        assert_eq!(snapshots[OVERFLOW_PLACE_COUNT - 1].label, "Autosmoke 64");
    }

    #[test]
    fn overflow_scenario_contains_only_snapshot_action() {
        let actions = PlacesAutosmokeScenario::Overflow.actions();

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0],
            PlacesAutosmokeAction::Snapshot { label: "overflow" }
        ));
    }

    #[test]
    fn layout_scenario_contains_sidebar_layout_actions() {
        let actions = PlacesAutosmokeScenario::Layout.actions();

        assert_eq!(actions.len(), 7);
        assert!(matches!(
            actions[0],
            PlacesAutosmokeAction::CaptureLayout { .. }
        ));
        assert!(matches!(
            actions[1],
            PlacesAutosmokeAction::HideSidebar { .. }
        ));
        assert!(matches!(
            actions[2],
            PlacesAutosmokeAction::ShowSidebar { .. }
        ));
        assert!(matches!(
            actions[3],
            PlacesAutosmokeAction::ResizeSidebar { .. }
        ));
        assert!(matches!(
            actions[4],
            PlacesAutosmokeAction::ResetSidebar { .. }
        ));
        assert!(matches!(
            actions[5],
            PlacesAutosmokeAction::RestoreLayout { .. }
        ));
        assert!(matches!(
            actions[6],
            PlacesAutosmokeAction::VerifyLayoutSettings { .. }
        ));
    }

    #[test]
    fn hit_test_scenario_contains_only_retained_hit_test_action() {
        let actions = PlacesAutosmokeScenario::HitTest.actions();

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0],
            PlacesAutosmokeAction::HitTest {
                label: "retained-hit-test"
            }
        ));
    }
}
