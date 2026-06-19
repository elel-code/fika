use std::env;
use std::time::Duration;

use crate::ui::retained::{TextShapeCacheStats, env_flag_is_truthy};

use super::PlaceSnapshot;
use super::paint_slots::PlacePaintSlotPerfLog;

const PERF_PLACES_VIEW_ENV: &str = "FIKA_PERF_PLACES_VIEW";
const CUSTOM_PLACES_ROWS_ENV: &str = "FIKA_CUSTOM_PLACES_ROWS";
const PLACES_ROW_VISUAL_POLICY_ENV: &str = "FIKA_PLACES_ROW_VISUAL_POLICY";
const PLACES_ROW_VISUAL_HANDOFF_ENV: &str = "FIKA_PLACES_ROW_VISUAL_HANDOFF";
const PLACES_EVENT_DELIVERY_POLICY_ENV: &str = "FIKA_PLACES_EVENT_DELIVERY_POLICY";
const DEFAULT_PLACES_EVENT_DELIVERY_POLICY: PlacesEventDeliveryPolicy =
    PlacesEventDeliveryPolicy::RetainedDnd;
const DEFAULT_PLACES_ROW_VISUAL_POLICY: PlacesRowVisualPolicy = PlacesRowVisualPolicy::CustomFull;

pub(crate) fn places_perf_enabled() -> bool {
    env::var(PERF_PLACES_VIEW_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlacesRowVisualPolicy {
    Gpui,
    CustomChrome,
    CustomText,
    CustomFull,
}

impl PlacesRowVisualPolicy {
    pub(crate) fn custom_layer_enabled(self) -> bool {
        matches!(
            self,
            Self::CustomChrome | Self::CustomText | Self::CustomFull
        )
    }

    pub(crate) fn paints_text(self) -> bool {
        matches!(self, Self::CustomText | Self::CustomFull)
    }

    pub(crate) fn paints_icon(self) -> bool {
        matches!(self, Self::CustomFull)
    }

    fn visual_kind(self) -> &'static str {
        match self {
            Self::Gpui => "gpui",
            Self::CustomChrome => "chrome",
            Self::CustomText => "text",
            Self::CustomFull => "full",
        }
    }
}

pub(crate) fn places_row_visual_policy() -> PlacesRowVisualPolicy {
    if env::var(CUSTOM_PLACES_ROWS_ENV).is_ok_and(|value| env_flag_is_truthy(&value)) {
        return PlacesRowVisualPolicy::CustomText;
    }

    env::var(PLACES_ROW_VISUAL_POLICY_ENV)
        .ok()
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "gpui" | "off" | "0" => Some(PlacesRowVisualPolicy::Gpui),
            "chrome" | "hybrid" => Some(PlacesRowVisualPolicy::CustomChrome),
            "text" | "custom-text" | "full-text" => Some(PlacesRowVisualPolicy::CustomText),
            "full" | "custom" | "full-icon" | "full-icons" | "default" | "1" | "true" | "yes"
            | "on" => Some(PlacesRowVisualPolicy::CustomFull),
            _ => None,
        })
        .unwrap_or(DEFAULT_PLACES_ROW_VISUAL_POLICY)
}

