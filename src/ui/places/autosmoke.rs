use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fika_core::load_app_settings;
use gpui::Context;

use crate::FikaApp;
use crate::ui::icons::FileIconSnapshot;

use super::interaction::{
    PlaceInteractionCursor, PlaceInteractionDecision, PlaceInteractionHit, PlaceInteractionTarget,
    PlaceRowTargetInput, place_row_path_list_target, place_row_place_drag_target,
    place_section_path_list_target, places_interaction_geometry,
};
use super::snapshot::PlaceSnapshot;

const AUTOSMOKE_PLACES_ENV: &str = "FIKA_AUTOSMOKE_PLACES";
const OVERFLOW_PLACE_COUNT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlacesAutosmokeScenario {
    DropTargets,
    Overflow,
    Layout,
    HitTest,
    RetainedTargeting,
    RetainedDnd,
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
    RetainedTargeting { label: &'static str },
    RetainedDnd { label: &'static str },
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

#[derive(Clone, Debug, PartialEq)]
struct PlacesRetainedTargetingAutosmokeReport {
    rows: usize,
    sections: usize,
    samples: Vec<PlacesRetainedTargetingAutosmokeSample>,
}

impl PlacesRetainedTargetingAutosmokeReport {
    fn ok(&self) -> bool {
        self.samples.iter().all(|sample| sample.ok)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PlacesRetainedTargetingAutosmokeSample {
    sample: &'static str,
    y: f32,
    target: &'static str,
    visible_index: Option<usize>,
    group: Option<&'static str>,
    activatable: bool,
    ok: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct PlacesRetainedDndAutosmokeReport {
    rows: usize,
    sections: usize,
    samples: Vec<PlacesRetainedDndAutosmokeSample>,
}

impl PlacesRetainedDndAutosmokeReport {
    fn ok(&self) -> bool {
        self.samples.iter().all(|sample| sample.ok)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PlacesRetainedDndAutosmokeSample {
    sample: &'static str,
    drag: &'static str,
    y: f32,
    target: String,
    cursor: &'static str,
    ok: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlacesSnapshotAutosmokeReport {
    visible: usize,
    sections: usize,
    active: usize,
    place_targets: usize,
    insert_before: usize,
    insert_after: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct PlacesTargetActionAutosmokeReport {
    target: String,
    changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlacesIndexActionAutosmokeReport {
    index: usize,
    changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacesLayoutAutosmokeState {
    pub(crate) width: f32,
    pub(crate) visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlacesLayoutSettingsAutosmokeReport {
    state: PlacesLayoutAutosmokeState,
    saved_width: Option<f32>,
    saved_visible: Option<bool>,
    ok: bool,
}

impl PlacesLayoutAutosmokeState {
    pub(crate) fn new(width: f32, visible: bool) -> Self {
        Self { width, visible }
    }
}

impl FikaApp {
    fn update_places_sidebar_layout_for_autosmoke(
        &mut self,
        width: f32,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let width_changed = self.set_places_sidebar_width(width);
        let visible_changed = self.set_places_sidebar_visible(visible);
        let changed = width_changed || visible_changed;
        if changed {
            self.schedule_app_settings_save(cx);
        }
        changed
    }
}

impl PlacesAutosmokeScenario {
    pub(crate) fn from_env() -> Option<Self> {
        places_autosmoke_scenario_from_value(&env::var(AUTOSMOKE_PLACES_ENV).ok()?)
    }

    fn marker_label(self) -> &'static str {
        match self {
            Self::DropTargets => "DropTargets",
            Self::Overflow => "Overflow",
            Self::Layout => "Layout",
            Self::HitTest => "HitTest",
            Self::RetainedTargeting => "RetainedTargeting",
            Self::RetainedDnd => "RetainedDnd",
        }
    }

    pub(crate) fn start_delay(self) -> Duration {
        let _ = self;
        Duration::from_millis(1200)
    }

    pub(crate) fn action_delay(self) -> Duration {
        match self {
            Self::DropTargets | Self::Overflow => Duration::from_millis(160),
            Self::Layout => Duration::from_millis(260),
            Self::HitTest | Self::RetainedTargeting | Self::RetainedDnd => {
                Duration::from_millis(160)
            }
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
            Self::RetainedTargeting => vec![PlacesAutosmokeAction::RetainedTargeting {
                label: "retained-targeting",
            }],
            Self::RetainedDnd => vec![PlacesAutosmokeAction::RetainedDnd {
                label: "retained-dnd",
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

pub(crate) fn start_places_autosmoke(scenario: PlacesAutosmokeScenario, cx: &mut Context<FikaApp>) {
    cx.spawn(
        move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                emit_places_autosmoke_start(scenario);
                cx.background_executor().timer(scenario.start_delay()).await;

                for action in scenario.actions() {
                    if this
                        .update(&mut cx, |app, cx| {
                            if app.apply_places_autosmoke_action(action, cx) {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                    cx.background_executor()
                        .timer(scenario.action_delay())
                        .await;
                }

                emit_places_autosmoke_complete(scenario);
            }
        },
    )
    .detach();
}

impl FikaApp {
    fn apply_places_autosmoke_action(
        &mut self,
        action: PlacesAutosmokeAction,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            PlacesAutosmokeAction::Snapshot { label } => {
                emit_places_autosmoke_snapshot(label, &self.place_snapshots());
                false
            }
            PlacesAutosmokeAction::TargetFirstPlace { label } => {
                let target = places_autosmoke_first_target_path(&self.place_snapshots());
                let changed = if let Some(path) = target.as_ref() {
                    self.set_place_drag_drop_target_for_path(path.clone())
                } else {
                    false
                };
                emit_places_autosmoke_place_target_action(label, target.as_deref(), changed);
                changed
            }
            PlacesAutosmokeAction::TargetInsertStart { label } => {
                let changed = self.set_place_drag_drop_target_for_insert(0);
                emit_places_autosmoke_insert_target_action(label, 0, changed);
                changed
            }
            PlacesAutosmokeAction::TargetInsertEnd { label } => {
                let index = self.places.len();
                let changed = self.set_place_drag_drop_target_for_insert(index);
                emit_places_autosmoke_insert_target_action(label, index, changed);
                changed
            }
            PlacesAutosmokeAction::ClearTargets { label } => {
                let changed = self.clear_place_drop_target();
                emit_places_autosmoke_clear_targets_action(label, changed);
                changed
            }
            PlacesAutosmokeAction::CaptureLayout { label } => {
                let original = *self.places_layout_autosmoke_original.get_or_insert(
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                );
                emit_places_autosmoke_layout_capture(label, original);
                false
            }
            PlacesAutosmokeAction::HideSidebar { label } => {
                let changed = self.update_places_sidebar_layout_for_autosmoke(
                    self.places_sidebar_width,
                    false,
                    cx,
                );
                emit_places_autosmoke_layout_update(
                    label,
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                    changed,
                );
                changed
            }
            PlacesAutosmokeAction::ShowSidebar { label } => {
                let changed = self.update_places_sidebar_layout_for_autosmoke(
                    self.places_sidebar_width,
                    true,
                    cx,
                );
                emit_places_autosmoke_layout_update(
                    label,
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                    changed,
                );
                changed
            }
            PlacesAutosmokeAction::ResizeSidebar { label } => {
                let target_width = places_autosmoke_resize_target_width(self.places_sidebar_width);
                let changed =
                    self.update_places_sidebar_layout_for_autosmoke(target_width, true, cx);
                emit_places_autosmoke_layout_resize(
                    label,
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                    target_width,
                    changed,
                );
                changed
            }
            PlacesAutosmokeAction::ResetSidebar { label } => {
                let changed = self.update_places_sidebar_layout_for_autosmoke(
                    super::PLACES_SIDEBAR_DEFAULT_WIDTH,
                    true,
                    cx,
                );
                emit_places_autosmoke_layout_update(
                    label,
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                    changed,
                );
                changed
            }
            PlacesAutosmokeAction::RestoreLayout { label } => {
                let original = self.places_layout_autosmoke_original.unwrap_or(
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                );
                let changed = self.update_places_sidebar_layout_for_autosmoke(
                    original.width,
                    original.visible,
                    cx,
                );
                emit_places_autosmoke_layout_update(
                    label,
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                    changed,
                );
                changed
            }
            PlacesAutosmokeAction::VerifyLayoutSettings { label } => {
                let settings = load_app_settings(&self.app_settings_path).ok();
                let saved_width = settings
                    .as_ref()
                    .and_then(|settings| settings.places_sidebar.width);
                let saved_visible = settings
                    .as_ref()
                    .and_then(|settings| settings.places_sidebar.visible);
                emit_places_autosmoke_layout_settings_verification(
                    label,
                    PlacesLayoutAutosmokeState::new(
                        self.places_sidebar_width,
                        self.places_sidebar_visible,
                    ),
                    saved_width,
                    saved_visible,
                    &self.app_settings_path,
                );
                false
            }
            PlacesAutosmokeAction::HitTest { label } => {
                emit_places_retained_hit_test_autosmoke(label, &self.place_snapshots());
                false
            }
            PlacesAutosmokeAction::RetainedTargeting { label } => {
                emit_places_retained_targeting_autosmoke(label, &self.place_snapshots());
                false
            }
            PlacesAutosmokeAction::RetainedDnd { label } => {
                emit_places_retained_dnd_autosmoke(label, &self.place_snapshots());
                false
            }
        }
    }
}

fn emit_places_autosmoke_start(scenario: PlacesAutosmokeScenario) {
    eprintln!(
        "[fika autosmoke] places start scenario={}",
        scenario.marker_label()
    );
}

fn emit_places_autosmoke_complete(scenario: PlacesAutosmokeScenario) {
    eprintln!(
        "[fika autosmoke] places complete scenario={}",
        scenario.marker_label()
    );
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
        "targeting" | "retained-targeting" | "retained_targeting" | "retained-click"
        | "retained_click" => Some(PlacesAutosmokeScenario::RetainedTargeting),
        "dnd" | "retained-dnd" | "retained_dnd" | "drag-drop" | "drag_drop" => {
            Some(PlacesAutosmokeScenario::RetainedDnd)
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

pub(crate) fn places_autosmoke_first_target_path(snapshots: &[PlaceSnapshot]) -> Option<PathBuf> {
    snapshots
        .iter()
        .find(|place| place.mounted)
        .map(|place| place.path.clone())
}

pub(crate) fn places_autosmoke_resize_target_width(current_width: f32) -> f32 {
    if current_width < 300.0 {
        320.0
    } else {
        super::PLACES_SIDEBAR_DEFAULT_WIDTH - 40.0
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

pub(crate) fn emit_places_retained_dnd_autosmoke(label: &'static str, snapshots: &[PlaceSnapshot]) {
    let report = retained_dnd_autosmoke_report(snapshots);
    for sample in &report.samples {
        eprintln!(
            "[fika autosmoke] places dnd label={} sample={} drag={} y={:.1} target={} cursor={} ok={}",
            label, sample.sample, sample.drag, sample.y, sample.target, sample.cursor, sample.ok
        );
    }
    eprintln!(
        "[fika autosmoke] places dnd-summary label={} rows={} sections={} ok={}",
        label,
        report.rows,
        report.sections,
        report.ok()
    );
}

pub(crate) fn emit_places_retained_targeting_autosmoke(
    label: &'static str,
    snapshots: &[PlaceSnapshot],
) {
    let report = retained_targeting_autosmoke_report(snapshots);
    for sample in &report.samples {
        eprintln!(
            "[fika autosmoke] places targeting label={} sample={} y={:.1} target={} visible_index={} group={} activatable={} ok={}",
            label,
            sample.sample,
            sample.y,
            sample.target,
            sample
                .visible_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            sample.group.unwrap_or("<none>"),
            sample.activatable,
            sample.ok
        );
    }
    eprintln!(
        "[fika autosmoke] places targeting-summary label={} rows={} sections={} ok={}",
        label,
        report.rows,
        report.sections,
        report.ok()
    );
}

pub(crate) fn emit_places_autosmoke_place_target_action(
    label: &'static str,
    target: Option<&Path>,
    changed: bool,
) {
    let report = target_action_autosmoke_report(target, changed);
    eprintln!(
        "[fika autosmoke] places action={} target={} changed={}",
        label, report.target, report.changed
    );
}

pub(crate) fn emit_places_autosmoke_insert_target_action(
    label: &'static str,
    index: usize,
    changed: bool,
) {
    let report = index_action_autosmoke_report(index, changed);
    eprintln!(
        "[fika autosmoke] places action={} index={} changed={}",
        label, report.index, report.changed
    );
}

pub(crate) fn emit_places_autosmoke_clear_targets_action(label: &'static str, changed: bool) {
    eprintln!(
        "[fika autosmoke] places action={} changed={}",
        label, changed
    );
}

pub(crate) fn emit_places_autosmoke_layout_capture(
    label: &'static str,
    state: PlacesLayoutAutosmokeState,
) {
    eprintln!(
        "[fika autosmoke] places action={} width={:.1} visible={}",
        label, state.width, state.visible
    );
}

pub(crate) fn emit_places_autosmoke_layout_update(
    label: &'static str,
    state: PlacesLayoutAutosmokeState,
    changed: bool,
) {
    eprintln!(
        "[fika autosmoke] places action={} width={:.1} visible={} changed={}",
        label, state.width, state.visible, changed
    );
}

pub(crate) fn emit_places_autosmoke_layout_resize(
    label: &'static str,
    state: PlacesLayoutAutosmokeState,
    target_width: f32,
    changed: bool,
) {
    eprintln!(
        "[fika autosmoke] places action={} width={:.1} visible={} target_width={:.1} changed={}",
        label, state.width, state.visible, target_width, changed
    );
}

pub(crate) fn emit_places_autosmoke_layout_settings_verification(
    label: &'static str,
    state: PlacesLayoutAutosmokeState,
    saved_width: Option<f32>,
    saved_visible: Option<bool>,
    path: &Path,
) -> bool {
    let report = layout_settings_autosmoke_report(state, saved_width, saved_visible);
    eprintln!(
        "[fika autosmoke] places action={} width={:.1} visible={} saved_width={} saved_visible={} ok={} path={}",
        label,
        report.state.width,
        report.state.visible,
        saved_width_label(report.saved_width),
        saved_visible_label(report.saved_visible),
        report.ok,
        path.display()
    );
    report.ok
}

pub(crate) fn emit_places_autosmoke_snapshot(label: &'static str, snapshots: &[PlaceSnapshot]) {
    let report = snapshot_autosmoke_report(snapshots);
    eprintln!(
        "[fika autosmoke] places snapshot={} visible={} sections={} active={} place_targets={} insert_before={} insert_after={}",
        label,
        report.visible,
        report.sections,
        report.active,
        report.place_targets,
        report.insert_before,
        report.insert_after
    );
}

fn target_action_autosmoke_report(
    target: Option<&Path>,
    changed: bool,
) -> PlacesTargetActionAutosmokeReport {
    PlacesTargetActionAutosmokeReport {
        target: target
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        changed,
    }
}

fn index_action_autosmoke_report(index: usize, changed: bool) -> PlacesIndexActionAutosmokeReport {
    PlacesIndexActionAutosmokeReport { index, changed }
}

fn layout_settings_autosmoke_report(
    state: PlacesLayoutAutosmokeState,
    saved_width: Option<f32>,
    saved_visible: Option<bool>,
) -> PlacesLayoutSettingsAutosmokeReport {
    let width_ok = saved_width.is_some_and(|width| layout_width_value_eq(width, state.width));
    let visible_ok = saved_visible == Some(state.visible);
    PlacesLayoutSettingsAutosmokeReport {
        state,
        saved_width,
        saved_visible,
        ok: width_ok && visible_ok,
    }
}

fn layout_width_value_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

fn saved_width_label(width: Option<f32>) -> String {
    width
        .map(|width| format!("{width:.1}"))
        .unwrap_or_else(|| "<none>".to_string())
}

fn saved_visible_label(visible: Option<bool>) -> String {
    visible
        .map(|visible| visible.to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

fn snapshot_autosmoke_report(snapshots: &[PlaceSnapshot]) -> PlacesSnapshotAutosmokeReport {
    PlacesSnapshotAutosmokeReport {
        visible: snapshots.len(),
        sections: snapshot_section_count(snapshots),
        active: snapshots.iter().filter(|place| place.active).count(),
        place_targets: snapshots.iter().filter(|place| place.drop_target).count(),
        insert_before: snapshots.iter().filter(|place| place.insert_before).count(),
        insert_after: snapshots.iter().filter(|place| place.insert_after).count(),
    }
}

fn snapshot_section_count(snapshots: &[PlaceSnapshot]) -> usize {
    let mut sections = 0;
    let mut current_group = None;
    for place in snapshots {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                sections += 1;
            }
        }
    }
    sections
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

fn retained_targeting_autosmoke_report(
    snapshots: &[PlaceSnapshot],
) -> PlacesRetainedTargetingAutosmokeReport {
    let geometry = places_interaction_geometry(snapshots);
    let mut samples = Vec::new();

    if let Some(row) = geometry.rows().iter().find(|row| row.activatable()) {
        samples.push(retained_targeting_autosmoke_sample(
            "activation-row",
            row.y + row.height / 2.0,
            "ActivationRow",
            &geometry,
        ));
    }
    if let Some(row) = geometry.rows().first() {
        samples.push(retained_targeting_autosmoke_sample(
            "context-row",
            row.y + row.height / 2.0,
            "ContextRow",
            &geometry,
        ));
    }
    if let Some(section) = geometry.sections().first() {
        samples.push(retained_targeting_autosmoke_sample(
            "context-section",
            section.y + 1.0,
            "ContextSection",
            &geometry,
        ));
    }

    PlacesRetainedTargetingAutosmokeReport {
        rows: geometry.rows().len(),
        sections: geometry.sections().len(),
        samples,
    }
}

fn retained_targeting_autosmoke_sample(
    sample: &'static str,
    y: f32,
    expected_target: &'static str,
    geometry: &super::interaction::PlacesInteractionGeometry,
) -> PlacesRetainedTargetingAutosmokeSample {
    let hit = geometry.hit_test_y(y);
    let (target, visible_index, group, activatable) = match hit {
        Some(PlaceInteractionHit::Row { row, .. }) if sample == "activation-row" => (
            "ActivationRow",
            Some(row.visible_index),
            Some(row.group),
            row.activatable(),
        ),
        Some(PlaceInteractionHit::Row { row, .. }) => (
            "ContextRow",
            Some(row.visible_index),
            Some(row.group),
            row.activatable(),
        ),
        Some(PlaceInteractionHit::Section(section)) => {
            ("ContextSection", None, Some(section.group), false)
        }
        None => ("<none>", None, None, false),
    };

    PlacesRetainedTargetingAutosmokeSample {
        sample,
        y,
        target,
        visible_index,
        group,
        activatable,
        ok: target == expected_target
            && (sample != "activation-row" || activatable)
            && (sample != "context-section" || group.is_some()),
    }
}

fn retained_dnd_autosmoke_report(snapshots: &[PlaceSnapshot]) -> PlacesRetainedDndAutosmokeReport {
    let geometry = places_interaction_geometry(snapshots);
    let mut samples = Vec::new();

    if let Some(row) = geometry.rows().first() {
        samples.push(retained_dnd_path_list_sample(
            "path-row-body",
            row.y + row.height / 2.0,
            "Place",
            PlaceInteractionCursor::DropMenu,
            false,
            &geometry,
        ));
        samples.push(retained_dnd_path_list_sample(
            "path-row-before",
            row.y + 1.0,
            "Insert",
            PlaceInteractionCursor::Copy,
            true,
            &geometry,
        ));
    }
    if let Some(section) = geometry.sections().first() {
        samples.push(retained_dnd_path_list_sample(
            "path-section",
            section.y + 1.0,
            "Insert",
            PlaceInteractionCursor::Copy,
            true,
            &geometry,
        ));
    }
    if geometry.rows().len() >= 2 {
        let source_index = geometry.rows()[0].place_index;
        let target_row = &geometry.rows()[1];
        samples.push(retained_dnd_place_drag_sample(
            "place-row-body",
            target_row.y + target_row.height / 2.0,
            source_index,
            "Insert",
            PlaceInteractionCursor::Move,
            &geometry,
        ));
    }
    samples.push(retained_dnd_path_list_sample(
        "path-outside",
        geometry.content_height() + 8.0,
        "Clear",
        PlaceInteractionCursor::NotAllowed,
        true,
        &geometry,
    ));

    PlacesRetainedDndAutosmokeReport {
        rows: geometry.rows().len(),
        sections: geometry.sections().len(),
        samples,
    }
}

fn retained_dnd_path_list_sample(
    sample: &'static str,
    y: f32,
    expected_target: &'static str,
    expected_cursor: PlaceInteractionCursor,
    can_add_place: bool,
    geometry: &super::interaction::PlacesInteractionGeometry,
) -> PlacesRetainedDndAutosmokeSample {
    let decision = match geometry.hit_test_y(y) {
        Some(PlaceInteractionHit::Row { row, drop_zone }) => {
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone,
                mounted: row.mounted,
                can_add_place,
                accepts_place: true,
                insert_before_index: row.insert_before_index,
                insert_after_index: row.insert_after_index,
                target_path: &row.path,
            })
        }
        Some(PlaceInteractionHit::Section(section)) => {
            place_section_path_list_target(can_add_place, section.insert_index)
        }
        None => PlaceInteractionDecision {
            target: PlaceInteractionTarget::Clear,
            cursor: PlaceInteractionCursor::NotAllowed,
        },
    };
    retained_dnd_autosmoke_sample_from_decision(
        sample,
        "path-list",
        y,
        decision,
        expected_target,
        expected_cursor,
    )
}

fn retained_dnd_place_drag_sample(
    sample: &'static str,
    y: f32,
    source_index: usize,
    expected_target: &'static str,
    expected_cursor: PlaceInteractionCursor,
    geometry: &super::interaction::PlacesInteractionGeometry,
) -> PlacesRetainedDndAutosmokeSample {
    let decision = match geometry.hit_test_y(y) {
        Some(PlaceInteractionHit::Row { row, drop_zone }) => place_row_place_drag_target(
            true,
            source_index,
            drop_zone,
            row.insert_before_index,
            row.insert_after_index,
        ),
        Some(PlaceInteractionHit::Section(section)) => {
            super::interaction::place_section_place_drag_target(
                true,
                source_index,
                section.insert_index,
            )
        }
        None => PlaceInteractionDecision {
            target: PlaceInteractionTarget::Clear,
            cursor: PlaceInteractionCursor::NotAllowed,
        },
    };
    retained_dnd_autosmoke_sample_from_decision(
        sample,
        "place",
        y,
        decision,
        expected_target,
        expected_cursor,
    )
}

fn retained_dnd_autosmoke_sample_from_decision(
    sample: &'static str,
    drag: &'static str,
    y: f32,
    decision: PlaceInteractionDecision,
    expected_target: &'static str,
    expected_cursor: PlaceInteractionCursor,
) -> PlacesRetainedDndAutosmokeSample {
    let target = retained_dnd_target_label(&decision);
    let cursor = retained_dnd_cursor_label(decision.cursor);
    PlacesRetainedDndAutosmokeSample {
        sample,
        drag,
        y,
        ok: target == expected_target && decision.cursor == expected_cursor,
        target: target.to_string(),
        cursor,
    }
}

fn retained_dnd_target_label(decision: &PlaceInteractionDecision) -> &'static str {
    match decision.target {
        PlaceInteractionTarget::Clear => "Clear",
        PlaceInteractionTarget::Insert { .. } => "Insert",
        PlaceInteractionTarget::Place { .. } => "Place",
    }
}

fn retained_dnd_cursor_label(cursor: PlaceInteractionCursor) -> &'static str {
    match cursor {
        PlaceInteractionCursor::Copy => "Copy",
        PlaceInteractionCursor::Move => "Move",
        PlaceInteractionCursor::DropMenu => "DropMenu",
        PlaceInteractionCursor::NotAllowed => "NotAllowed",
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
        assert_eq!(
            places_autosmoke_scenario_from_value("retained-targeting"),
            Some(PlacesAutosmokeScenario::RetainedTargeting)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("retained_click"),
            Some(PlacesAutosmokeScenario::RetainedTargeting)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("retained-dnd"),
            Some(PlacesAutosmokeScenario::RetainedDnd)
        );
        assert_eq!(
            places_autosmoke_scenario_from_value("drag_drop"),
            Some(PlacesAutosmokeScenario::RetainedDnd)
        );
        assert_eq!(places_autosmoke_scenario_from_value("off"), None);
    }

    #[test]
    fn scenario_marker_labels_match_analyzer_markers() {
        assert_eq!(
            PlacesAutosmokeScenario::DropTargets.marker_label(),
            "DropTargets"
        );
        assert_eq!(PlacesAutosmokeScenario::Overflow.marker_label(), "Overflow");
        assert_eq!(PlacesAutosmokeScenario::Layout.marker_label(), "Layout");
        assert_eq!(PlacesAutosmokeScenario::HitTest.marker_label(), "HitTest");
        assert_eq!(
            PlacesAutosmokeScenario::RetainedTargeting.marker_label(),
            "RetainedTargeting"
        );
        assert_eq!(
            PlacesAutosmokeScenario::RetainedDnd.marker_label(),
            "RetainedDnd"
        );
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
    fn target_action_autosmoke_report_formats_target_path() {
        assert_eq!(
            target_action_autosmoke_report(Some(std::path::Path::new("/home/yk/Downloads")), true),
            PlacesTargetActionAutosmokeReport {
                target: "/home/yk/Downloads".to_string(),
                changed: true,
            }
        );
        assert_eq!(
            target_action_autosmoke_report(None, false),
            PlacesTargetActionAutosmokeReport {
                target: "<none>".to_string(),
                changed: false,
            }
        );
    }

    #[test]
    fn first_target_path_uses_first_mounted_place() {
        let mut hidden = test_place(0, "", "Offline", "/offline");
        hidden.mounted = false;
        let home = test_place(1, "", "Home", "/home/yk");
        let downloads = test_place(2, "", "Downloads", "/home/yk/Downloads");

        assert_eq!(
            places_autosmoke_first_target_path(&[hidden, home, downloads]),
            Some(PathBuf::from("/home/yk"))
        );
    }

    #[test]
    fn first_target_path_returns_none_when_no_place_is_mounted() {
        let mut hidden = test_place(0, "", "Offline", "/offline");
        hidden.mounted = false;

        assert_eq!(places_autosmoke_first_target_path(&[hidden]), None);
    }

    #[test]
    fn index_action_autosmoke_report_keeps_insert_index() {
        assert_eq!(
            index_action_autosmoke_report(12, true),
            PlacesIndexActionAutosmokeReport {
                index: 12,
                changed: true,
            }
        );
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
    fn layout_resize_target_moves_between_default_and_wide_widths() {
        assert_eq!(places_autosmoke_resize_target_width(220.0), 320.0);
        assert_eq!(places_autosmoke_resize_target_width(320.0), 180.0);
    }

    #[test]
    fn layout_settings_autosmoke_report_matches_saved_sidebar_state() {
        let state = PlacesLayoutAutosmokeState::new(276.0, false);

        assert_eq!(
            layout_settings_autosmoke_report(state, Some(276.2), Some(false)),
            PlacesLayoutSettingsAutosmokeReport {
                state,
                saved_width: Some(276.2),
                saved_visible: Some(false),
                ok: true,
            }
        );
        assert!(!layout_settings_autosmoke_report(state, Some(277.0), Some(false)).ok);
        assert!(!layout_settings_autosmoke_report(state, Some(276.0), Some(true)).ok);
        assert!(!layout_settings_autosmoke_report(state, None, Some(false)).ok);
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
    fn retained_targeting_scenario_contains_only_targeting_action() {
        let actions = PlacesAutosmokeScenario::RetainedTargeting.actions();

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0],
            PlacesAutosmokeAction::RetainedTargeting {
                label: "retained-targeting"
            }
        ));
    }

    #[test]
    fn retained_dnd_scenario_contains_only_dnd_action() {
        let actions = PlacesAutosmokeScenario::RetainedDnd.actions();

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0],
            PlacesAutosmokeAction::RetainedDnd {
                label: "retained-dnd"
            }
        ));
    }

    #[test]
    fn snapshot_autosmoke_report_counts_visible_sections_and_targets() {
        let mut home = test_place(0, "", "Home", "/home/yk");
        home.active = true;
        let mut downloads = test_place(1, "", "Downloads", "/home/yk/Downloads");
        downloads.drop_target = true;
        let mut root = test_place(2, "Devices", "Root", "/");
        root.insert_before = true;
        let mut network = test_place(3, "Network", "Share", "smb://server/share");
        network.insert_after = true;

        assert_eq!(
            snapshot_autosmoke_report(&[home, downloads, root, network]),
            PlacesSnapshotAutosmokeReport {
                visible: 4,
                sections: 2,
                active: 1,
                place_targets: 1,
                insert_before: 1,
                insert_after: 1,
            }
        );
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

    #[test]
    fn retained_dnd_autosmoke_report_covers_path_and_place_drag_targets() {
        let report = retained_dnd_autosmoke_report(&[
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
                    sample.drag,
                    sample.target.as_str(),
                    sample.cursor
                ))
                .collect::<Vec<_>>(),
            vec![
                ("path-row-body", "path-list", "Place", "DropMenu"),
                ("path-row-before", "path-list", "Insert", "Copy"),
                ("path-section", "path-list", "Insert", "Copy"),
                ("place-row-body", "place", "Insert", "Move"),
                ("path-outside", "path-list", "Clear", "NotAllowed"),
            ]
        );
    }

    #[test]
    fn retained_targeting_autosmoke_report_covers_activation_row_and_context_targets() {
        let report = retained_targeting_autosmoke_report(&[
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
                    sample.target,
                    sample.visible_index,
                    sample.group,
                    sample.activatable
                ))
                .collect::<Vec<_>>(),
            vec![
                ("activation-row", "ActivationRow", Some(0), Some(""), true),
                ("context-row", "ContextRow", Some(0), Some(""), true),
                (
                    "context-section",
                    "ContextSection",
                    None,
                    Some("Devices"),
                    false
                ),
            ]
        );
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
