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
    PNG marker file with `Thumb::URI` and `Thumb::MTime`, so stale markers do not
    suppress a changed file forever.
  - On cache miss, reads freedesktop thumbnailer desktop files from
    `$XDG_DATA_HOME/thumbnailers` and `$XDG_DATA_DIRS/*/thumbnailers`, matches
    `MimeType=` against the request MIME, expands `Exec=` field codes
    (`%i`, `%u`, `%o`, `%s`) plus common freedesktop file/url fields
    (`%f`, `%F`, `%U`, `%d`, `%D`, `%n`, `%N`, `%%`), and runs the installed
    thumbnailer. If no registry entry matches, Fika falls back to a small
    built-in command list for common image, video, and document formats
    (`gdk-pixbuf-thumbnailer`, `ffmpegthumbnailer`, `totem-video-thumbnailer`,
    `evince-thumbnailer`). Generated PNGs are patched with freedesktop
    `Thumb::URI` and `Thumb::MTime` metadata before being moved into the normal
    thumbnail cache.
- `src/core/model.rs`
  - `ModelEntry` carries `thumbnail_path: Option<PathBuf>` as the pane-local
    preview role, matching Dolphin's `iconPixmap` separation from base file
    metadata.
  - Directory loading leaves the preview role empty. Thumbnail cache probing is
    scheduled from the visible/read-ahead item band instead of running across
    the whole directory before the first model reset. Same-file reloads preserve
    the role only when name, size, and mtime still match; changed files clear it.
- `src/main.rs` and `src/ui/file_grid/snapshot.rs`
  - Pane snapshots copy ordinary-file thumbnail roles into
    `VisibleItemSnapshot::thumbnail_path`. The snapshot type lives in
    `src/ui/file_grid/snapshot.rs`. Directories ignore thumbnail paths even if
    malformed test data supplies one.
  - Visible ordinary files without a thumbnail role are queued through
    `ThumbnailRequestQueue` using the entry's existing mtime metadata, so the UI
    frame path does not restat files.
  - Theme file icons are resolved on-demand through `FileIconCache`
    (`src/ui/icons/cache.rs`). The model role writeback path
    (`ModelEntry.icon_name` and `src/ui/icons/roles.rs`) has been removed.
    File-grid rendering feeds thumbnail/theme paths into the custom image paint
    layer, which uses GPUI `RetainAllImageCache` internally and paints loaded
    images with `Window::paint_image`.
- `src/core/thumbnails/scheduler.rs`
  - Owns the UI-neutral scheduling support around `ThumbnailRequestQueue`:
    `ThumbnailScheduler`, `ThumbnailCandidate`, `ThumbnailWorkKey`,
    `ThumbnailProbeResult`, request queue and seen-set state, active batch
    cancellation, Dolphin `KFileItemModelRolesUpdater::indexesToResolve()` style
    read-ahead index calculation, candidate-to-request conversion, matching
    failure-marker checks, bounded worker queue, and
    `ItemId + path` guarded probe-result application to `DirectoryModel`.
    Long-lived work keys store pane/generation/item/mtime plus path and MIME
    hashes rather than retaining full paths or MIME strings.
  - A bounded background cache-probe batch is processed by a fixed worker queue
    with at most four in-flight requests; each worker checks freedesktop cache
    hits, attempts external thumbnail generation on miss, and returns results
    that `src/main.rs` writes back to `DirectoryModel` by
    `PaneId + generation + ItemId + path`.
  - Thumbnail scheduling is visible-first. Visible indexes are queued first;
    read-ahead follows Dolphin's order: after the visible range, before the
    visible range in reverse, last page, first page, then remaining indexes up
    to the resolve limit. Pane snapshots add candidates to the current
    pane-generation work set without pruning already-seen items just because
    the current visible/read-ahead range changed; that avoids zoom/scroll
    cycles cancelling and recreating the same thumbnail work. Queued deferred
    requests are promoted when the same item becomes visible. Work is cleared
    when the pane closes or its generation changes.
  - Matching failure markers are checked before visible work is enqueued. Skipped
    failures are remembered in the pane-local work key for the same mtime, and a
    later mtime change is allowed to enqueue a new request.
  - Core tests now cover the full deterministic external-thumbnailer path:
    parsing a local `.thumbnailer`, executing it, patching freedesktop
    `Thumb::URI` / `Thumb::MTime`, moving the result into `normal/`, and writing
    a matching failure marker on an attempted thumbnailer failure without
    retrying the same mtime.
- `src/ui/file_grid.rs`
  - Item rendering uses `thumbnail_path` when the preview role exists. The
    normal theme/MIME icon role is used only when no preview role exists,
    matching Dolphin's `iconPixmap` before `iconName` paint order. Thumbnail
    images do not change compact layout geometry and do not enter the theme icon
    cache. MIME/iconName role updates must leave an existing `thumbnail_path`
    intact, so a resolved preview is not replaced by the ordinary PNG/image
    theme icon.
- Pending visible thumbnail work is cancelled when panes navigate or close.

## Remaining Work

- Add true system end-to-end coverage for host-installed thumbnailers; the
  deterministic local-thumbnailer path and failure marker behavior are covered
  in core tests, but installed thumbnailer availability depends on the host.