pub(crate) fn places_row_visual_handoff_enabled() -> bool {
    env::var(PLACES_ROW_VISUAL_HANDOFF_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlacesEventDeliveryPolicy {
    GpuiShells,
    RetainedProbe,
    RetainedPointer,
    RetainedTargeting,
    RetainedDnd,
}

impl PlacesEventDeliveryPolicy {
    pub(crate) fn retained_event_layer_enabled(self) -> bool {
        matches!(
            self,
            Self::RetainedProbe
                | Self::RetainedPointer
                | Self::RetainedTargeting
                | Self::RetainedDnd
        )
    }

    pub(crate) fn retained_pointer_enabled(self) -> bool {
        matches!(
            self,
            Self::RetainedPointer | Self::RetainedTargeting | Self::RetainedDnd
        )
    }

    pub(crate) fn retained_targeting_enabled(self) -> bool {
        matches!(self, Self::RetainedTargeting | Self::RetainedDnd)
    }

    pub(crate) fn retained_dnd_enabled(self) -> bool {
        matches!(self, Self::RetainedDnd)
    }

    fn kind(self) -> &'static str {
        match self {
            Self::GpuiShells => "gpui",
            Self::RetainedProbe => "retained-probe",
            Self::RetainedPointer => "retained-pointer",
            Self::RetainedTargeting => "retained-targeting",
            Self::RetainedDnd => "retained-dnd",
        }
    }

    fn retained_interaction(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::RetainedTargeting | Self::RetainedDnd => rows + sections,
            Self::GpuiShells | Self::RetainedProbe | Self::RetainedPointer => 0,
        }
    }

    fn retained_hitboxes(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::RetainedTargeting | Self::RetainedDnd => rows + sections,
            Self::GpuiShells | Self::RetainedProbe | Self::RetainedPointer => 0,
        }
    }

    fn retained_probe_hitboxes(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::GpuiShells => 0,
            Self::RetainedProbe
            | Self::RetainedPointer
            | Self::RetainedTargeting
            | Self::RetainedDnd => rows + sections,
        }
    }

    fn gpui_event_shells(self, rows: usize, sections: usize) -> usize {
        self.gpui_row_section_event_shells(rows, sections)
            + self.gpui_typed_dnd_payload_shells(rows, sections)
    }

    fn gpui_row_section_event_shells(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::GpuiShells
            | Self::RetainedProbe
            | Self::RetainedPointer
            | Self::RetainedTargeting => rows + sections,
            Self::RetainedDnd => 0,
        }
    }

    fn gpui_typed_dnd_payload_shells(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::RetainedDnd => usize::from(rows + sections > 0),
            Self::GpuiShells
            | Self::RetainedProbe
            | Self::RetainedPointer
            | Self::RetainedTargeting => 0,
        }
    }

    fn gpui_sidebar_leave_shells(self) -> usize {
        if self.retained_pointer_enabled() {
            0
        } else {
            3
        }
    }

    fn retained_targeting(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::RetainedTargeting | Self::RetainedDnd => rows + sections,
            Self::GpuiShells | Self::RetainedProbe | Self::RetainedPointer => 0,
        }
    }

    fn retained_dnd(self, rows: usize, sections: usize) -> usize {
        match self {
            Self::RetainedDnd => rows + sections,
            Self::GpuiShells
            | Self::RetainedProbe
            | Self::RetainedPointer
            | Self::RetainedTargeting => 0,
        }
    }
}

pub(crate) fn places_event_delivery_policy() -> PlacesEventDeliveryPolicy {
    env::var(PLACES_EVENT_DELIVERY_POLICY_ENV)
        .ok()
        .and_then(|value| places_event_delivery_policy_from_str(&value))
        .unwrap_or(DEFAULT_PLACES_EVENT_DELIVERY_POLICY)
}

