use std::env;
use std::time::Duration;

use fika_core::ZoomChange;
use gpui::{ScrollDelta, point, px};

const AUTOSMOKE_ITEM_VIEW_ENV: &str = "FIKA_AUTOSMOKE_ITEM_VIEW";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemViewAutosmokeScenario {
    Zoom,
    Scroll,
    ZoomScroll,
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
}

impl ItemViewAutosmokeScenario {
    pub(crate) fn from_env() -> Option<Self> {
        item_view_autosmoke_scenario_from_value(&env::var(AUTOSMOKE_ITEM_VIEW_ENV).ok()?)
    }

    pub(crate) fn start_delay(self) -> Duration {
        let _ = self;
        Duration::from_millis(1200)
    }

    pub(crate) fn action_delay(self) -> Duration {
        let _ = self;
        Duration::from_millis(180)
    }

    pub(crate) fn actions(self) -> Vec<ItemViewAutosmokeAction> {
        let mut actions = Vec::new();
        if matches!(self, Self::Zoom | Self::ZoomScroll) {
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
        if matches!(self, Self::Scroll | Self::ZoomScroll) {
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
        actions
    }
}

fn item_view_autosmoke_scenario_from_value(value: &str) -> Option<ItemViewAutosmokeScenario> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "zoom-scroll" | "scroll-zoom" => {
            Some(ItemViewAutosmokeScenario::ZoomScroll)
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
    fn zoom_scroll_scenario_contains_expected_action_count() {
        let actions = ItemViewAutosmokeScenario::ZoomScroll.actions();

        assert_eq!(actions.len(), 8);
        assert!(matches!(actions[0], ItemViewAutosmokeAction::Zoom { .. }));
        assert!(matches!(actions[4], ItemViewAutosmokeAction::Scroll { .. }));
    }
}
