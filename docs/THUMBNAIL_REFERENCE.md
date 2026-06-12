# Thumbnail Reference

This document records the Dolphin and freedesktop.org references for Fika's
thumbnail pipeline. Thumbnail work belongs in core scheduling/cache code first;
GPUI should only render the resolved image path for visible items.

## Dolphin Sources

- `../dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`
  - Updates expensive item roles separately from the base file model.
  - Schedules preview/thumbnail work around visible indexes first, then expands
    to the rest of the model.
  - Cancels stale preview jobs when the directory, icon size, or visible range
    changes.
- `../dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`
  - Keeps preview role state out of the view widget identity.
  - Tracks request generations so outdated thumbnail results cannot mutate the
    current model.
- `../dolphin/src/kitemviews/kfileitemlistwidget.cpp`
  - Renders either the resolved preview pixmap or the normal file icon.
  - The widget consumes model roles; it does not own thumbnail generation.
- `../dolphin/src/views/dolphinview.cpp`
  - View settings and zoom/icon-size changes trigger role updates without
    blocking directory navigation.

## Freedesktop Thumbnail Spec

- Cache root:
  - `$XDG_CACHE_HOME/thumbnails/` when `XDG_CACHE_HOME` is set.
  - `~/.cache/thumbnails/` otherwise.
- Normal thumbnails live in `normal/` and are sized up to 128x128.
- Large thumbnails live in `large/` and are sized up to 256x256.
- Failure markers live in `fail/gnome-thumbnail-factory/`.
- Cache file names are `md5(uri).png`, where `uri` is the canonical file URI
  such as `file:///home/user/Pictures/a%20b.png`.
- A failed thumbnail generation should leave a failure marker so later scans can
  skip the same file until metadata changes invalidate the request.

## Fika Mapping

- `src/core/thumbnails.rs`
  - Builds freedesktop file URIs from absolute paths using percent-encoded path
    bytes.
  - Computes the required MD5 cache key without adding a new direct dependency.
  - Resolves `normal/`, `large/`, and failure cache paths from a thumbnail cache
    root.
  - Looks up cached thumbnails by checking `normal/` before `large/`, matching
    the current TODO acceptance order. Cache hits are trusted only when PNG
    `tEXt` metadata has the expected `Thumb::URI`; path-based lookups also
    require `Thumb::MTime` to match the source file mtime.
  - Records failure markers under `fail/gnome-thumbnail-factory/` using a small
    PNG marker file.
- `src/core/entries.rs`
  - `EntryData` carries `thumbnail_path: Option<PathBuf>` as a lightweight model
    role.
  - Directory loading resolves existing cache hits for ordinary files and keeps
    only the thumbnail path in core entries after URI/mtime validation. Pixel
    data belongs in the GPUI image cache, not in core model entries.
- `src/main.rs`
  - Pane snapshots copy ordinary-file thumbnail cache hits into
    `VisibleItemSnapshot::thumbnail_path`. Directories ignore thumbnail paths
    even if malformed test data supplies one.
- `src/ui/file_grid.rs`
  - Item rendering tries `thumbnail_path` first, then falls back to the resolved
    theme/MIME icon image, then to the compact text marker. Thumbnail images do
    not change compact layout geometry and do not enter the theme icon cache.
- Future worker integration should route requests by `PaneId + generation`,
  prioritize current visible items, cap concurrent generation jobs, and cancel
  pending work when navigation or zoom invalidates the request.

## Remaining Work

- Add a thumbnail request queue that accepts visible item ranges and schedules
  visible-first work.
- Integrate external thumbnailers or image decoding behind a bounded worker
  pool.
- Update stale `thumbnail_path` roles when generated thumbnails finish or
  metadata changes invalidate cache entries.
- Make visible item slots request missing thumbnails as they enter the viewport
  without resizing the compact layout.