fn places_event_delivery_policy_from_str(value: &str) -> Option<PlacesEventDeliveryPolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => None,
        "gpui" | "gpui-shells" | "shells" | "off" | "0" | "false" | "no" => {
            Some(PlacesEventDeliveryPolicy::GpuiShells)
        }
        "probe" | "retained-probe" | "hitbox-probe" | "hitboxes-probe" => {
            Some(PlacesEventDeliveryPolicy::RetainedProbe)
        }
        "pointer" | "retained-pointer" | "hover" | "retained-hover" => {
            Some(PlacesEventDeliveryPolicy::RetainedPointer)
        }
        "targeting" | "retained-targeting" | "click" | "retained-click" => {
            Some(PlacesEventDeliveryPolicy::RetainedTargeting)
        }
        "dnd" | "retained-dnd" | "drag-drop" | "retained-drag-drop" => {
            Some(PlacesEventDeliveryPolicy::RetainedDnd)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesSnapshotPerfLog {
    pub(crate) source_count: usize,
    pub(crate) visible_count: usize,
    pub(crate) section_count: usize,
    pub(crate) elapsed: Duration,
}

pub(crate) fn emit_places_snapshot_perf_log(log: PlacesSnapshotPerfLog) {
    eprintln!(
        "[fika places-view] source={} visible={} sections={} snapshot={}us",
        log.source_count,
        log.visible_count,
        log.section_count,
        log.elapsed.as_micros(),
    );
}

pub(crate) fn emit_place_paint_slot_perf_log(log: PlacePaintSlotPerfLog) {
    let stats = log.stats;
    eprintln!(
        "[fika places-slots] rows={} sections={} entries={} inserted={} content={} geometry={} visual={} unchanged={} removed={} project={}us",
        stats.rows,
        stats.sections,
        stats.entries,
        stats.inserted,
        stats.content_changed,
        stats.geometry_changed,
        stats.visual_changed,
        stats.unchanged,
        stats.removed,
        log.elapsed.as_micros(),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesSidebarPerfLog {
    pub(crate) row_count: usize,
    pub(crate) section_count: usize,
    pub(crate) element_count: usize,
    pub(crate) build_elapsed: Duration,
}

pub(crate) fn emit_places_sidebar_perf_log(log: PlacesSidebarPerfLog) {
    eprintln!(
        "[fika places-sidebar] rows={} sections={} elements={} build={}us",
        log.row_count,
        log.section_count,
        log.element_count,
        log.build_elapsed.as_micros(),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesRowVisualPerfLog {
    pub(crate) rows: usize,
    pub(crate) painted_rows: usize,
    pub(crate) prepaint_elapsed: Duration,
    pub(crate) paint_elapsed: Duration,
}

pub(crate) fn emit_places_row_visual_perf_log(log: PlacesRowVisualPerfLog) {
    eprintln!(
        "[fika places-row-visual] rows={} painted={} prepaint={}us paint={}us",
        log.rows,
        log.painted_rows,
        log.prepaint_elapsed.as_micros(),
        log.paint_elapsed.as_micros(),
    );
}

pub(crate) type PlacesRowTextShapeCacheStats = TextShapeCacheStats;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesRowTextShapeCachePerfLog {
    pub(crate) stats: PlacesRowTextShapeCacheStats,
}

pub(crate) fn emit_places_row_text_shape_cache_perf_log(log: PlacesRowTextShapeCachePerfLog) {
    let stats = log.stats;
    eprintln!(
        "[fika places-row-shape-cache] hits={} misses={} evicted={} entries={}",
        stats.hits, stats.misses, stats.evicted, stats.entries,
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacesScrollbarPerfLog {
    pub(crate) visible: bool,
    pub(crate) max_scroll_y: f32,
    pub(crate) thumb_height: f32,
    pub(crate) track_height: f32,
}

pub(crate) fn emit_places_scrollbar_perf_log(log: PlacesScrollbarPerfLog) {
    eprintln!(
        "[fika places-scrollbar] visible={} max_scroll_y={} thumb_height={} track_height={}",
        usize::from(log.visible),
        log.max_scroll_y,
        log.thumb_height,
        log.track_height,
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesEventProbePerfLog {
    pub(crate) rows: usize,
    pub(crate) sections: usize,
    pub(crate) hitboxes: usize,
    pub(crate) hovered_hitboxes: usize,
    pub(crate) pointer_delivery: bool,
    pub(crate) targeting_delivery: bool,
    pub(crate) dnd_delivery: bool,
    pub(crate) prepaint_elapsed: Duration,
    pub(crate) paint_elapsed: Duration,
}

pub(crate) fn emit_places_event_probe_perf_log(log: PlacesEventProbePerfLog) {
    eprintln!(
        "[fika places-event-probe] rows={} sections={} hitboxes={} hovered={} pointer={} targeting={} dnd={} prepaint={}us paint={}us",
        log.rows,
        log.sections,
        log.hitboxes,
        log.hovered_hitboxes,
        usize::from(log.pointer_delivery),
        usize::from(log.targeting_delivery),
        usize::from(log.dnd_delivery),
        log.prepaint_elapsed.as_micros(),
        log.paint_elapsed.as_micros(),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesRendererPolicyLog {
    pub(crate) row_count: usize,
    pub(crate) section_count: usize,
    pub(crate) row_visual_policy: PlacesRowVisualPolicy,
    pub(crate) row_visual_paints_text: bool,
    pub(crate) row_visual_paints_icon: bool,
    pub(crate) section_visual_paints_text: bool,
    pub(crate) event_delivery_policy: PlacesEventDeliveryPolicy,
    pub(crate) scrollbar_canvas_count: usize,
}

pub(crate) fn emit_places_renderer_policy_log(log: PlacesRendererPolicyLog) {
    let row_gpui = if log.row_visual_policy == PlacesRowVisualPolicy::Gpui {
        log.row_count
    } else {
        0
    };
    let row_visual_layer = if log.row_visual_policy.custom_layer_enabled() {
        log.row_count
    } else {
        0
    };
    let text_gpui = if log.row_visual_paints_text {
        0
    } else {
        log.row_count
    };
    let icon_gpui = if log.row_visual_paints_icon {
        0
    } else {
        log.row_count
    };
    let section_gpui = if log.section_visual_paints_text {
        0
    } else {
        log.section_count
    };
    let retained_interaction = log
        .event_delivery_policy
        .retained_interaction(log.row_count, log.section_count);
    let retained_probe_hitboxes = log
        .event_delivery_policy
        .retained_probe_hitboxes(log.row_count, log.section_count);
    eprintln!(
        "[fika places-renderer-policy] rows={} row_gpui={} row_visual_layer={} text_gpui={} icon_gpui={} retained_interaction={} drag_shell={} section_gpui={} scrollbar_canvas={} visual_kind={} event_policy={} retained_probe_hitboxes={}",
        log.row_count,
        row_gpui,
        row_visual_layer,
        text_gpui,
        icon_gpui,
        retained_interaction,
        log.row_count,
        section_gpui,
        log.scrollbar_canvas_count,
        log.row_visual_policy.visual_kind(),
        log.event_delivery_policy.kind(),
        retained_probe_hitboxes,
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesRowVisualHandoffPerfLog {
    pub(crate) rows: usize,
    pub(crate) enabled: bool,
    pub(crate) ready: bool,
    pub(crate) warmup_frames_seen: u8,
    pub(crate) required_warmup_frames: u8,
    pub(crate) paint_text: bool,
    pub(crate) paint_icon: bool,
    pub(crate) gpui_text: bool,
    pub(crate) gpui_icon: bool,
}

pub(crate) fn emit_places_row_visual_handoff_perf_log(log: PlacesRowVisualHandoffPerfLog) {
    eprintln!(
        "[fika places-row-handoff] rows={} enabled={} ready={} frames={}/{} paint_text={} paint_icon={} gpui_text={} gpui_icon={}",
        log.rows,
        usize::from(log.enabled),
        usize::from(log.ready),
        log.warmup_frames_seen,
        log.required_warmup_frames,
        usize::from(log.paint_text),
        usize::from(log.paint_icon),
        usize::from(log.gpui_text),
        usize::from(log.gpui_icon),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesInteractionPolicyLog {
    pub(crate) row_count: usize,
    pub(crate) section_count: usize,
    pub(crate) event_delivery_policy: PlacesEventDeliveryPolicy,
}

impl PlacesInteractionPolicyLog {
    pub(crate) fn retained_row_target_decisions(self) -> usize {
        self.row_count
    }

    pub(crate) fn retained_section_target_decisions(self) -> usize {
        self.section_count
    }

    pub(crate) fn retained_hitboxes(self) -> usize {
        self.event_delivery_policy
            .retained_hitboxes(self.row_count, self.section_count)
    }

    pub(crate) fn retained_probe_hitboxes(self) -> usize {
        self.event_delivery_policy
            .retained_probe_hitboxes(self.row_count, self.section_count)
    }

    pub(crate) fn gpui_event_shells(self) -> usize {
        self.event_delivery_policy
            .gpui_event_shells(self.row_count, self.section_count)
    }

    pub(crate) fn gpui_row_section_event_shells(self) -> usize {
        self.event_delivery_policy
            .gpui_row_section_event_shells(self.row_count, self.section_count)
    }

    pub(crate) fn gpui_typed_dnd_payload_shells(self) -> usize {
        self.event_delivery_policy
            .gpui_typed_dnd_payload_shells(self.row_count, self.section_count)
    }

    pub(crate) fn gpui_sidebar_leave_shells(self) -> usize {
        self.event_delivery_policy.gpui_sidebar_leave_shells()
    }

    pub(crate) fn retained_targeting(self) -> usize {
        self.event_delivery_policy
            .retained_targeting(self.row_count, self.section_count)
    }

    pub(crate) fn retained_dnd(self) -> usize {
        self.event_delivery_policy
            .retained_dnd(self.row_count, self.section_count)
    }

    pub(crate) fn drag_shells(self) -> usize {
        self.row_count
    }

    pub(crate) fn drag_start_models(self) -> usize {
        self.row_count
    }
}

pub(crate) fn emit_places_interaction_policy_log(log: PlacesInteractionPolicyLog) {
    eprintln!(
        "[fika places-interaction-policy] rows={} sections={} row_target_decisions={} section_target_decisions={} retained_hitboxes={} retained_probe_hitboxes={} gpui_event_shells={} gpui_row_section_event_shells={} gpui_typed_dnd_payload_shells={} drag_shells={} drag_start_models={} gpui_sidebar_leave_shells={} event_policy={} retained_targeting={} retained_dnd={}",
        log.row_count,
        log.section_count,
        log.retained_row_target_decisions(),
        log.retained_section_target_decisions(),
        log.retained_hitboxes(),
        log.retained_probe_hitboxes(),
        log.gpui_event_shells(),
        log.gpui_row_section_event_shells(),
        log.gpui_typed_dnd_payload_shells(),
        log.drag_shells(),
        log.drag_start_models(),
        log.gpui_sidebar_leave_shells(),
        log.event_delivery_policy.kind(),
        log.retained_targeting(),
        log.retained_dnd(),
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacesInteractionGeometryPerfLog {
    pub(crate) rows: usize,
    pub(crate) sections: usize,
    pub(crate) entries: usize,
    pub(crate) content_height: f32,
    pub(crate) hit_tests: usize,
    pub(crate) elapsed: Duration,
}

pub(crate) fn emit_places_interaction_geometry_perf_log(log: PlacesInteractionGeometryPerfLog) {
    eprintln!(
        "[fika places-interaction-geometry] rows={} sections={} entries={} content_height={} hit_tests={} project={}us",
        log.rows,
        log.sections,
        log.entries,
        log.content_height,
        log.hit_tests,
        log.elapsed.as_micros(),
    );
}

pub(crate) fn places_section_count(places: &[PlaceSnapshot]) -> usize {
    let mut current_group = None;
    let mut section_count = 0;
    for place in places {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                section_count += 1;
            }
        }
    }
    section_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icons::FileIconSnapshot;
    use std::path::PathBuf;

    #[test]
    fn places_perf_env_flag_truthy_values_are_explicit() {
        assert!(env_flag_is_truthy("1"));
        assert!(env_flag_is_truthy(" true "));
        assert!(env_flag_is_truthy("YES"));
        assert!(env_flag_is_truthy("on"));
        assert!(!env_flag_is_truthy(""));
        assert!(!env_flag_is_truthy("0"));
        assert!(!env_flag_is_truthy("false"));
        assert!(!env_flag_is_truthy("disabled"));
    }

    #[test]
    fn places_row_visual_policy_keeps_chrome_and_text_boundaries_separate() {
        assert!(!PlacesRowVisualPolicy::Gpui.custom_layer_enabled());
        assert!(!PlacesRowVisualPolicy::Gpui.paints_text());
        assert_eq!(PlacesRowVisualPolicy::Gpui.visual_kind(), "gpui");

        assert!(PlacesRowVisualPolicy::CustomChrome.custom_layer_enabled());
        assert!(!PlacesRowVisualPolicy::CustomChrome.paints_text());
        assert!(!PlacesRowVisualPolicy::CustomChrome.paints_icon());
        assert_eq!(PlacesRowVisualPolicy::CustomChrome.visual_kind(), "chrome");

        assert!(PlacesRowVisualPolicy::CustomText.custom_layer_enabled());
        assert!(PlacesRowVisualPolicy::CustomText.paints_text());
        assert!(!PlacesRowVisualPolicy::CustomText.paints_icon());
        assert_eq!(PlacesRowVisualPolicy::CustomText.visual_kind(), "text");

        assert!(PlacesRowVisualPolicy::CustomFull.custom_layer_enabled());
        assert!(PlacesRowVisualPolicy::CustomFull.paints_text());
        assert!(PlacesRowVisualPolicy::CustomFull.paints_icon());
        assert_eq!(PlacesRowVisualPolicy::CustomFull.visual_kind(), "full");
    }

    #[test]
    fn places_event_delivery_policy_parser_keeps_probe_explicit() {
        assert_eq!(
            places_event_delivery_policy_from_str("gpui"),
            Some(PlacesEventDeliveryPolicy::GpuiShells)
        );
        assert_eq!(
            places_event_delivery_policy_from_str("retained-probe"),
            Some(PlacesEventDeliveryPolicy::RetainedProbe)
        );
        assert_eq!(
            places_event_delivery_policy_from_str("retained-pointer"),
            Some(PlacesEventDeliveryPolicy::RetainedPointer)
        );
        assert_eq!(
            places_event_delivery_policy_from_str("retained-targeting"),
            Some(PlacesEventDeliveryPolicy::RetainedTargeting)
        );
        assert_eq!(
            places_event_delivery_policy_from_str("retained-dnd"),
            Some(PlacesEventDeliveryPolicy::RetainedDnd)
        );
        assert_eq!(places_event_delivery_policy_from_str("retained"), None);
    }

    #[test]
    fn places_event_delivery_default_uses_retained_dnd_mixed_policy() {
        assert_eq!(
            DEFAULT_PLACES_EVENT_DELIVERY_POLICY,
            PlacesEventDeliveryPolicy::RetainedDnd
        );
        assert_eq!(
            DEFAULT_PLACES_EVENT_DELIVERY_POLICY.gpui_event_shells(11, 2),
            1
        );
        assert_eq!(
            DEFAULT_PLACES_EVENT_DELIVERY_POLICY.retained_hitboxes(11, 2),
            13
        );
    }

    #[test]
    fn places_section_count_tracks_visible_group_transitions() {
        let places = vec![
            place("", "Home"),
            place("", "Downloads"),
            place("Network", "Network"),
            place("Network", "Share"),
            place("Devices", "Root"),
        ];

        assert_eq!(places_section_count(&places), 2);
    }

    #[test]
    fn places_interaction_policy_keeps_gpui_shell_boundary_explicit() {
        let policy = PlacesInteractionPolicyLog {
            row_count: 11,
            section_count: 2,
            event_delivery_policy: PlacesEventDeliveryPolicy::GpuiShells,
        };

        assert_eq!(policy.retained_row_target_decisions(), 11);
        assert_eq!(policy.retained_section_target_decisions(), 2);
        assert_eq!(policy.retained_hitboxes(), 0);
        assert_eq!(policy.retained_probe_hitboxes(), 0);
        assert_eq!(policy.gpui_event_shells(), 13);
        assert_eq!(policy.gpui_row_section_event_shells(), 13);
        assert_eq!(policy.gpui_typed_dnd_payload_shells(), 0);
        assert_eq!(policy.gpui_sidebar_leave_shells(), 3);
        assert_eq!(policy.drag_shells(), 11);
        assert_eq!(policy.drag_start_models(), 11);
    }

    #[test]
    fn places_interaction_pointer_policy_keeps_full_delivery_unclaimed() {
        let policy = PlacesInteractionPolicyLog {
            row_count: 11,
            section_count: 2,
            event_delivery_policy: PlacesEventDeliveryPolicy::RetainedPointer,
        };

        assert!(PlacesEventDeliveryPolicy::RetainedPointer.retained_event_layer_enabled());
        assert!(PlacesEventDeliveryPolicy::RetainedPointer.retained_pointer_enabled());
        assert_eq!(policy.retained_hitboxes(), 0);
        assert_eq!(policy.retained_probe_hitboxes(), 13);
        assert_eq!(policy.gpui_event_shells(), 13);
        assert_eq!(policy.gpui_row_section_event_shells(), 13);
        assert_eq!(policy.gpui_typed_dnd_payload_shells(), 0);
        assert_eq!(policy.gpui_sidebar_leave_shells(), 0);
        assert_eq!(policy.retained_targeting(), 0);
        assert_eq!(policy.retained_dnd(), 0);
        assert_eq!(policy.drag_shells(), 11);
        assert_eq!(policy.drag_start_models(), 11);
    }

    #[test]
    fn places_interaction_targeting_policy_keeps_dnd_shell_boundary_explicit() {
        let policy = PlacesInteractionPolicyLog {
            row_count: 11,
            section_count: 2,
            event_delivery_policy: PlacesEventDeliveryPolicy::RetainedTargeting,
        };

        assert!(PlacesEventDeliveryPolicy::RetainedTargeting.retained_event_layer_enabled());
        assert!(PlacesEventDeliveryPolicy::RetainedTargeting.retained_pointer_enabled());
        assert!(PlacesEventDeliveryPolicy::RetainedTargeting.retained_targeting_enabled());
        assert_eq!(
            PlacesEventDeliveryPolicy::RetainedTargeting.retained_interaction(11, 2),
            13
        );
        assert_eq!(policy.retained_hitboxes(), 13);
        assert_eq!(policy.retained_probe_hitboxes(), 13);
        assert_eq!(policy.gpui_event_shells(), 13);
        assert_eq!(policy.gpui_row_section_event_shells(), 13);
        assert_eq!(policy.gpui_typed_dnd_payload_shells(), 0);
        assert_eq!(policy.gpui_sidebar_leave_shells(), 0);
        assert_eq!(policy.retained_targeting(), 13);
        assert_eq!(policy.retained_dnd(), 0);
        assert_eq!(policy.drag_shells(), 11);
        assert_eq!(policy.drag_start_models(), 11);
    }

    #[test]
    fn places_interaction_dnd_policy_keeps_single_gpui_typed_drag_boundary_explicit() {
        let policy = PlacesInteractionPolicyLog {
            row_count: 11,
            section_count: 2,
            event_delivery_policy: PlacesEventDeliveryPolicy::RetainedDnd,
        };

        assert!(PlacesEventDeliveryPolicy::RetainedDnd.retained_event_layer_enabled());
        assert!(PlacesEventDeliveryPolicy::RetainedDnd.retained_pointer_enabled());
        assert!(PlacesEventDeliveryPolicy::RetainedDnd.retained_targeting_enabled());
        assert!(PlacesEventDeliveryPolicy::RetainedDnd.retained_dnd_enabled());
        assert_eq!(
            PlacesEventDeliveryPolicy::RetainedDnd.retained_interaction(11, 2),
            13
        );
        assert_eq!(policy.retained_hitboxes(), 13);
        assert_eq!(policy.retained_probe_hitboxes(), 13);
        assert_eq!(policy.gpui_event_shells(), 1);
        assert_eq!(policy.gpui_row_section_event_shells(), 0);
        assert_eq!(policy.gpui_typed_dnd_payload_shells(), 1);
        assert_eq!(policy.gpui_sidebar_leave_shells(), 0);
        assert_eq!(policy.retained_targeting(), 13);
        assert_eq!(policy.retained_dnd(), 13);
        assert_eq!(policy.drag_shells(), 11);
        assert_eq!(policy.drag_start_models(), 11);
    }

    #[test]
    fn places_interaction_probe_does_not_claim_delivery() {
        let policy = PlacesInteractionPolicyLog {
            row_count: 11,
            section_count: 2,
            event_delivery_policy: PlacesEventDeliveryPolicy::RetainedProbe,
        };

        assert_eq!(policy.retained_hitboxes(), 0);
        assert_eq!(policy.retained_probe_hitboxes(), 13);
        assert_eq!(policy.gpui_event_shells(), 13);
        assert_eq!(policy.gpui_row_section_event_shells(), 13);
        assert_eq!(policy.gpui_typed_dnd_payload_shells(), 0);
        assert_eq!(policy.gpui_sidebar_leave_shells(), 3);
        assert_eq!(policy.drag_shells(), 11);
        assert_eq!(policy.drag_start_models(), 11);
    }

    fn place(group: &'static str, label: &str) -> PlaceSnapshot {
        PlaceSnapshot {
            index: 0,
            group,
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: label.to_string(),
            path: PathBuf::from(label),
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
        }
    }
}
