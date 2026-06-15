# Clipboard Reference

Fika's clipboard path follows Dolphin's file-clipboard semantics while staying on
GPUI's public clipboard API.

## Dolphin Sources

- `../dolphin/src/views/dolphinview.cpp`
  - `cutSelectedItemsToClipboard()` builds selection `QMimeData`, marks it as
    cut with `KIO::setClipboardDataCut()`, exports URLs for the portal, then
    writes it to `QApplication::clipboard()`.
  - `copySelectedItemsToClipboard()` builds the same selection MIME data and
    writes it without the cut marker.
  - `pasteToUrl()` passes `QApplication::clipboard()->mimeData()` to
    `KIO::paste()` and listens for created-item and copy-job signals.
  - `selectionMimeData()` delegates selected model indexes to the file item
    model.
- `../dolphin/src/kitemviews/kfileitemmodel.cpp`
  - `KFileItemModel::createMimeData()` converts selected item indexes to URL
    lists and `mostLocalUrl()` lists.
  - It skips children whose parent is already included in the MIME payload.
  - `KUrlMimeData::setUrls()` is the final file-list MIME writer.
- `../dolphin/src/kitemviews/private/kfileitemclipboard.{h,cpp}`
  - Tracks the active cut set from clipboard MIME data.
  - Uses `application/x-kde-cutselection` through KDE/KIO helpers.

## GPUI Sources

- `gpui/src/platform.rs`
  - `ClipboardItem` stores string, image, and `ExternalPaths` entries.
  - `ClipboardItem::new_string_with_metadata()` keeps app-local metadata on a
    single string entry.
  - `ClipboardEntry::ExternalPaths` represents platform-provided path lists.
- `gpui/src/app.rs`
  - `App` exposes `read_from_clipboard()` and `write_to_clipboard()`.
  - Linux and FreeBSD builds also expose primary-selection read/write APIs.
- `gpui_linux/src/linux/wayland/clipboard.rs`
  - The Wayland backend offers text MIME types through `TEXT_MIME_TYPES`.
  - It defines `FILE_LIST_MIME_TYPE` as `text/uri-list`, but normal clipboard
    reads currently accept only the allowed text MIME list exposed there.
  - The app-visible send path serializes `ClipboardItem::text()`.
- `gpui_linux/src/linux/wayland/client.rs`
  - Clipboard and primary writes create Wayland data sources from GPUI
    clipboard items.
  - Drag-and-drop data offers accept `text/uri-list` and convert received paths
    into GPUI `ExternalPaths`.
- `gpui_linux/src/linux/x11/clipboard.rs`
  - Normal clipboard reads prefer image/text targets and do not request the
    `text/uri-list` target as a file-list `ExternalPaths` entry.

## Fika Mapping

- Core file clipboard data lives in `src/core/clipboard.rs`.
- `FileClipboardRole` mirrors Dolphin copy/cut state.
- `encode_file_clipboard_text()` writes a file URI list. Cut payloads include a
  Fika metadata marker, and the decoder also accepts common `copy`/`cut`
  first-line markers.
- `decode_file_clipboard_text()` accepts `file://` URI-list text and plain
  absolute paths.
- `ClipboardState` in `src/ui/clipboard.rs` and `src/ui/clipboard/state.rs`
  bridges the core payload to GPUI `ClipboardItem`.
- Copy and cut write the payload to GPUI clipboard and, on Linux/FreeBSD, the
  primary selection.
- Paste imports GPUI clipboard first, then primary selection on Linux/FreeBSD.
- Middle-click paste is primary-selection-only: blank pane space pastes into the
  current directory, and middle-clicking a directory item pastes into that
  directory without falling back to the normal clipboard.
- URI-list payloads paste as file transfers. Plain text payloads paste as a new
  `Pasted Text.txt` file using the same keep-both naming path as file creation.
- Paste result handling records transfer undo for copied/moved files and create
  undo for pasted text files.

## Known Dolphin/KDE Clipboard Limitation

- Dolphin/KDE file copy publishes the selected URLs as clipboard MIME data,
  including `text/uri-list`, and keeps cut/copy state in KDE-specific MIME
  metadata.
- Current GPUI Linux clipboard reads do not expose clipboard target lists or
  convert `text/uri-list` clipboard data into `ClipboardEntry::ExternalPaths`
  on the normal paste path.
- When a Dolphin file-copy offer also exposes a plain text target, GPUI can
  return that text to Fika. Fika then treats it as ordinary text paste and
  creates `Pasted Text.txt` because it cannot see the hidden file-list MIME
  target.
- Fixing this correctly requires GPUI Linux backend work: read `text/uri-list`
  before generic text for clipboard paste, convert file URLs to
  `ExternalPaths`, and expose enough metadata to preserve KDE cut/copy state.
  Fika should not patch the vendored GPUI checkout in-tree; keep this as an
  upstream/backend task unless a local GPUI fork is intentionally adopted.

## Remaining Protocol Work

- GPUI's current public clipboard API does not let Fika explicitly publish a
  multi-entry Wayland data source with both `text/uri-list` and `text/plain`
  from app code.
- GPUI's Linux clipboard read path currently reads image/text MIME types before
  file-list MIME types; peers that offer `text/uri-list` need backend support
  before Fika can import them directly as file transfers.
- Drag-and-drop path-list offers now arrive as GPUI `ExternalPaths` and are
  wired into Fika's pane file operation pipeline. Arbitrary non-path or
  multi-MIME drag offers still need backend exposure before they can be unified
  with the same `FileClipboardPayload` model.
