# Archive Reference

This document records the source references for Fika's archive and Ark
integration. Dolphin is the behavioral reference for context actions and Ark
drag-and-drop interop; Fika keeps the file-manager UI native and uses Ark as an
external archive tool when the user invokes archive actions.

## Dolphin Sources

- `../dolphin/src/dolphincontextmenu.cpp`
  - `addAdditionalActions()` delegates context-menu extensions to
    `KFileItemActions::addActionsTo(..., MenuActionSource::All, ...)`.
  - Local directory targets can add local-only actions such as Open Terminal
    before the service-menu actions.
- `../dolphin/src/dolphinviewcontainer.cpp`
  - Item activation asks `DolphinView::openItemAsFolderUrl()` before launching
    a file, so archives can be browsed as folders when the setting allows it.
  - Middle-click falls back to opening archives in a tab when there is no second
    or third associated application.
- `../dolphin/src/settings/dolphin_generalsettings.kcfg`
  - `BrowseThroughArchives` controls archive-as-folder browsing.
- `../dolphin/src/views/draganddrophelper.cpp`
  - Ark drag payloads carry a D-Bus service and object path.
  - Dolphin calls `org.kde.ark.DndExtract.extractSelectedFilesTo` with the drop
    destination when both Ark MIME payload fields are present.
- `../dolphin/src/views/draganddrophelper.h`
  - Ark DnD MIME names are
    `application/x-kde-ark-dndextract-service` and
    `application/x-kde-ark-dndextract-path`.

## Fika Mapping

- `src/core/archive.rs`
  - Classifies common archive MIME types and extensions.
  - Parses and validates Ark DnD payloads and builds the structured D-Bus
    request for `extractSelectedFilesTo`.
- `src/core/launcher/ark.rs`
  - Builds Ark launch plans for `Compress`, `Extract Here`, and `Extract To`
    through the `ark` command.
  - Keeps Ark execution behind the same `DesktopLaunchPlan` and systemd-user
    launcher used by Open With and service-menu actions.
- `src/main.rs`
  - Adds Fika-owned context action IDs:
    `fika.builtin.ark.compress`, `fika.builtin.ark.extract-here`, and
    `fika.builtin.ark.extract-to`.
  - Shows `Compress...` for local file/directory item selections.
  - Shows `Extract Here` and `Extract To...` only for a single local archive,
    using MIME information first and file extension as a fallback.
  - Does not expose Ark local actions for Trash or pure network URI targets.
  - De-duplicates by visible label when a discovered service menu already
    provides the same archive action.

## Remaining Work

- Add archive-as-folder browsing after the directory model can represent archive
  virtual folders cleanly.
- Surface completion/failure from launched Ark processes in the UI instead of
  only recording the start action.
- Revisit archive actions for remote locations after the network backend can
  provide a local mounted path or a backend-native archive operation.
