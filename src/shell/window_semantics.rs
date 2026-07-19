use winit::event_loop::ActiveEventLoop;
use winit::window::WindowAttributes;

#[cfg(target_os = "linux")]
use winit::platform::wayland::{ActiveEventLoopExtWayland, WindowAttributesWayland};

const FIKA_WAYLAND_APP_ID: &str = "fika";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellDialogWindowRole {
    Create,
    OpenWith,
    Properties,
    Rename,
    TaskDetail,
    TrashConflict,
}

impl ShellDialogWindowRole {
    fn wayland_instance(self) -> &'static str {
        match self {
            Self::Create => "fika-create-dialog",
            Self::OpenWith => "fika-open-with-dialog",
            Self::Properties => "fika-properties-dialog",
            Self::Rename => "fika-rename-dialog",
            Self::TaskDetail => "fika-task-detail-dialog",
            Self::TrashConflict => "fika-trash-conflict-dialog",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellWindowRole {
    Main,
    Dialog(ShellDialogWindowRole),
}

impl ShellWindowRole {
    fn wayland_instance(self) -> &'static str {
        match self {
            Self::Main => "fika-main",
            Self::Dialog(role) => role.wayland_instance(),
        }
    }
}

pub(crate) fn apply_window_platform_semantics(
    event_loop: &dyn ActiveEventLoop,
    attrs: WindowAttributes,
    role: ShellWindowRole,
) -> WindowAttributes {
    apply_wayland_window_semantics(event_loop, attrs, role)
}

#[cfg(target_os = "linux")]
fn apply_wayland_window_semantics(
    event_loop: &dyn ActiveEventLoop,
    attrs: WindowAttributes,
    role: ShellWindowRole,
) -> WindowAttributes {
    if !event_loop.is_wayland() {
        return attrs;
    }

    attrs.with_platform_attributes(Box::new(
        WindowAttributesWayland::default()
            .with_name(FIKA_WAYLAND_APP_ID, role.wayland_instance())
            .with_prefer_csd(false),
    ))
}

#[cfg(not(target_os = "linux"))]
fn apply_wayland_window_semantics(
    _event_loop: &dyn ActiveEventLoop,
    attrs: WindowAttributes,
    _role: ShellWindowRole,
) -> WindowAttributes {
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_roles_have_distinct_wayland_instances() {
        assert_eq!(
            ShellDialogWindowRole::Create.wayland_instance(),
            "fika-create-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::OpenWith.wayland_instance(),
            "fika-open-with-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Rename.wayland_instance(),
            "fika-rename-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Properties.wayland_instance(),
            "fika-properties-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::TaskDetail.wayland_instance(),
            "fika-task-detail-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::TrashConflict.wayland_instance(),
            "fika-trash-conflict-dialog"
        );
    }

    #[test]
    fn main_window_uses_stable_wayland_instance() {
        assert_eq!(ShellWindowRole::Main.wayland_instance(), "fika-main");
    }
}
