use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Div, ParentElement, Rgba, Stateful, Styled, div, px, rgb};

use super::state::SpaceInfoSnapshot;
use super::{fixed_status_text, status_section};

pub(super) fn space_info(pane_id: PaneId, space: Option<SpaceInfoSnapshot>) -> Stateful<Div> {
    match space {
        Some(space) => {
            let used_width = space_usage_width(space.used_percent);
            status_section()
                .id(format!("status-space-info-{}", pane_id.0))
                .gap_2()
                .text_xs()
                .text_color(rgb(0x59636e))
                .child(fixed_status_text(104.0, space.free_label))
                .child(
                    div()
                        .relative()
                        .w(px(72.0))
                        .min_w_0()
                        .flex_shrink_1()
                        .h(px(6.0))
                        .rounded_md()
                        .bg(rgb(0xe6e9ef))
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .w(px(used_width))
                                .h(px(6.0))
                                .rounded_md()
                                .bg(space_usage_color(space.used_percent)),
                        ),
                )
                .child(fixed_status_text(152.0, space.detail_label).text_color(rgb(0x7a8494)))
        }
        None => status_section()
            .id(format!("status-space-info-{}", pane_id.0))
            .gap_2()
            .text_xs()
            .text_color(rgb(0x7a8494))
            .child(fixed_status_text(104.0, "Space unavailable"))
            .child(
                div()
                    .relative()
                    .w(px(72.0))
                    .min_w_0()
                    .flex_shrink_1()
                    .h(px(6.0))
                    .rounded_md()
                    .bg(rgb(0xe6e9ef)),
            )
            .child(fixed_status_text(152.0, "").text_color(rgb(0x7a8494))),
    }
}

fn space_usage_width(percent: u8) -> f32 {
    72.0 * f32::from(percent) / 100.0
}

fn space_usage_color(percent: u8) -> Rgba {
    if percent >= 90 {
        rgb(0xb42318)
    } else if percent >= 75 {
        rgb(0xb54708)
    } else {
        rgb(0x2f6fed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_usage_width_follows_meter_width() {
        assert_eq!(space_usage_width(0), 0.0);
        assert_eq!(space_usage_width(50), 36.0);
        assert_eq!(space_usage_width(100), 72.0);
    }
}
