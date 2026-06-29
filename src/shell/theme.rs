use cosmic_text::Color as TextColor;

use crate::shell::options::ShellViewMode;
use crate::shell::tasks::ShellTaskStatusKind;

pub(crate) type UiColor = [f32; 4];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellThemeMode {
    Light,
    Dark,
}

impl ShellThemeMode {
    pub(crate) fn from_dark_mode(dark_mode: bool) -> Self {
        if dark_mode { Self::Dark } else { Self::Light }
    }

    pub(crate) fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShellTheme {
    mode: ShellThemeMode,
    view_surface: UiColor,
    view_content: UiColor,
    chrome: UiColor,
    sidebar: UiColor,
    divider: UiColor,
    primary_text: TextColor,
    muted_text: TextColor,
    section_text: TextColor,
    accent_text: TextColor,
    accent: UiColor,
    task_running: UiColor,
    task_completed: UiColor,
    task_failed: UiColor,
    task_cancelled: UiColor,
}

impl ShellTheme {
    pub(crate) fn for_dark_mode(dark_mode: bool) -> Self {
        match ShellThemeMode::from_dark_mode(dark_mode) {
            ShellThemeMode::Light => Self::light(),
            ShellThemeMode::Dark => Self::dark(),
        }
    }

    #[cfg(test)]
    pub(crate) fn mode(self) -> ShellThemeMode {
        self.mode
    }

    pub(crate) fn is_dark(self) -> bool {
        self.mode.is_dark()
    }

    pub(crate) fn view_mode_surface(self, _view_mode: ShellViewMode) -> UiColor {
        self.view_surface
    }

    pub(crate) fn view_mode_content(self, _view_mode: ShellViewMode) -> UiColor {
        self.view_content
    }

    pub(crate) fn view_mode_clear(self, view_mode: ShellViewMode) -> wgpu::Color {
        let [r, g, b, a] = self.view_mode_surface(view_mode);
        wgpu::Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: a as f64,
        }
    }

    pub(crate) fn chrome(self) -> UiColor {
        self.chrome
    }

    pub(crate) fn sidebar(self) -> UiColor {
        self.sidebar
    }

    pub(crate) fn divider(self) -> UiColor {
        self.divider
    }

    pub(crate) fn primary_text(self) -> TextColor {
        self.primary_text
    }

    pub(crate) fn muted_text(self) -> TextColor {
        self.muted_text
    }

    pub(crate) fn section_text(self) -> TextColor {
        self.section_text
    }

    pub(crate) fn accent_text(self) -> TextColor {
        self.accent_text
    }

    pub(crate) fn accent(self) -> UiColor {
        self.accent
    }

    pub(crate) fn task_status_color(self, kind: ShellTaskStatusKind) -> UiColor {
        match kind {
            ShellTaskStatusKind::Running => self.task_running,
            ShellTaskStatusKind::Completed => self.task_completed,
            ShellTaskStatusKind::Failed => self.task_failed,
            ShellTaskStatusKind::Cancelled => self.task_cancelled,
        }
    }

    fn light() -> Self {
        Self {
            mode: ShellThemeMode::Light,
            view_surface: [0.973, 0.976, 0.984, 1.0],
            view_content: [0.973, 0.976, 0.984, 1.0],
            chrome: [0.973, 0.976, 0.984, 1.0],
            sidebar: [0.973, 0.976, 0.984, 1.0],
            divider: [0.784, 0.808, 0.839, 1.0],
            primary_text: TextColor::rgb(36, 41, 47),
            muted_text: TextColor::rgb(89, 99, 110),
            section_text: TextColor::rgb(107, 114, 128),
            accent_text: TextColor::rgb(31, 79, 191),
            accent: [0.184, 0.435, 0.929, 1.0],
            task_running: [0.184, 0.435, 0.929, 1.0],
            task_completed: [0.102, 0.514, 0.286, 1.0],
            task_failed: [0.820, 0.184, 0.184, 1.0],
            task_cancelled: [0.475, 0.514, 0.565, 1.0],
        }
    }

    fn dark() -> Self {
        Self {
            mode: ShellThemeMode::Dark,
            view_surface: [0.102, 0.112, 0.126, 1.0],
            view_content: [0.078, 0.086, 0.098, 1.0],
            chrome: [0.125, 0.137, 0.153, 1.0],
            sidebar: [0.118, 0.129, 0.145, 1.0],
            divider: [0.255, 0.278, 0.310, 1.0],
            primary_text: TextColor::rgb(226, 232, 240),
            muted_text: TextColor::rgb(148, 163, 184),
            section_text: TextColor::rgb(156, 163, 175),
            accent_text: TextColor::rgb(147, 197, 253),
            accent: [0.184, 0.435, 0.929, 1.0],
            task_running: [0.184, 0.435, 0.929, 1.0],
            task_completed: [0.102, 0.514, 0.286, 1.0],
            task_failed: [0.820, 0.184, 0.184, 1.0],
            task_cancelled: [0.475, 0.514, 0.565, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_mode_selects_light_and_dark_palettes() {
        let light = ShellTheme::for_dark_mode(false);
        let dark = ShellTheme::for_dark_mode(true);

        assert_eq!(light.mode(), ShellThemeMode::Light);
        assert_eq!(dark.mode(), ShellThemeMode::Dark);
        assert!(!light.is_dark());
        assert!(dark.is_dark());
        assert_eq!(light.chrome(), [0.973, 0.976, 0.984, 1.0]);
        assert_eq!(dark.chrome(), [0.125, 0.137, 0.153, 1.0]);
        assert_eq!(
            light.task_status_color(ShellTaskStatusKind::Failed),
            [0.820, 0.184, 0.184, 1.0]
        );
    }
}
