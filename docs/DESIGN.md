# Fika Design: GPUI Target Architecture

本文档描述未来目标架构。当前 Slint 代码是旧实现，不再作为设计目标。后续工作以
`docs/GPUI_DOLPHIN_MIGRATION_PLAN.md` 为执行计划，以本地 `../dolphin` 源码为第一参考。

## Goals

- 用 GPUI 重建 Fika UI shell，消除 `.slint` 静态结构带来的 slot、sidecar model、callback
  glue 和 focused-pane fallback。
- 保留并清理可以复用的 Rust core：file operations、trash、MIME/open-with、devices、
  thumbnail、settings、portal/helper 边界。
- 以 Dolphin 的 directory lister/model/view/controller 执行流作为第一参考目标。
- 每个 pane 都是稳定 identity：`PaneId + generation` 是所有异步结果和 UI 事件的路由边界。
- UI 只渲染 state 和派发 input action；目录模型变化只通过 model/lister event 进入 view。

## Non-Goals

- 不把现有 `.slint` 文件翻译为 GPUI widget。
- 不保留 Slint 兼容层、slot 兼容层、旧 reload queue 或 focused-pane fallback。
- 不追求一次性复制 Dolphin 的所有 KDE/KIO 后端。第一阶段只实现本地文件系统，但执行流必须
  能映射到 Dolphin。
- 不在 GPUI view 里直接执行阻塞 I/O。

## Reference Priority

1. Dolphin source execution flow.
2. Linux desktop specifications and services used by Dolphin-compatible behavior:
   XDG trash, freedesktop thumbnails, MIME apps, service menus, UDisks2, Polkit.
3. Existing Fika Rust modules, only when they do not conflict with the Dolphin flow.
4. GPUI idioms for entity/view/state composition.

## Target Crate Layout

Planned structure:

```text
crates/
  fika-core/
    pane/
    directory/
    model/
    operations/
    trash/
    thumbnails/
    devices/
    desktop/
    settings/
  fika-gpui/
    app.rs
    window.rs
    pane_view.rs
    item_view.rs
    menus.rs
    dialogs.rs
    input.rs
  fika-portal/
  fika-privileged-helper/
```

Core rules:

- `fika-core` must not depend on GPUI, Slint, window handles, or UI image/model types.
- `fika-gpui` owns presentation, input routing, focus, menus, animations, and entity lifecycle.
- Async workers communicate with core through typed events. UI receives already-routed pane events.
- Every long-running operation carries enough identity to reject stale results.

## Core Model

### Pane

`PaneState` is a core object, not a UI slot.

Required fields:

- `PaneId`
- `generation`
- `current_dir`
- history back/forward stacks
- `DirectoryModel`
- `DirectoryListerHandle`
- selection
- search/filter state
- view state: layout mode, scroll offset, zoom, visible range cache
- thumbnail pending/cache handles

Opening or closing split panes creates or drops pane entities. It must not clone global UI state or share watcher state.

### Directory Lister

The lister mirrors Dolphin's `KDirLister -> KFileItemModel` relationship.

Inputs:

- `load_directory(path)`
- `refresh_directory(path)`
- watcher events
- file operation affected-directory refresh

Outputs:

- `DirectoryLoadingStarted`
- `DirectoryItemsAdded`
- `DirectoryItemsDeleted`
- `DirectoryItemsRefreshed`
- `DirectoryListingCompleted`
- `DirectoryCurrentRemoved`
- `DirectoryError`

All outputs carry `PaneId`, `generation`, and `path`.

### Directory Model

`DirectoryModel` owns entries, sorting, filtering, trash metadata, and path-index lookup.

The model emits Dolphin-style deltas:

- add items into pending/visible ranges
- delete item ranges
- refresh item data and preserve stable item identity where applicable
- full listing refresh only when the lister cannot classify a delta

The GPUI item view consumes model signals and snapshots. It does not decide whether a filesystem event is an add, delete, refresh, or full reload.

## GPUI UI Layer

### App Entity

The app entity owns:

- global settings
- theme
- pane tree
- Places and Devices models
- global menus/dialog state
- async event receiver

### Pane Entity

Each pane entity owns a core `PaneState` handle and renders:

- path/navigation row
- optional search/filter row
- file item view
- pane status row

The pane entity receives input locally and emits controller actions. It never routes through "currently focused pane" unless the command is explicitly global, such as `Ctrl+L` on the focused pane.

### Item View

The item view follows Dolphin's `KItemListView` boundary:

- model signals create insert/remove/change layout work
- layouter owns compact/details/icons geometry
- controller owns hit-test, selection, activation, DnD, keyboard navigation
- renderer owns visible item drawing

GPUI allows this to stay in Rust entities without projecting every intermediate row through a static UI language.

## Async and Stale Result Policy

Every async result must include:

- `PaneId` when pane-scoped
- `generation`
- request serial when multiple same-generation requests can overlap
- source path or operation id

Apply path:

1. Receive event.
2. Resolve pane by `PaneId`.
3. Check generation and path.
4. Apply to core model.
5. Notify GPUI entity/view.

No async result may apply by focused pane.

## Undo and File Operation Policy

Undo follows Dolphin's model: the operation performs filesystem changes; directory lister/model signals update the visible view.

Fika-specific requirements:

- undo start takes a serial
- undo finish with stale serial is ignored
- affected directories are mapped to pane ids
- each affected pane calls lister refresh
- UI does not manually rebuild item views after undo

## Slint Archive Policy

The old Slint implementation may be read for:

- file operation behavior
- trash edge cases
- thumbnail cache behavior
- MIME/open-with/service-menu parsing
- UDisks2 diagnostics
- tests that encode known bugs

It must not be used as:

- UI architecture reference
- pane identity reference
- directory refresh execution-flow reference
- compatibility surface for GPUI

## Acceptance Definition

The GPUI rewrite is architecturally acceptable when:

- single pane and split pane both refresh correctly from external filesystem changes
- undo refreshes visible directories without manual UI rebuilds
- closing a pane drops its lister/watcher and cannot receive stale results
- two panes showing the same directory have independent selection, scroll, generation, and watcher state
- Dolphin source references exist for every core file-view execution path
- Slint is no longer in the primary build dependency graph
