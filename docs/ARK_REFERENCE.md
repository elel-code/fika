# Ark Integration Reference

Fika's archive integration should follow Dolphin's menu boundaries first, then
decide whether an action is backed by KDE service menus, Ark D-Bus drag
extraction, Ark command-line execution, or a Rust fallback.

## Dolphin Sources

- `../dolphin/src/dolphincontextmenu.h`
  - `DolphinContextMenu` receives a shared `KFileItemActions` instance.
  - The header describes two dynamic menus for item context menus:
    `Open With` and `Actions`.
- `../dolphin/src/dolphincontextmenu.cpp`
  - `addItemContextMenu()` calls `m_fileItemActions->setItemListProperties()`
    for the current selected items, inserts single/multi item open actions,
    then calls `insertDefaultItemActions()` and `addAdditionalActions()`.
  - In multi-selection context menus, Dolphin checks whether all selected items
    can be opened as folders and may add `open_in_new_tabs`; it still calls
    `addOpenWithActions()` before default and service actions.
  - `addViewportContextMenu()` builds a `KFileItemListProperties` for the
    current directory, inserts Create New, Open With, Paste, Add to Places,
    Sort/View, then calls `addAdditionalActions(baseUrlProperties)`.
  - `addAdditionalActions()` inserts a separator, optionally adds Dolphin's
    local `open_terminal_here` action, then delegates to
    `m_fileItemActions->addActionsTo(..., KFileItemActions::MenuActionSource::All, ...)`.
    Compress/Extract entries are expected to arrive through this KDE service
    action path when Ark service menus are installed.
- `../dolphin/src/views/draganddrophelper.h`
  - Defines Ark drag MIME types:
    `application/x-kde-ark-dndextract-service` and
    `application/x-kde-ark-dndextract-path`.
- `../dolphin/src/views/draganddrophelper.cpp`
  - `dropUrls()` checks `isArkDndMimeType()` before ordinary URL drops.
  - For Ark drag data, Dolphin reads the remote D-Bus service name and object
    path from the MIME payload, then calls
    `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)`.
  - Ordinary URL drops continue through `KIO::drop()` after the no-op
    self-drop guard.
- `../dolphin/.flatpak-manifest.json`
  - Dolphin's Flatpak bundle includes Ark and archive libraries, including
    `libarchive`, `libzip`, `lzo`, `lrzip`, and `ark`. This confirms Dolphin's
    archive UX depends on Ark being available rather than being implemented
    entirely inside Dolphin itself.

## Dolphin Behavior Model

- Context-menu Compress and Extract are not hard-coded in Dolphin's static item
  menu. They are service/menu actions contributed through KDE's file item action
  infrastructure.
- A file manager still needs archive-aware menu placement:
  - Single archive file: Extract actions should appear with service actions.
  - Single non-archive file or directory: Compress should be available when Ark
    service actions match.
  - Multi-selection: Compress should be available when the action supports
    multi-file Exec field codes or when Fika supplies a fallback.
  - Blank directory menu: Dolphin sets the current directory as the
    `KFileItemListProperties` target, so service actions can apply to the
    directory itself.
- Dragging items out of Ark is separate from service menus. Ark publishes the
  D-Bus service/object MIME pair; Dolphin sends the destination path back to
  Ark via `extractSelectedFilesTo()`.

## Current Fika State

- Fika already parses KDE service menu files in `src/core/launcher.rs`.
  Matching actions become `ServiceMenuAction` values and execute through
  `DesktopLaunchPlan` plus `launch_with_systemd_user()`.
- Context menus already render service actions with named icons, support
  `TopLevel` promotion, `More Actions`, and `X-KDE-Submenu` nesting.
- Blank directory menus now request service actions for the current directory
  target, so Ark service menu entries can appear there when installed.
- Multi-selection service actions are intersected across selected items and only
  actions that support multi-path Exec field codes are shown.
- `src/core/archive.rs` now provides a small archive classifier that checks MIME
  types first and falls back to common archive extensions.
- Item and multi-selection context menus now expose a built-in `Compress...`
  fallback only when no matching service-menu Compress action exists. The
  fallback builds an `ark --add ...` `DesktopLaunchPlan` and executes through
  the same systemd user transient unit launcher as Open With and service-menu
  actions.
