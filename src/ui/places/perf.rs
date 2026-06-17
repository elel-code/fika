use std::env;
use std::time::Duration;

use super::PlacePaintSlotPerfLog;
use super::PlaceSnapshot;

const PERF_PLACES_VIEW_ENV: &str = "FIKA_PERF_PLACES_VIEW";
const CUSTOM_PLACES_ROWS_ENV: &str = "FIKA_CUSTOM_PLACES_ROWS";

pub(crate) fn places_perf_enabled() -> bool {
    env::var(PERF_PLACES_VIEW_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

pub(crate) fn custom_places_rows_enabled() -> bool {
    env::var(CUSTOM_PLACES_ROWS_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

pub(crate) fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
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
    pub(crate) prepaint_elapsed: Duration,
    pub(crate) paint_elapsed: Duration,
}

pub(crate) fn emit_places_row_visual_perf_log(log: PlacesRowVisualPerfLog) {
    eprintln!(
        "[fika places-row-visual] rows={} prepaint={}us paint={}us",
        log.rows,
        log.prepaint_elapsed.as_micros(),
        log.paint_elapsed.as_micros(),
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
pub(crate) struct PlacesRendererPolicyLog {
    pub(crate) row_count: usize,
    pub(crate) section_count: usize,
    pub(crate) custom_row_visuals: bool,
    pub(crate) scrollbar_canvas_count: usize,
}

pub(crate) fn emit_places_renderer_policy_log(log: PlacesRendererPolicyLog) {
    let row_gpui = if log.custom_row_visuals {
        0
    } else {
        log.row_count
    };
    let row_visual_layer = if log.custom_row_visuals {
        log.row_count
    } else {
        0
    };
    eprintln!(
        "[fika places-renderer-policy] rows={} row_gpui={} row_visual_layer={} icon_gpui={} retained_interaction=0 drag_shell={} section_gpui={} scrollbar_canvas={}",
        log.row_count,
        row_gpui,
        row_visual_layer,
        log.row_count,
        log.row_count,
        log.section_count,
        log.scrollbar_canvas_count,
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
