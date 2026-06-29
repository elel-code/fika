use winit::cursor::{Cursor as WinitCursor, CursorIcon};
use winit::dpi::PhysicalSize;
use winit::event::{Modifiers, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Theme, Window, WindowAttributes, WindowId};

use crate::WgpuState;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ShellDialogWindowKind {
    Create,
    OpenWith,
    Rename,
}

impl ShellDialogWindowKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::OpenWith => "open-with",
            Self::Rename => "rename",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellDialogWindowSpec {
    title: String,
    surface_size: PhysicalSize<u32>,
    min_surface_size: Option<PhysicalSize<u32>>,
    max_surface_size: Option<PhysicalSize<u32>>,
    resizable: bool,
    theme: Option<Theme>,
}

impl ShellDialogWindowSpec {
    pub(crate) fn fixed(title: String, surface_size: PhysicalSize<u32>, theme: Theme) -> Self {
        Self {
            title,
            surface_size,
            min_surface_size: Some(surface_size),
            max_surface_size: Some(surface_size),
            resizable: false,
            theme: Some(theme),
        }
    }

    fn window_attributes(&self) -> WindowAttributes {
        let mut attrs = WindowAttributes::default()
            .with_title(self.title.clone())
            .with_surface_size(self.surface_size)
            .with_resizable(self.resizable)
            .with_theme(self.theme);
        if let Some(min_surface_size) = self.min_surface_size {
            attrs = attrs.with_min_surface_size(min_surface_size);
        }
        if let Some(max_surface_size) = self.max_surface_size {
            attrs = attrs.with_max_surface_size(max_surface_size);
        }
        attrs
    }
}

pub(crate) struct ShellDetachedDialogWindow {
    kind: ShellDialogWindowKind,
    renderer: WgpuState,
    window: Box<dyn Window>,
    cursor_icon: CursorIcon,
}

impl ShellDetachedDialogWindow {
    pub(crate) fn create(
        event_loop: &dyn ActiveEventLoop,
        kind: ShellDialogWindowKind,
        spec: &ShellDialogWindowSpec,
    ) -> Result<Self, String> {
        let window = event_loop
            .create_window(spec.window_attributes())
            .map_err(|error| format!("{} dialog window create failed: {error}", kind.as_str()))?;
        let renderer = WgpuState::new(window.as_ref())
            .map_err(|error| format!("{} dialog renderer init failed: {error}", kind.as_str()))?;
        Ok(Self {
            kind,
            renderer,
            window,
            cursor_icon: CursorIcon::Default,
        })
    }

    pub(crate) fn kind(&self) -> ShellDialogWindowKind {
        self.kind
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub(crate) fn renderer_size(&self) -> PhysicalSize<u32> {
        self.renderer.size
    }

    pub(crate) fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    pub(crate) fn sync(&mut self, spec: &ShellDialogWindowSpec) {
        self.window.set_title(&spec.title);
        self.window.set_theme(spec.theme);
        self.window
            .set_min_surface_size(spec.min_surface_size.map(Into::into));
        self.window
            .set_max_surface_size(spec.max_surface_size.map(Into::into));
        self.window.set_resizable(spec.resizable);
        if let Some(applied) = self.window.request_surface_size(spec.surface_size.into()) {
            self.renderer.resize(applied);
        }
        self.request_redraw();
    }

    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        self.renderer.resize(size);
    }

    pub(crate) fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub(crate) fn set_cursor(&mut self, cursor_icon: CursorIcon) {
        if self.cursor_icon == cursor_icon {
            return;
        }
        self.cursor_icon = cursor_icon;
        self.window.set_cursor(WinitCursor::Icon(cursor_icon));
    }

