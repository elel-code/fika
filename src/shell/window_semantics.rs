use winit::event::{ElementState, WindowEvent};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellModalWindowEventDisposition {
    Pass,
    Block,
    BlockAndRequestAttention,
}

impl ShellModalWindowEventDisposition {
    pub(crate) fn blocks(self) -> bool {
        matches!(self, Self::Block | Self::BlockAndRequestAttention)
    }

    pub(crate) fn requests_attention(self) -> bool {
        matches!(self, Self::BlockAndRequestAttention)
    }
}

pub(crate) fn modal_window_event_disposition(
    event: &WindowEvent,
) -> ShellModalWindowEventDisposition {
    if !main_window_event_blocked_by_modal_dialog(event) {
        return ShellModalWindowEventDisposition::Pass;
    }
    if main_window_event_requests_modal_attention(event) {
        ShellModalWindowEventDisposition::BlockAndRequestAttention
    } else {
        ShellModalWindowEventDisposition::Block
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellWindowCloseRequestTarget {
    Main,
    RecentlyClosedDialog,
}

pub(crate) fn window_manager_close_request_exits_application(
    target: ShellWindowCloseRequestTarget,
    modal_dialog_open: bool,
) -> bool {
    match target {
        ShellWindowCloseRequestTarget::Main => true,
        ShellWindowCloseRequestTarget::RecentlyClosedDialog => !modal_dialog_open,
    }
}

fn main_window_event_blocked_by_modal_dialog(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::DragEntered { .. }
            | WindowEvent::DragPosition { .. }
            | WindowEvent::DragDropped { .. }
            | WindowEvent::DragLeft { .. }
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::PointerMoved { .. }
            | WindowEvent::PointerEntered { .. }
            | WindowEvent::PointerLeft { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::PointerButton { .. }
            | WindowEvent::HoldGesture { .. }
            | WindowEvent::PinchGesture { .. }
            | WindowEvent::PanGesture { .. }
            | WindowEvent::DoubleTapGesture { .. }
            | WindowEvent::RotationGesture { .. }
            | WindowEvent::TouchpadPressure { .. }
    )
}

fn main_window_event_requests_modal_attention(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::DragDropped { .. } => true,
        WindowEvent::KeyboardInput {
            event,
            is_synthetic: false,
            ..
        } => event.state == ElementState::Pressed,
        WindowEvent::PointerButton {
            state: ElementState::Pressed,
            ..
        } => true,
        _ => false,
    }
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

    #[test]
    fn modal_event_policy_passes_window_lifecycle_events() {
        assert_eq!(
            modal_window_event_disposition(&WindowEvent::CloseRequested),
            ShellModalWindowEventDisposition::Pass
        );
        assert_eq!(
            modal_window_event_disposition(&WindowEvent::RedrawRequested),
            ShellModalWindowEventDisposition::Pass
        );
    }

    #[test]
    fn modal_event_policy_blocks_passive_input_without_attention() {
        assert_eq!(
            modal_window_event_disposition(&WindowEvent::DragPosition {
                id: winit::data_transfer::DataTransferId::from_raw(1),
                position: (10.0, 20.0).into(),
                proposed_action: None,
            }),
            ShellModalWindowEventDisposition::Block
        );
    }

    #[test]
    fn modal_event_policy_requests_attention_for_commit_like_input() {
        assert_eq!(
            modal_window_event_disposition(&WindowEvent::DragDropped {
                id: winit::data_transfer::DataTransferId::from_raw(2),
                proposed_action: None,
            }),
            ShellModalWindowEventDisposition::BlockAndRequestAttention
        );
    }

    #[test]
    fn window_manager_close_request_policy_exits_main_even_with_modal() {
        assert!(window_manager_close_request_exits_application(
            ShellWindowCloseRequestTarget::Main,
            true,
        ));
        assert!(window_manager_close_request_exits_application(
            ShellWindowCloseRequestTarget::Main,
            false,
        ));
    }

    #[test]
    fn window_manager_close_request_policy_exits_stale_dialog_only_without_modal() {
        assert!(window_manager_close_request_exits_application(
            ShellWindowCloseRequestTarget::RecentlyClosedDialog,
            false,
        ));
        assert!(!window_manager_close_request_exits_application(
            ShellWindowCloseRequestTarget::RecentlyClosedDialog,
            true,
        ));
    }
}
