use std::env;
use std::time::Duration;

use fika_core::{PaneId, ViewMode, ZoomChange};
use gpui::{Context, ScrollDelta, point, px};

use crate::FikaApp;

const AUTOSMOKE_ITEM_VIEW_ENV: &str = "FIKA_AUTOSMOKE_ITEM_VIEW";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemViewAutosmokeScenario {
    Zoom,
    Scroll,
    ZoomScroll,
    IconsZoomScroll,
    DetailsZoomScroll,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ItemViewAutosmokeAction {
    Zoom {
        label: &'static str,
        change: ZoomChange,
    },
    Scroll {
        label: &'static str,
        delta: ScrollDelta,
    },
    ViewMode {
        label: &'static str,
        mode: ViewMode,
    },
    Settle {
        label: &'static str,
    },
}

impl ItemViewAutosmokeScenario {
    pub(crate) fn from_env() -> Option<Self> {
        item_view_autosmoke_scenario_from_value(&env::var(AUTOSMOKE_ITEM_VIEW_ENV).ok()?)
    }

    fn marker_label(self) -> &'static str {
        match self {
            Self::Zoom => "Zoom",
            Self::Scroll => "Scroll",
            Self::ZoomScroll => "ZoomScroll",
            Self::IconsZoomScroll => "IconsZoomScroll",
            Self::DetailsZoomScroll => "DetailsZoomScroll",
        }
    }

    pub(crate) fn start_delay(self) -> Duration {
        let _ = self;
        Duration::from_millis(1200)
    }

    pub(crate) fn action_delay(self) -> Duration {
        let _ = self;
        Duration::from_millis(350)
    }

    pub(crate) fn actions(self) -> Vec<ItemViewAutosmokeAction> {
        let mut actions = Vec::new();
        if matches!(self, Self::IconsZoomScroll) {
            actions.push(ItemViewAutosmokeAction::ViewMode {
                label: "view-icons",
                mode: ViewMode::Icons,
            });
        }
        if matches!(self, Self::DetailsZoomScroll) {
            actions.push(ItemViewAutosmokeAction::ViewMode {
                label: "view-details",
                mode: ViewMode::Details,
            });
            actions.push(ItemViewAutosmokeAction::Settle {
                label: "settle-details",
            });
        }
        if matches!(
            self,
            Self::Zoom | Self::ZoomScroll | Self::IconsZoomScroll | Self::DetailsZoomScroll
        ) {
            actions.extend([
                ItemViewAutosmokeAction::Zoom {
                    label: "zoom-in",
                    change: ZoomChange::In,
                },
                ItemViewAutosmokeAction::Zoom {
                    label: "zoom-in",
                    change: ZoomChange::In,
                },
                ItemViewAutosmokeAction::Zoom {
                    label: "zoom-out",
                    change: ZoomChange::Out,
                },
                ItemViewAutosmokeAction::Zoom {
                    label: "zoom-out",
                    change: ZoomChange::Out,
                },
            ]);
        }
        if matches!(
            self,
            Self::Scroll | Self::ZoomScroll | Self::IconsZoomScroll | Self::DetailsZoomScroll
        ) {
            actions.extend([
                ItemViewAutosmokeAction::Scroll {
                    label: "scroll-forward",
                    delta: ScrollDelta::Pixels(point(px(0.0), px(-260.0))),
                },
                ItemViewAutosmokeAction::Scroll {
                    label: "scroll-forward",
                    delta: ScrollDelta::Pixels(point(px(0.0), px(-260.0))),
                },
                ItemViewAutosmokeAction::Scroll {
                    label: "scroll-back",
                    delta: ScrollDelta::Pixels(point(px(0.0), px(260.0))),
                },
                ItemViewAutosmokeAction::Scroll {
                    label: "scroll-back",
                    delta: ScrollDelta::Pixels(point(px(0.0), px(260.0))),
                },
            ]);
        }
        if matches!(self, Self::DetailsZoomScroll) {
            actions.extend([
                ItemViewAutosmokeAction::Settle {
                    label: "settle-details",
                },
                ItemViewAutosmokeAction::Settle {
                    label: "settle-details",
                },
            ]);
        }
        actions
    }
}

