use std::path::PathBuf;

use crate::FikaApp;
use crate::ui::shortcuts::PlaceInputAction;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkAuthDraft {
    pub(crate) pane_id: fika_core::PaneId,
    pub(crate) path: PathBuf,
    pub(crate) uri: String,
    pub(crate) message: String,
    pub(crate) username: String,
    pub(crate) domain: String,
    pub(crate) password: String,
    pub(crate) focus: NetworkAuthField,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkAuthField {
    Username,
    Domain,
    Password,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkAuthInputResult {
    Cancel,
    Commit,
    Edited,
    Ignore,
}

pub(crate) fn apply_network_auth_input_action(
    draft: &mut NetworkAuthDraft,
    action: PlaceInputAction,
) -> NetworkAuthInputResult {
    match action {
        PlaceInputAction::Cancel => NetworkAuthInputResult::Cancel,
        PlaceInputAction::Commit => NetworkAuthInputResult::Commit,
        PlaceInputAction::NextField => {
            draft.focus = draft.focus.next();
            NetworkAuthInputResult::Edited
        }
        PlaceInputAction::Backspace => {
            draft.backspace();
            draft.error = None;
            NetworkAuthInputResult::Edited
        }
        PlaceInputAction::Insert(text) => {
            draft.insert_text(&text);
            draft.error = None;
            NetworkAuthInputResult::Edited
        }
        PlaceInputAction::Ignore => NetworkAuthInputResult::Ignore,
    }
}

pub(crate) fn network_auth_overlay(
    draft: NetworkAuthDraft,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id("network-auth-layer")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .bg(rgba(0x00000066))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_network_auth_draft();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .id("network-auth-dialog")
                .w(px(480.0))
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0xd5d9df))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(0x1f2328))
                        .child("Network Authentication"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .px_4()
                        .py_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x59636e))
                                .child(draft.message.clone()),
                        )
                        .child(network_auth_field(
                            NetworkAuthField::Username,
                            "Username",
                            draft.username,
                            false,
                            draft.focus == NetworkAuthField::Username,
                            cx,
                        ))
                        .child(network_auth_field(
                            NetworkAuthField::Domain,
                            "Domain",
                            draft.domain,
                            false,
                            draft.focus == NetworkAuthField::Domain,
                            cx,
                        ))
                        .child(network_auth_field(
                            NetworkAuthField::Password,
                            "Password",
                            draft.password,
                            true,
                            draft.focus == NetworkAuthField::Password,
                            cx,
                        ))
                        .when_some(draft.error, |panel, error| {
                            panel.child(div().text_sm().text_color(rgb(0xb42318)).child(error))
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .pt_1()
                                .child(
                                    dialog_button("cancel", "Cancel").on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            |this, _event: &gpui::MouseDownEvent, _window, cx| {
                                                this.dismiss_network_auth_draft();
                                                cx.stop_propagation();
                                                cx.notify();
                                            },
                                        ),
                                    ),
                                )
                                .child(
                                    dialog_button("connect", "Connect")
                                        .bg(rgb(0x2f6fed))
                                        .text_color(rgb(0xffffff))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this,
                                                 _event: &gpui::MouseDownEvent,
                                                 _window,
                                                 cx| {
                                                    this.commit_network_auth_draft();
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                },
                                            ),
                                        ),
                                ),
                        ),
                ),
        )
}

impl NetworkAuthDraft {
    pub(crate) fn new(
        pane_id: fika_core::PaneId,
        path: PathBuf,
        uri: String,
        message: String,
        default_username: Option<String>,
        default_domain: Option<String>,
    ) -> Self {
        Self {
            pane_id,
            path,
            uri,
            message,
            username: default_username.unwrap_or_default(),
            domain: default_domain.unwrap_or_default(),
            password: String::new(),
            focus: NetworkAuthField::Password,
            error: None,
        }
    }

    pub(crate) fn to_auth(&self) -> fika_core::NetworkAuth {
        fika_core::NetworkAuth {
            username: non_empty_string(&self.username),
            domain: non_empty_string(&self.domain),
            password: Some(self.password.clone()),
            anonymous: false,
            remember: true,
        }
    }

    fn backspace(&mut self) {
        match self.focus {
            NetworkAuthField::Username => {
                self.username.pop();
            }
            NetworkAuthField::Domain => {
                self.domain.pop();
            }
            NetworkAuthField::Password => {
                self.password.pop();
            }
        }
    }

    fn insert_text(&mut self, text: &str) {
        match self.focus {
            NetworkAuthField::Username => self.username.push_str(text),
            NetworkAuthField::Domain => self.domain.push_str(text),
            NetworkAuthField::Password => self.password.push_str(text),
        }
    }
}

impl NetworkAuthField {
    fn next(self) -> Self {
        match self {
            Self::Username => Self::Domain,
            Self::Domain => Self::Password,
            Self::Password => Self::Username,
        }
    }
}

fn network_auth_field(
    field: NetworkAuthField,
    label: &'static str,
    value: String,
    password: bool,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let mut display_value = if password {
        "*".repeat(value.len())
    } else {
        value
    };
    if focused {
        display_value.push('|');
    }
    div()
        .id(format!("network-auth-field-{field:?}"))
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(rgb(0x6b7280)).child(label))
        .child(
            div()
                .min_h(px(30.0))
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(if focused {
                    rgb(0x2f6fed)
                } else {
                    rgb(0xc8ced6)
                })
                .bg(if focused {
                    rgb(0xf3f7ff)
                } else {
                    rgb(0xffffff)
                })
                .text_sm()
                .text_color(rgb(0x24292f))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.set_network_auth_draft_focus(field);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(display_value),
        )
}

fn dialog_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(format!("network-auth-{id}"))
        .px_3()
        .py_1()
        .rounded_md()
        .text_sm()
        .text_color(rgb(0x1f2328))
        .border_1()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xffffff))
        .hover(|button| button.bg(rgb(0xeaf1ff)))
        .cursor_pointer()
        .child(label)
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_auth_draft_uses_defaults_and_masks_password_value() {
        let draft = NetworkAuthDraft::new(
            fika_core::PaneId(1),
            PathBuf::from("smb://server/share/"),
            "smb://server/share/".to_string(),
            "Password required".to_string(),
            Some("yk".to_string()),
            Some("WORKGROUP".to_string()),
        );

        assert_eq!(draft.username, "yk");
        assert_eq!(draft.domain, "WORKGROUP");
        assert_eq!(draft.focus, NetworkAuthField::Password);
    }

    #[test]
    fn network_auth_input_edits_focused_field_and_builds_auth() {
        let mut draft = NetworkAuthDraft::new(
            fika_core::PaneId(1),
            PathBuf::from("smb://server/share/"),
            "smb://server/share/".to_string(),
            "Password required".to_string(),
            None,
            None,
        );

        assert_eq!(
            apply_network_auth_input_action(
                &mut draft,
                PlaceInputAction::Insert("secret".to_string())
            ),
            NetworkAuthInputResult::Edited
        );
        assert_eq!(draft.password, "secret");
        assert_eq!(
            apply_network_auth_input_action(&mut draft, PlaceInputAction::NextField),
            NetworkAuthInputResult::Edited
        );
        assert_eq!(draft.focus, NetworkAuthField::Username);
        assert_eq!(
            apply_network_auth_input_action(&mut draft, PlaceInputAction::Insert("yk".to_string())),
            NetworkAuthInputResult::Edited
        );

        let auth = draft.to_auth();
        assert_eq!(auth.username.as_deref(), Some("yk"));
        assert_eq!(auth.password.as_deref(), Some("secret"));
        assert!(auth.remember);
    }
}
