use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

#[cfg(target_os = "linux")]
use winit::platform::wayland::{
    ActiveEventLoopExtWayland, WindowAttributesWayland, WindowExtWayland,
};

const FIKA_WAYLAND_APP_ID: &str = "fika";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellDialogWindowRole {
    Create,
    OpenWith,
    Rename,
}

impl ShellDialogWindowRole {
    fn wayland_instance(self) -> &'static str {
        match self {
            Self::Create => "fika-create-dialog",
            Self::OpenWith => "fika-open-with-dialog",
            Self::Rename => "fika-rename-dialog",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellWaylandDialogParentStatus {
    NotWayland,
    MissingToplevel,
    WinitParentApiUnavailable,
}

pub(crate) fn wayland_dialog_parent_status(
    event_loop: &dyn ActiveEventLoop,
    parent: Option<&dyn Window>,
    dialog: &dyn Window,
) -> ShellWaylandDialogParentStatus {
    detect_wayland_dialog_parent_status(event_loop, parent, dialog)
}

#[cfg(target_os = "linux")]
fn detect_wayland_dialog_parent_status(
    event_loop: &dyn ActiveEventLoop,
    parent: Option<&dyn Window>,
    dialog: &dyn Window,
) -> ShellWaylandDialogParentStatus {
    if !event_loop.is_wayland() {
        return ShellWaylandDialogParentStatus::NotWayland;
    }
    if parent.and_then(WindowExtWayland::xdg_toplevel).is_none() || dialog.xdg_toplevel().is_none()
    {
        return ShellWaylandDialogParentStatus::MissingToplevel;
    }

    // winit exposes the xdg_toplevel proxy pointer, but not a safe parent-binding API
    // or the owned queue/connection needed to issue set_parent here.
    ShellWaylandDialogParentStatus::WinitParentApiUnavailable
}

#[cfg(not(target_os = "linux"))]
fn detect_wayland_dialog_parent_status(
    _event_loop: &dyn ActiveEventLoop,
    _parent: Option<&dyn Window>,
    _dialog: &dyn Window,
) -> ShellWaylandDialogParentStatus {
    ShellWaylandDialogParentStatus::NotWayland
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
    }

    #[test]
    fn main_window_uses_stable_wayland_instance() {
        assert_eq!(ShellWindowRole::Main.wayland_instance(), "fika-main");
    }
}