    pub(crate) fn renderer_and_window_mut(&mut self) -> (&mut WgpuState, &dyn Window) {
        (&mut self.renderer, self.window.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShellDialogWindowHostEvent {
    CloseRequested,
    SurfaceResized,
    ScaleFactorChanged {
        scale_factor: f32,
        renderer_size: PhysicalSize<u32>,
    },
    ModifiersChanged(Modifiers),
}

#[derive(Default)]
pub(crate) struct ShellDialogWindows {
    create: Option<ShellDetachedDialogWindow>,
    open_with: Option<ShellDetachedDialogWindow>,
    rename: Option<ShellDetachedDialogWindow>,
}

impl ShellDialogWindows {
    pub(crate) fn is_open(&self, kind: ShellDialogWindowKind) -> bool {
        self.get(kind).is_some()
    }

    pub(crate) fn get(&self, kind: ShellDialogWindowKind) -> Option<&ShellDetachedDialogWindow> {
        match kind {
            ShellDialogWindowKind::Create => self.create.as_ref(),
            ShellDialogWindowKind::OpenWith => self.open_with.as_ref(),
            ShellDialogWindowKind::Rename => self.rename.as_ref(),
        }
    }

    pub(crate) fn get_mut(
        &mut self,
        kind: ShellDialogWindowKind,
    ) -> Option<&mut ShellDetachedDialogWindow> {
        match kind {
            ShellDialogWindowKind::Create => self.create.as_mut(),
            ShellDialogWindowKind::OpenWith => self.open_with.as_mut(),
            ShellDialogWindowKind::Rename => self.rename.as_mut(),
        }
    }

    pub(crate) fn request_redraw(&self, kind: ShellDialogWindowKind) -> bool {
        self.get(kind).is_some_and(|window| {
            window.request_redraw();
            true
        })
    }

    pub(crate) fn resize(&mut self, kind: ShellDialogWindowKind, size: PhysicalSize<u32>) -> bool {
        self.get_mut(kind).is_some_and(|window| {
            window.resize(size);
            window.request_redraw();
            true
        })
    }

    pub(crate) fn set_cursor(
        &mut self,
        kind: ShellDialogWindowKind,
        cursor_icon: CursorIcon,
    ) -> bool {
        self.get_mut(kind).is_some_and(|window| {
            window.set_cursor(cursor_icon);
            true
        })
    }

    pub(crate) fn renderer_size(&self, kind: ShellDialogWindowKind) -> Option<PhysicalSize<u32>> {
        self.get(kind).map(ShellDetachedDialogWindow::renderer_size)
    }

    pub(crate) fn handle_window_event(
        &mut self,
        kind: ShellDialogWindowKind,
        event: &WindowEvent,
    ) -> Option<ShellDialogWindowHostEvent> {
        match event {
            WindowEvent::CloseRequested => Some(ShellDialogWindowHostEvent::CloseRequested),
            WindowEvent::SurfaceResized(size) => {
                self.resize(kind, *size);
                Some(ShellDialogWindowHostEvent::SurfaceResized)
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.get(kind)
                    .map(|window| ShellDialogWindowHostEvent::ScaleFactorChanged {
                        scale_factor: window.scale_factor(),
                        renderer_size: window.renderer_size(),
                    })
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                Some(ShellDialogWindowHostEvent::ModifiersChanged(*modifiers))
            }
            _ => None,
        }
    }

    pub(crate) fn set(&mut self, kind: ShellDialogWindowKind, window: ShellDetachedDialogWindow) {
        debug_assert_eq!(window.kind(), kind);
        match kind {
            ShellDialogWindowKind::Create => self.create = Some(window),
            ShellDialogWindowKind::OpenWith => self.open_with = Some(window),
            ShellDialogWindowKind::Rename => self.rename = Some(window),
        }
    }

    pub(crate) fn close(&mut self, kind: ShellDialogWindowKind) {
        match kind {
            ShellDialogWindowKind::Create => self.create = None,
            ShellDialogWindowKind::OpenWith => self.open_with = None,
            ShellDialogWindowKind::Rename => self.rename = None,
        }
    }

    pub(crate) fn close_all(&mut self) {
        self.create = None;
        self.open_with = None;
        self.rename = None;
    }

    pub(crate) fn window_kind_for_id(&self, window_id: WindowId) -> Option<ShellDialogWindowKind> {
        [
            self.create.as_ref(),
            self.open_with.as_ref(),
            self.rename.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find(|window| window.window_id() == window_id)
        .map(ShellDetachedDialogWindow::kind)
    }
}
