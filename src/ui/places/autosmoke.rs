use std::env;
use std::time::Duration;

const AUTOSMOKE_PLACES_ENV: &str = "FIKA_AUTOSMOKE_PLACES";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlacesAutosmokeScenario {
    DropTargets,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlacesAutosmokeAction {
    Snapshot { label: &'static str },
    TargetFirstPlace { label: &'static str },
    TargetInsertStart { label: &'static str },
    TargetInsertEnd { label: &'static str },
    ClearTargets { label: &'static str },
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
        let _ = self;
        Duration::from_millis(160)
    }

    pub(crate) fn actions(self) -> Vec<PlacesAutosmokeAction> {
        let _ = self;
        vec![
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
        ]
    }
}

fn places_autosmoke_scenario_from_value(value: &str) -> Option<PlacesAutosmokeScenario> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "targets" | "drop-targets" | "drop_targets" => {
            Some(PlacesAutosmokeScenario::DropTargets)
        }
        _ => None,
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
}
