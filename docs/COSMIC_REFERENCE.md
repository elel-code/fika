# COSMIC Files Reference

This file records local `./cosmic-files` reference points for follow-up Fika polish.

## Visual Direction

Reference files:

- `cosmic-files/src/menu.rs`
- `cosmic-files/src/app.rs`
- `cosmic-files/src/dialog.rs`

Useful UI direction:

- Prefer quieter surfaces with clearer row spacing, softer separators, and less dense menu groups where it helps scanning.
- Keep primary panes visually simple: sidebar, toolbar, content, and footer should feel integrated instead of heavily framed.
- Context menus should keep practical action grouping, but visual weight can move closer to COSMIC Files than Dolphin when the interaction rules are already covered by tests.
- Fika's first visual pass keeps Dolphin-like pane layout, but shifts the surface colors toward COSMIC's quieter feel: off-white light backgrounds, lower-contrast separators, softer hover fills, 7-8px radii for controls/menus, and consistent selected/drop feedback between the sidebar and main pane.

## Terminal Launch

Reference files:

- `cosmic-files/src/mime_app.rs`
- `cosmic-files/src/app.rs`

Useful terminal rules:

- Build a terminal candidate list from desktop application metadata, specifically `TerminalEmulator` categories.
- Prefer the default `x-scheme-handler/terminal` desktop handler when the desktop reports one.
- Keep a known terminal fallback path for environments without desktop metadata.

Current Fika mapping:

- `src/desktop/terminal.rs` keeps explicit `FIKA_TERMINAL` / `TERMINAL` overrides first.
- After explicit overrides, Fika queries `xdg-mime query default x-scheme-handler/terminal`, resolves the desktop file, and accepts it when it is a visible `TerminalEmulator`.
- Fika then prefers `com.system76.CosmicTerm.desktop`, scans visible `TerminalEmulator` desktop entries, and finally falls back to known terminal executable names.
