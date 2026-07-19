use crate::shell::file_item_view::style::DolphinItemPalette;
use crate::shell::theme::ShellTheme;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShellPaintPalettes {
    pub(crate) shell: ShellTheme,
    pub(crate) dolphin_item: DolphinItemPalette,
}

impl ShellPaintPalettes {
    pub(crate) fn from_shell_theme(shell: ShellTheme) -> Self {
        Self {
            shell,
            dolphin_item: DolphinItemPalette::from_shell_theme(shell),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_palettes_reuse_shell_theme_adapters() {
        let shell = ShellTheme::for_dark_mode(true);
        let palettes = ShellPaintPalettes::from_shell_theme(shell);

        assert!(palettes.shell.is_dark());
        assert_eq!(
            palettes.dolphin_item,
            DolphinItemPalette::from_shell_theme(shell)
        );
    }
}
