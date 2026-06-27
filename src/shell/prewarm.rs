use std::env;
use std::time::Duration;

use crate::shell::metrics::{
    DOLPHIN_MAX_BLOCK_TIMEOUT, ICON_RASTER_VISIBLE_SYNC_BUDGET, ICON_ROLE_READ_AHEAD_LIMIT,
    ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME, TEXT_LABEL_PREWARM_RASTER_MISS_BUDGET,
    TEXT_RASTER_MISS_BUDGET_PER_FRAME, VISIBLE_ICON_ROLE_PREWARM_BUDGET,
    VISIBLE_TEXT_LABEL_PREWARM_BUDGET,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextLabelPrewarmMode {
    VisibleOnly,
    DolphinReadAhead,
    ResolveAllSmallDirectory,
}

#[derive(Default)]
pub(crate) struct IconRolePrewarmStats {
    pub(crate) entries: usize,
    pub(crate) deferred: usize,
    pub(crate) read_ahead: usize,
    pub(crate) resolve_us: u128,
    pub(crate) over_budget: bool,
}

#[derive(Default)]
pub(crate) struct IconRasterPrewarmStats {
    pub(crate) entries: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) failed: usize,
    pub(crate) raster_us: u128,
    pub(crate) over_budget: bool,
}

#[derive(Default)]
pub(crate) struct TextLabelPrewarmStats {
    pub(crate) entries: usize,
    pub(crate) read_ahead: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) deferred: usize,
    pub(crate) raster_us: u128,
    pub(crate) over_budget: bool,
}

pub(crate) fn icon_raster_miss_budget_for_frame(reason: &str) -> usize {
    if let Some(budget) = env_usize("FIKA_WGPU_ICON_RASTER_MISS_BUDGET") {
        return budget;
    }
    if matches!(
        reason,
        "autosmoke-scroll" | "wheel-scroll" | "zoom" | "wheel-zoom" | "autosmoke-zoom"
    ) {
        0
    } else if visible_exact_icon_roles_enabled_for_frame(reason) {
        ICON_RASTER_VISIBLE_SYNC_BUDGET
    } else {
        0
    }
}

pub(crate) fn icon_role_prewarm_budget_for_frame(reason: &str) -> Duration {
    if visible_exact_icon_roles_enabled_for_frame(reason) {
        DOLPHIN_MAX_BLOCK_TIMEOUT
    } else {
        VISIBLE_ICON_ROLE_PREWARM_BUDGET
    }
}

pub(crate) fn visible_exact_icon_roles_enabled_for_frame(reason: &str) -> bool {
    matches!(
        reason,
        "startup"
            | "activate-directory"
            | "double-click-directory"
            | "context-open"
            | "place-open"
            | "device-mount"
            | "history-back"
            | "history-forward"
            | "parent-directory"
            | "location-commit"
            | "reload-directory"
            | "toggle-hidden"
            | "context-toggle-hidden"
            | "auto-cycle"
            | "mode-click"
            | "switch-immediate"
    )
}

pub(crate) fn icon_role_read_ahead_queue_budget_for_frame(
    reason: &str,
    small_directory_read_ahead: bool,
) -> usize {
    if matches!(reason, "zoom" | "wheel-zoom" | "autosmoke-zoom") {
        return 0;
    }
    if small_directory_read_ahead {
        ICON_ROLE_READ_AHEAD_LIMIT
    } else {
        ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME
    }
}

pub(crate) fn text_label_prewarm_mode_for_scene_prewarm(reason: &str) -> TextLabelPrewarmMode {
    if visible_exact_icon_roles_enabled_for_frame(reason) {
        TextLabelPrewarmMode::ResolveAllSmallDirectory
    } else {
        text_label_prewarm_mode_for_frame(reason)
    }
}

pub(crate) fn text_label_prewarm_mode_for_frame(reason: &str) -> TextLabelPrewarmMode {
    if matches!(
        reason,
        "autosmoke-scroll" | "wheel-scroll" | "zoom" | "wheel-zoom" | "autosmoke-zoom"
    ) {
        TextLabelPrewarmMode::VisibleOnly
    } else {
        TextLabelPrewarmMode::DolphinReadAhead
    }
}

pub(crate) fn text_label_prewarm_budget_for_mode(mode: TextLabelPrewarmMode) -> Duration {
    if mode == TextLabelPrewarmMode::ResolveAllSmallDirectory {
        DOLPHIN_MAX_BLOCK_TIMEOUT
    } else {
        VISIBLE_TEXT_LABEL_PREWARM_BUDGET
    }
}

pub(crate) fn text_label_raster_miss_budget_for_mode(mode: TextLabelPrewarmMode) -> usize {
    if mode == TextLabelPrewarmMode::VisibleOnly {
        TEXT_RASTER_MISS_BUDGET_PER_FRAME
    } else {
        TEXT_LABEL_PREWARM_RASTER_MISS_BUDGET
    }
}

pub(crate) fn default_text_raster_miss_budget() -> usize {
    env_usize("FIKA_WGPU_TEXT_RASTER_MISS_BUDGET").unwrap_or(TEXT_RASTER_MISS_BUDGET_PER_FRAME)
}

fn env_usize(key: &str) -> Option<usize> {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prewarm_modes_keep_scroll_and_zoom_visible_only() {
        assert_eq!(
            text_label_prewarm_mode_for_frame("wheel-scroll"),
            TextLabelPrewarmMode::VisibleOnly
        );
        assert_eq!(
            text_label_prewarm_mode_for_frame("wheel-zoom"),
            TextLabelPrewarmMode::VisibleOnly
        );
    }

    #[test]
    fn scene_prewarm_resolves_small_directories_for_navigation_reasons() {
        assert!(visible_exact_icon_roles_enabled_for_frame(
            "activate-directory"
        ));
        assert_eq!(
            text_label_prewarm_mode_for_scene_prewarm("activate-directory"),
            TextLabelPrewarmMode::ResolveAllSmallDirectory
        );
    }
}