- Single archive file context menus now expose built-in `Extract Here` and
  `Extract To...` fallbacks only when no matching service-menu Extract action
  exists. `Extract Here` builds `ark --batch --destination <archive-parent>
  <archive>`, while `Extract To...` builds `ark --batch --dialog --destination
  <archive-parent> <archive>` so Ark owns the destination/options dialog. Both
  execute through the same systemd user transient unit launcher as Open With and
  service-menu actions.
- `src/core/archive.rs` now parses Ark's drag-extract MIME pair into a
  validated `ArkDndExtractPayload` containing the remote D-Bus service and
  object path. It also builds an `ArkDndExtractRequest` and can execute
  `extractSelectedFilesTo(destination)` through the shared session bus helper.
  Fika now handles ordinary external path-list drops through GPUI
  `ExternalPaths`, but it still does not receive arbitrary multi-MIME external
  offers through the GPUI backend, so Ark drag-extract offers cannot reach this
  executor yet. Archive virtual-directory browsing is also not implemented.

## Fika Implementation Plan

1. Keep service-menu driven actions as the first path.
   - Continue using `MimeApplicationCache::service_actions_for_targets()`.
   - Ensure `all/allfiles`, `all/all`, `inode/directory`, and archive MIME
     targets match Ark service menus without UI-side `.desktop` parsing.
   - Preserve Ark service `Icon=` values and submenu labels in the context menu.

2. Add a small archive classifier in core. [done]
   - Detect archive candidates by MIME first, then extension fallback for common
     formats: `.zip`, `.tar`, `.tar.gz`, `.tgz`, `.tar.bz2`, `.tbz2`,
     `.tar.xz`, `.txz`, `.7z`, `.rar`.
   - Keep the classifier separate from expensive thumbnail/preview roles.

3. Add fallback context-menu actions only when service menus do not provide
   equivalent actions.
   - Multi-selection and normal files/directories: show `Compress...`. [done]
   - Single archive file: show Extract Here and Extract To entries. [done]
   - If neither Ark nor a Rust fallback is available, render actions disabled
     with a status message rather than silently omitting the feature.

4. Route execution through the existing operation/status infrastructure.
   - Ark command-line fallback should run under the same systemd user transient
     unit launcher boundary as Open With/service actions. [done for
     Compress/Extract]
   - Rust fallback archive work should run off the UI thread and report progress
     through pane-local `TransferProgress`.
   - Operation results should refresh affected directories and use the existing
     pane-local status bar.

5. Add Ark DnD extraction support.
   - Extend external drop parsing to recognize Ark's two MIME values. [core
     parser done]
   - Store `remote_service`, `remote_path`, and destination directory in a core
     request. [core request/executor done; GPUI/backend multi-MIME offer
     routing pending]
   - Execute `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)` on the
     shared session bus helper. [core executor done; GPUI/backend multi-MIME
     offer routing pending]

6. Defer archive virtual-directory browsing.
   - Archive browsing needs a virtual `DirectoryLister` source, operation
     limits, and file extraction/update semantics. It should not be mixed into
     the first Compress/Extract menu implementation.

## Test Targets

- Service-menu matching keeps Ark actions visible for single file, directory,
  blank directory, and multi-selection targets.
- Built-in fallback Compress appears only when no matching service action exists.
  [covered]
- Archive classifier detects common formats and ignores unrelated extensions.
  [covered]
- Extract fallback appears for a single archive and does not appear for normal
  files. [covered]
- Ark DnD MIME parsing requires both service and path MIME values. [covered]
- Ark DnD execution sends `extractSelectedFilesTo(destination)` through the
  shared session bus helper. [core request/executor covered]
- External drop plumbing must route Ark DnD MIME offers to that executor and not
  ordinary file copy/move/link. [pending GPUI/backend MIME offer wiring]

## Remaining Work

- Wire Ark multi-MIME DnD offers from the GPUI/backend drag data path into the
  core parser/executor.
- Add Rust fallback archive work for systems without Ark, if required beyond the
  current Ark command-line fallback.
- Design archive virtual-directory browsing separately.
