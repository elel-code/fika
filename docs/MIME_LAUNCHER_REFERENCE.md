# MIME and Launcher Reference

This document records the source references for Fika's MIME identification,
icon selection, Open With menu, and future process launching path.

## Dolphin References

- `../dolphin/src/kitemviews/kfileitemmodel.cpp`
  - `KFileItemModel::createItemDataList()` resolves MIME types synchronously
    only when sorting by type needs stable ordering.
  - `KFileItemModel::retrieveData()` stores only fast roles on the model path.
    It avoids calling expensive `KFileItem::iconName()` when MIME type is
    unknown.
  - When MIME type is known, icon data comes from `item.iconName()`. If the
    icon theme does not provide that icon, Dolphin falls back through
    `QMimeDatabase().mimeTypeForName(item.mimetype()).genericIconName()`.
- `../dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`
  - MIME comments, icon names, permissions, thumbnails, and other expensive
    roles are resolved outside the fast listing path.
  - The updater can stop MIME role work when the view changes, matching the
    model/lister cancellation boundary.
- `../dolphin/src/kitemviews/kfileitemlistview.cpp`
  - `applyRolesToItem()` inserts `iconName` when it is missing and uses the
    same generic icon fallback if the theme lacks the specific MIME icon.
- `../dolphin/src/kitemviews/kstandarditemlistwidget.cpp`
  - Widget rendering uses the `iconName` role and does not determine MIME
    types itself.
- `../dolphin/src/dolphincontextmenu.cpp`
  - Context menu population delegates file-specific Open With and service menu
    actions to KDE action infrastructure.
- `../dolphin/src/servicemenushortcutmanager.cpp`
  - Service menu shortcuts are registered from `KFileItemActions`; execution
    remains owned by the action layer.

## Cosmic Files References

- `../cosmic-files/src/mime_icon.rs`
  - Uses shared-mime-info (`xdg_mime::SharedMimeInfo`) for MIME guesses and
    icon name lookup on Unix.
  - Falls back to path-based MIME guessing when the shared MIME result is
    uncertain and not a special filesystem type.
  - Caches MIME icon handles by `(mime, size)`.
- `../cosmic-files/src/mime_app.rs`
  - Builds a MIME application cache from desktop entries and `mimeapps.list`.
  - Tracks default applications separately from additional associations.
- `../cosmic-files/src/app.rs`
  - Groups launch requests by MIME type.
  - Special-cases desktop files and executable files.
  - Tries the MIME app cache first, then parent MIME types, then a generic open
    fallback.

## Fika Mapping

- Core MIME parsing lives in `src/core/mime.rs`.
  - It reads shared-mime-info `globs2`, `icons`, `generic-icons`, and MIME XML
    icon declarations from XDG data directories.
  - It maps literal filenames, multi-suffix patterns, simple extensions, and
    common magic bytes to MIME names.
- Entry construction lives in `src/core/entries.rs`.
  - Directory listing always stores a `mime_type` on `EntryData`.
  - The fast path uses shared-mime-info filename/glob data.
  - Only generic `application/octet-stream` files read a small prefix for magic
    sniffing, so common extension-mapped files do not open file contents during
    listing.
- UI icon selection lives in `src/main.rs`.
  - File icons are cached by MIME/file kind and icon size.
  - Candidate order mirrors Dolphin: specific MIME icon names first, then
    shared-mime-info icon names, then generic MIME icon names, then unknown file
    fallback.
- Core launcher and application discovery lives in `src/core/launcher.rs`.
  - It parses `.desktop` `[Desktop Entry]` application records, `MimeType=`,
    `Exec=`, `Actions=`, and `[Desktop Action ...]` groups.
  - `launch_with_systemd_user()` starts launch plans as user systemd transient
    units through the shared `src/core/bus.rs` session-bus helper, so Open
    With, service menu, Ark fallback, and Open in New Window use one D-Bus
    timeout/retry boundary.
  - It preserves `Icon=` from application desktop entries, `[Desktop Action ...]`
    groups, and KDE service menu desktop entries. A service action uses its own
    action icon first, then falls back to the parent application/service icon,
    matching Dolphin's action-layer icon propagation.
  - It discovers KDE service menu desktop files from XDG data service menu
    directories and accepts `Type=Service` records with
    `X-KDE-ServiceTypes=KonqPopupMenu/Plugin` or `KFileItemAction/Plugin`.
  - Service menu actions are filtered by target MIME using exact MIME,
    `type/*`, `all/all`, `all/allfiles`, and `inode/directory` matching.
  - KDE service menu conditions now also honor local `X-KDE-Protocols=file`,
    `X-KDE-RequiredNumberOfUrls`, `X-KDE-MinNumberOfUrls`,
    `X-KDE-MaxNumberOfUrls`, `X-KDE-ShowIfExecutable`, and
    `X-KDE-Priority=TopLevel`. Non-local protocol-only actions are not shown in
    Fika's current local-filesystem UI.
  - `X-KDE-Submenu` is preserved on service actions. The GPUI menu renders it
    as real nested submenu rows inside `More Actions`, so unrelated service
    actions are not shown as one flat list and KDE submenu structure is not
    flattened into disabled headers.
  - Multi-selection service menu actions are an intersection across all
    selected targets and require an Exec field that accepts multiple paths
    (`%F` or `%U`), so single-file `%f/%u` actions are not offered for batch
    operations.
  - It parses `mimeapps.list` `[Default Applications]`,
    `[Added Associations]`, and `[Removed Associations]`.
  - Application ordering mirrors the desktop association stack: default
    application first, then added associations, then applications that declare
    the MIME type themselves, with removed associations filtered out.
  - Open With lookup also accepts wildcard desktop MIME declarations such as
    `image/*` before considering parent MIME fallback. If no exact or wildcard
    application is available, text-like child MIME types such as `text/x-rust`,
    `application/json`, `application/*+json`, and shell/script MIME types fall
    back to `text/plain` associations.
  - It builds a launch plan from desktop `Exec` field codes and converts it to
    systemd user transient units.
  - Launch execution uses D-Bus `org.freedesktop.systemd1.Manager.StartTransientUnit()`
    on the session bus. Fika does not retain child process handles.
- Open With UI integration lives in `src/main.rs`.
  - The item context menu stores core-derived `MimeApplication` values on the
    menu target.
  - GPUI rows only render the Open With and Actions submenus and route the
    selected desktop or service action id back to core launcher data.
  - The selected application is launched through `launch_with_systemd_user()`;
    success and structured launcher errors are reported back to the originating
    pane status bar.
  - Service menu and desktop action rows render the core-provided icon name
    through the same named theme icon resolver used by file/place icons, with
    compact marker fallback only when the theme cannot resolve the name.
  - The "Other Application..." dialog lists every application from the same
    core desktop application cache, including apps that do not declare the
    current MIME type. Selecting one still builds a `DesktopLaunchPlan` and
    launches it through `launch_with_systemd_user()`.
  - When the dialog was opened for a known MIME type, non-default application
    rows expose a Set Default action. The action writes the user
    `mimeapps.list`, updates `[Default Applications]`, adds the application to
    `[Added Associations]`, removes the same application from
    `[Removed Associations]`, reloads the launcher cache, and keeps the dialog
    open with the new default badge.
  - Service menu action execution uses the same `DesktopLaunchPlan` and systemd
    transient unit path as Open With. Multi-selection actions pass the selected
    path list to the launch plan instead of re-reading filesystem state.

## Remaining Work

- Add advanced KDE conditions that need KIO or authorization context.
