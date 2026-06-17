use std::env;
use std::time::Duration;

use super::PlaceSnapshot;

const PERF_PLACES_VIEW_ENV: &str = "FIKA_PERF_PLACES_VIEW";

pub(crate) fn places_perf_enabled() -> bool {
    env::var(PERF_PLACES_VIEW_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

fn env_flag_is_truthy(value: &str) -> bool {
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
pub(crate) struct PlacesRendererPolicyLog {
    pub(crate) row_count: usize,
    pub(crate) section_count: usize,
    pub(crate) scrollbar_canvas_count: usize,
}

pub(crate) fn emit_places_renderer_policy_log(log: PlacesRendererPolicyLog) {
    eprintln!(
        "[fika places-renderer-policy] rows={} row_gpui={} row_visual_layer=0 icon_gpui={} retained_interaction=0 drag_shell={} section_gpui={} scrollbar_canvas={}",
        log.row_count,
        log.row_count,
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
