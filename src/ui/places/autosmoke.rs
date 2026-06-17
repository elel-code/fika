use std::env;
use std::path::PathBuf;
use std::time::Duration;

use crate::ui::icons::FileIconSnapshot;

use super::interaction::places_interaction_geometry;
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

#[derive(Clone, Debug, PartialEq)]
struct PlacesHitTestAutosmokeReport {
    rows: usize,
    sections: usize,
    samples: Vec<PlacesHitTestAutosmokeSample>,
}

impl PlacesHitTestAutosmokeReport {
    fn ok(&self) -> bool {
        self.samples.iter().all(|sample| sample.ok)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PlacesHitTestAutosmokeSample {
    sample: &'static str,
    y: f32,
    kind: &'static str,
    zone: &'static str,
    visible_index: Option<usize>,
    insert_index: Option<usize>,
    ok: bool,
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

pub(crate) fn emit_places_retained_hit_test_autosmoke(
    label: &'static str,
    snapshots: &[PlaceSnapshot],
) {
    let report = retained_hit_test_autosmoke_report(snapshots);
    for sample in &report.samples {
        eprintln!(
            "[fika autosmoke] places hit-test label={} sample={} y={:.1} kind={} zone={} visible_index={} insert_index={} ok={}",
            label,
            sample.sample,
            sample.y,
            sample.kind,
            sample.zone,
            sample
                .visible_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            sample
                .insert_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            sample.ok
        );
    }
    eprintln!(
        "[fika autosmoke] places hit-test-summary label={} rows={} sections={} ok={}",
        label,
        report.rows,
        report.sections,
        report.ok()
    );
}

fn retained_hit_test_autosmoke_report(snapshots: &[PlaceSnapshot]) -> PlacesHitTestAutosmokeReport {
    let geometry = places_interaction_geometry(snapshots);
    let mut samples = Vec::new();

    if let Some(row) = geometry.rows().first() {
        samples.push(retained_hit_test_autosmoke_sample(
            "row-before",
            row.y + 1.0,
            "Row",
            "InsertBefore",
            &geometry,
        ));
        samples.push(retained_hit_test_autosmoke_sample(
            "row-body",
            row.y + row.height / 2.0,
            "Row",
            "OnPlace",
            &geometry,
        ));
        samples.push(retained_hit_test_autosmoke_sample(
            "row-after",
            row.y + row.height - 1.0,
            "Row",
            "InsertAfter",
            &geometry,
        ));
    }
    if let Some(section) = geometry.sections().first() {
        samples.push(retained_hit_test_autosmoke_sample(
            "section",
            section.y + 1.0,
            "Section",
            "Section",
            &geometry,
        ));
    }

    PlacesHitTestAutosmokeReport {
        rows: geometry.rows().len(),
        sections: geometry.sections().len(),
        samples,
    }
}

fn retained_hit_test_autosmoke_sample(
    sample: &'static str,
    y: f32,
    expected_kind: &'static str,
    expected_zone: &'static str,
    geometry: &super::interaction::PlacesInteractionGeometry,
) -> PlacesHitTestAutosmokeSample {
    let hit = geometry.hit_test_y(y);
    let (kind, zone, visible_index, insert_index) = match hit {
        Some(hit) => (
            hit.kind(),
            hit.drop_zone(),
            hit.visible_index(),
            Some(hit.insert_index()),
        ),
        None => ("<none>", "<none>", None, None),
    };
    PlacesHitTestAutosmokeSample {
        sample,
        y,
        kind,
        zone,
        visible_index,
        insert_index,
        ok: kind == expected_kind && zone == expected_zone,
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

    #[test]
    fn retained_hit_test_autosmoke_report_covers_row_edges_body_and_section() {
        let report = retained_hit_test_autosmoke_report(&[
            test_place(0, "", "Home", "/home/yk"),
            test_place(1, "Devices", "Root", "/"),
        ]);

        assert_eq!(report.rows, 2);
        assert_eq!(report.sections, 1);
        assert!(report.ok());
        assert_eq!(
            report
                .samples
                .iter()
                .map(|sample| (
                    sample.sample,
                    sample.kind,
                    sample.zone,
                    sample.visible_index
                ))
                .collect::<Vec<_>>(),
            vec![
                ("row-before", "Row", "InsertBefore", Some(0)),
                ("row-body", "Row", "OnPlace", Some(0)),
                ("row-after", "Row", "InsertAfter", Some(0)),
                ("section", "Section", "Section", None),
            ]
        );
        assert_eq!(report.samples[0].insert_index, Some(0));
        assert_eq!(report.samples[1].insert_index, Some(1));
        assert_eq!(report.samples[2].insert_index, Some(1));
        assert_eq!(report.samples[3].insert_index, Some(1));
    }

    fn test_place(index: usize, group: &'static str, label: &str, path: &str) -> PlaceSnapshot {
        PlaceSnapshot {
            index,
            group,
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: label.to_string(),
            path: PathBuf::from(path),
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
            editable: true,
            removable: true,
        }
    }
}
