use cosmic_text::Color as TextColor;

use crate::shell::tasks::ShellTaskStatusKind;

pub(crate) type UiColor = [f32; 4];

pub(crate) const POPUP_BACKDROP: UiColor = [0.047, 0.051, 0.056, 0.30];
pub(crate) const POPUP_SURFACE: UiColor = [0.990, 0.991, 0.988, 0.99];
pub(crate) const POPUP_HEADER: UiColor = [0.965, 0.969, 0.973, 1.0];
pub(crate) const POPUP_PANEL: UiColor = [0.976, 0.979, 0.978, 1.0];
pub(crate) const POPUP_INPUT: UiColor = [1.000, 1.000, 1.000, 1.0];
pub(crate) const POPUP_BORDER: UiColor = [0.706, 0.722, 0.741, 1.0];
pub(crate) const POPUP_DIVIDER: UiColor = [0.855, 0.863, 0.873, 1.0];
pub(crate) const POPUP_ROW_ALT: UiColor = [0.974, 0.976, 0.972, 1.0];
pub(crate) const POPUP_BUTTON_SECONDARY: UiColor = [0.941, 0.944, 0.941, 1.0];
pub(crate) const POPUP_BUTTON_PRIMARY: UiColor = [0.119, 0.392, 0.635, 1.0];
pub(crate) const POPUP_BUTTON_PRIMARY_SOFT: UiColor = [0.176, 0.459, 0.620, 1.0];
pub(crate) const POPUP_BUTTON_WARNING: UiColor = [0.718, 0.404, 0.133, 1.0];
pub(crate) const POPUP_BUTTON_DANGER: UiColor = [0.700, 0.235, 0.188, 1.0];
pub(crate) const POPUP_STATUS_RUNNING: UiColor = [0.119, 0.392, 0.635, 1.0];
pub(crate) const POPUP_STATUS_COMPLETED: UiColor = [0.090, 0.506, 0.286, 1.0];
pub(crate) const POPUP_STATUS_FAILED: UiColor = [0.765, 0.235, 0.188, 1.0];
pub(crate) const POPUP_STATUS_CANCELLED: UiColor = [0.408, 0.459, 0.522, 1.0];
pub(crate) const POPUP_FIELD_FOCUS: UiColor = [0.239, 0.502, 0.710, 1.0];
pub(crate) const POPUP_MARKER_NEUTRAL: UiColor = [0.608, 0.674, 0.753, 1.0];
pub(crate) const POPUP_WARNING_HEADER: UiColor = [1.000, 0.957, 0.890, 1.0];
pub(crate) const POPUP_WARNING_DIVIDER: UiColor = [0.906, 0.737, 0.490, 1.0];

pub(crate) fn popup_title_text() -> TextColor {
    TextColor::rgb(31, 37, 45)
}

pub(crate) fn popup_body_text() -> TextColor {
    TextColor::rgb(48, 56, 66)
}

pub(crate) fn popup_muted_text() -> TextColor {
    TextColor::rgb(100, 113, 128)
}

pub(crate) fn popup_soft_text() -> TextColor {
    TextColor::rgb(74, 86, 100)
}

pub(crate) fn popup_inverse_text() -> TextColor {
    TextColor::rgb(255, 255, 255)
}

pub(crate) fn popup_error_text() -> TextColor {
    TextColor::rgb(177, 54, 43)
}

pub(crate) fn popup_warning_text() -> TextColor {
    TextColor::rgb(92, 55, 18)
}

pub(crate) fn popup_status_text(kind: ShellTaskStatusKind) -> TextColor {
    match kind {
        ShellTaskStatusKind::Running => TextColor::rgb(46, 103, 168),
        ShellTaskStatusKind::Completed => TextColor::rgb(31, 132, 79),
        ShellTaskStatusKind::Failed => popup_error_text(),
        ShellTaskStatusKind::Cancelled => popup_muted_text(),
    }
}
