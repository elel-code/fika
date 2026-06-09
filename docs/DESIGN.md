# Fika Design: GPUI Architecture

本文档描述当前 GPUI 主线架构。实现边界以根 Cargo package 和 `src/` 源码目录为准；Dolphin 源码执行流仍是目录加载、刷新、model signal 和 current-directory-removed 行为的第一参考。

## Goals

- 用 GPUI 承载窗口、pane、输入路由和渲染。
- 保持 `fika-core` UI-neutral：core 不依赖 GPUI、窗口句柄或 UI model 类型。
- 每个 pane 都有稳定 identity：`PaneId + generation` 是 lister、watcher、async result 和 UI event 的路由边界。
- 目录变化通过 lister event 进入 `DirectoryModel`，GPUI 层只渲染 snapshot 并派发 action。
- 旧 UI 主路径不再存在；新功能只进入 GPUI/core 主路径。

## Non-Goals

- 不翻译旧 UI 文件。
- 不保留旧 slot、focused-pane fallback 或 reload queue。
- 不一次性复制 Dolphin 的所有 KDE/KIO 后端。当前主线先保住本地目录、pane identity 和 portal/helper 边界。
- 不在 GPUI render/input 路径中执行阻塞 I/O。

## Reference Priority

1. Dolphin source execution flow.
2. Linux desktop specifications and services used by Dolphin-like behavior:
   XDG trash, freedesktop thumbnails, MIME apps, service menus, UDisks2, Polkit.
3. Existing `fika-core` modules when they preserve the Dolphin-style flow.
4. GPUI idioms for entity, view, state, and input composition.

## Source Layout

```text
src/
  lib.rs                     UI-neutral core module exports
  entries.rs                 directory entry metadata and sorting input
  directory.rs               directory lister, watcher events, generation checks
  model.rs                   directory model snapshots and model signals
  pane.rs                    pane identity, pane state, split/close routing
  operations.rs              operation controller boundary
  file_ops.rs                file transfer/trash/create/rename primitives
  privilege.rs               privileged helper API surface
  main.rs                    GPUI window, pane rendering, chooser shell
  bin/fika-xdp-filechooser.rs
                             portal FileChooser backend
  bin/fika-privileged-helper.rs
                             system-bus privileged helper binary
```

Root `Cargo.toml` is a single package. It exposes the `fika_core` library from
`src/lib.rs` and builds the `fika`, `fika-xdp-filechooser`, and
`fika-privileged-helper` binaries from `src/main.rs` and `src/bin/`. GPUI
is sourced from the official Zed repository with a git dependency and no numeric
crate release pin.

## Core Model

### Pane

`PaneState` is a core object, not a UI slot. It owns:

- `PaneId`
- `generation`
- `current_dir`
- `DirectoryModel`
- `DirectoryLister`
- watcher state

Opening or closing split panes creates or drops pane state. It must not clone global UI state or share watcher state.

### Directory Lister

The lister mirrors Dolphin's `KDirLister -> KFileItemModel` boundary.

Inputs:

- load directory
- reload current directory
- watcher refresh
- current-directory-removed detection

Outputs:

- `LoadingStarted`
- `ItemsAdded`
- `ItemsDeleted`
- `ItemsRefreshed`
- `ListingCompleted`
- `CurrentDirectoryRemoved`
- `Error`

All outputs carry `PaneId`, `generation`, and path context so stale events can be rejected.

### Directory Model

`DirectoryModel` owns entries and emits model signals:

- reset on new listing
- insert item ranges
- delete item ranges
- refresh item ranges
- report loading/error state

The GPUI pane consumes snapshots and signals. It does not decide whether a filesystem event is an add, delete, refresh, or full reload.

## GPUI Layer

The current GPUI shell owns:

- window creation through `gpui_platform::application()`
- pane toolbar actions
- split/close/focus routing by `PaneId`
- directory item rendering
- watcher polling handoff into core events
- pane-local selection, navigation shortcuts, and manager actions
- background file-operation tasks that return affected directories
- chooser path output and portal metadata output

Rendering is intentionally thin. Feature work should move domain logic into
`fika-core` first, then expose it through GPUI actions.

## Async and Stale Result Policy

Every pane-scoped async result must include:

- `PaneId`
- `generation`
- source path or operation id

Apply path:

1. Receive event.
2. Resolve pane by `PaneId`.
3. Check generation and path.
4. Apply to core model.
5. Notify GPUI view.

No pane-scoped async result may apply by focused pane.

## Undo and File Operation Policy

File operations belong in core. UI actions should produce operation requests; operation completion should return affected directories and trigger lister refresh for panes that show those directories.

Undo follows the same rule: filesystem change first, affected pane refresh second, no manual item-view rebuild in the UI layer.

## Historical Docs

The archived optimization documents describe the removed UI implementation and are not architecture input for new code. They may be read only for behavior notes and bug history.

## Acceptance Definition

The GPUI architecture is acceptable when:

- single pane and split pane refresh correctly from external filesystem changes
- closing a pane drops its lister/watcher and cannot receive stale results
- two panes showing the same directory have independent generation and watcher state
- current-directory-removed uses nearest existing ancestor fallback
- portal and privileged-helper binaries build from the root package
- the main build has no dependency on the removed UI implementation