pub(crate) fn start_item_view_autosmoke(
    pane_id: PaneId,
    scenario: ItemViewAutosmokeScenario,
    cx: &mut Context<FikaApp>,
) {
    cx.spawn(
        move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                emit_item_view_autosmoke_start(pane_id, scenario);
                cx.background_executor().timer(scenario.start_delay()).await;

                for action in scenario.actions() {
                    match action {
                        ItemViewAutosmokeAction::Zoom { label, change } => {
                            if this
                                .update(&mut cx, |app, cx| {
                                    emit_item_view_autosmoke_zoom_action(label, pane_id);
                                    app.apply_zoom_change_with_context(pane_id, change, cx);
                                    cx.notify();
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        ItemViewAutosmokeAction::Scroll { label, delta } => {
                            if this
                                .update(&mut cx, |app, cx| {
                                    let changed = app.scroll_pane_from_wheel(pane_id, delta);
                                    emit_item_view_autosmoke_scroll_action(label, pane_id, changed);
                                    if changed {
                                        cx.notify();
                                    }
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        ItemViewAutosmokeAction::ViewMode { label, mode } => {
                            if this
                                .update(&mut cx, |app, cx| {
                                    app.set_pane_view_mode(pane_id, mode);
                                    emit_item_view_autosmoke_mode_action(label, pane_id, mode);
                                    cx.notify();
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        ItemViewAutosmokeAction::Settle { label } => {
                            if this
                                .update(&mut cx, |_app, cx| {
                                    emit_item_view_autosmoke_settle_action(label, pane_id);
                                    cx.notify();
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    cx.background_executor()
                        .timer(scenario.action_delay())
                        .await;
                }

                emit_item_view_autosmoke_complete(pane_id, scenario);
            }
        },
    )
    .detach();
}

fn emit_item_view_autosmoke_start(pane_id: PaneId, scenario: ItemViewAutosmokeScenario) {
    eprintln!(
        "[fika autosmoke] item-view start pane={} scenario={}",
        pane_id.0,
        scenario.marker_label()
    );
}

fn emit_item_view_autosmoke_complete(pane_id: PaneId, scenario: ItemViewAutosmokeScenario) {
    eprintln!(
        "[fika autosmoke] item-view complete pane={} scenario={}",
        pane_id.0,
        scenario.marker_label()
    );
}

fn emit_item_view_autosmoke_zoom_action(label: &'static str, pane_id: PaneId) {
    eprintln!(
        "[fika autosmoke] item-view action={} pane={}",
        label, pane_id.0
    );
}

fn emit_item_view_autosmoke_scroll_action(label: &'static str, pane_id: PaneId, changed: bool) {
    eprintln!(
        "[fika autosmoke] item-view action={} pane={} changed={}",
        label, pane_id.0, changed
    );
}

fn emit_item_view_autosmoke_mode_action(label: &'static str, pane_id: PaneId, mode: ViewMode) {
    eprintln!(
        "[fika autosmoke] item-view action={} pane={} mode={:?}",
        label, pane_id.0, mode
    );
}

fn emit_item_view_autosmoke_settle_action(label: &'static str, pane_id: PaneId) {
    eprintln!(
        "[fika autosmoke] item-view action={} pane={}",
        label, pane_id.0
    );
}

fn item_view_autosmoke_scenario_from_value(value: &str) -> Option<ItemViewAutosmokeScenario> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "zoom-scroll" | "scroll-zoom" => {
            Some(ItemViewAutosmokeScenario::ZoomScroll)
        }
        "details-zoom-scroll" | "details-scroll-zoom" | "details" => {
            Some(ItemViewAutosmokeScenario::DetailsZoomScroll)
        }
        "icons-zoom-scroll" | "icons-scroll-zoom" | "icons" => {
            Some(ItemViewAutosmokeScenario::IconsZoomScroll)
        }
        "zoom" => Some(ItemViewAutosmokeScenario::Zoom),
        "scroll" => Some(ItemViewAutosmokeScenario::Scroll),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autosmoke_scenario_values() {
        assert_eq!(
            item_view_autosmoke_scenario_from_value("zoom-scroll"),
            Some(ItemViewAutosmokeScenario::ZoomScroll)
        );
        assert_eq!(
            item_view_autosmoke_scenario_from_value("details-zoom-scroll"),
            Some(ItemViewAutosmokeScenario::DetailsZoomScroll)
        );
        assert_eq!(
            item_view_autosmoke_scenario_from_value("icons-zoom-scroll"),
            Some(ItemViewAutosmokeScenario::IconsZoomScroll)
        );
        assert_eq!(
            item_view_autosmoke_scenario_from_value("1"),
            Some(ItemViewAutosmokeScenario::ZoomScroll)
        );
        assert_eq!(
            item_view_autosmoke_scenario_from_value("zoom"),
            Some(ItemViewAutosmokeScenario::Zoom)
        );
        assert_eq!(
            item_view_autosmoke_scenario_from_value("scroll"),
            Some(ItemViewAutosmokeScenario::Scroll)
        );
        assert_eq!(item_view_autosmoke_scenario_from_value("off"), None);
    }

    #[test]
    fn scenario_marker_labels_match_runtime_markers() {
        assert_eq!(ItemViewAutosmokeScenario::Zoom.marker_label(), "Zoom");
        assert_eq!(ItemViewAutosmokeScenario::Scroll.marker_label(), "Scroll");
        assert_eq!(
            ItemViewAutosmokeScenario::ZoomScroll.marker_label(),
            "ZoomScroll"
        );
        assert_eq!(
            ItemViewAutosmokeScenario::IconsZoomScroll.marker_label(),
            "IconsZoomScroll"
        );
        assert_eq!(
            ItemViewAutosmokeScenario::DetailsZoomScroll.marker_label(),
            "DetailsZoomScroll"
        );
    }

    #[test]
    fn zoom_scroll_scenario_contains_expected_action_count() {
        let actions = ItemViewAutosmokeScenario::ZoomScroll.actions();

        assert_eq!(actions.len(), 8);
        assert!(matches!(actions[0], ItemViewAutosmokeAction::Zoom { .. }));
        assert!(matches!(actions[4], ItemViewAutosmokeAction::Scroll { .. }));
    }

    #[test]
    fn details_zoom_scroll_scenario_switches_mode_before_actions() {
        let actions = ItemViewAutosmokeScenario::DetailsZoomScroll.actions();

        assert_eq!(actions.len(), 12);
        assert!(matches!(
            actions[0],
            ItemViewAutosmokeAction::ViewMode {
                mode: ViewMode::Details,
                ..
            }
        ));
        assert!(matches!(actions[1], ItemViewAutosmokeAction::Settle { .. }));
        assert!(matches!(actions[2], ItemViewAutosmokeAction::Zoom { .. }));
        assert!(matches!(actions[6], ItemViewAutosmokeAction::Scroll { .. }));
        assert!(matches!(
            actions[10],
            ItemViewAutosmokeAction::Settle { .. }
        ));
        assert!(matches!(
            actions[11],
            ItemViewAutosmokeAction::Settle { .. }
        ));
    }

    #[test]
    fn icons_zoom_scroll_scenario_switches_mode_before_actions() {
        let actions = ItemViewAutosmokeScenario::IconsZoomScroll.actions();

        assert_eq!(actions.len(), 9);
        assert!(matches!(
            actions[0],
            ItemViewAutosmokeAction::ViewMode {
                mode: ViewMode::Icons,
                ..
            }
        ));
        assert!(matches!(actions[1], ItemViewAutosmokeAction::Zoom { .. }));
        assert!(matches!(actions[5], ItemViewAutosmokeAction::Scroll { .. }));
    }
}
