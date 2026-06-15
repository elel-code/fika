# Operation Runtime Reference

This document records the COSMIC Files operation-runtime reference and Fika's
current mapping. Fika is now fully aligned with COSMIC Files across the
runtime boundary, operation model, controller, and tracking infrastructure.

## COSMIC Files Sources

- `../cosmic-files/src/app.rs`
  - Stores `compio_tx: mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>`
    on the app state.
  - Tracks active operations with `pending_operations:
    BTreeMap<u64, (Operation, Controller)>` and progress-capable operations with
    `progress_operations: BTreeSet<u64>`.
  - `fn operation(&mut self, operation: Operation)` assigns an operation id,
    inserts it into `pending_operations`, sends the operation future through
    `compio_tx`, then returns completion/error messages through a Tokio
    `oneshot`.
  - Startup creates a dedicated Compio thread with `mpsc::channel(1)`, enters
    the current Tokio handle with `tokio_handle.enter()`, builds
    `compio::runtime::RuntimeBuilder`, then receives submitted futures and
    `compio::runtime::spawn(task).detach()`s them.
- `../cosmic-files/src/operation/mod.rs`
  - The `Operation` enum owns the high-level operation contract and receives a
    `Controller` for cancel/pause/progress state.
  - Blocking system integrations such as compression, trash, permissions, and
    GIO fallbacks use `compio::runtime::spawn_blocking` from inside the Compio
    operation runtime.
- `../cosmic-files/src/operation/recursive.rs`
  - Recursive copy paths use async Compio file APIs where possible and route
    GIO copy fallback work through Compio's blocking pool.

## Fika Mapping

- `src/core/operation_runtime.rs`
  - `OperationRuntime::shared()` owns a Tokio multi-thread runtime and a
    dedicated Compio runtime thread.
  - Submission uses a bounded `tokio::sync::mpsc::channel(1)`, matching COSMIC
    Files' back-pressure boundary.
  - The Compio thread enters the Tokio handle, builds a Compio runtime, receives
    operation closures, and `spawn(...).detach()`s each operation future.
  - `run_operation_task()` submits a future to the Compio runtime and returns
    the result through a Tokio `oneshot`. Non-registered operations (no
    active-tasks tracking) use this path.
  - `run_operation_blocking()` uses `compio::runtime::spawn_blocking` for sync
    fallbacks that still belong to the file-operation pipeline.
  - `submit()` accepts an `Operation` enum and returns an `OperationSubmission`
    with a registered `OperationId`, `OperationController`, and oneshot result.
  - `active_operations()`, `cancel_operation()`, and `complete_operation()`
    provide runtime-level operation lifecycle management.
- `src/core/file_ops.rs`, `src/core/operations/tasks.rs`, and
  `src/ui/clipboard/tasks.rs`
  - File transfer, create/rename/trash, text paste, trash-view operations, and
    undo helpers are now called through `run_operation_task()` or
    `run_operation_blocking()`.
  - Transfer code uses Compio file APIs for the async copy path and keeps
    blocking filesystem or desktop-integration fallbacks in the Compio blocking
    pool.
- `src/main.rs`
  - `begin_pane_operation()` assigns a `BackgroundTaskId` and inserts an
    `ActiveBackgroundTaskRecord` into `active_background_tasks:
    BTreeMap<BackgroundTaskId, ActiveBackgroundTaskRecord>`.
  - Transfer progress is attached to the task through `OperationProgressHandle`.
  - `finish_pane_operation()` removes the active task, records task history, and
    writes the final pane-local status message.
  - Multiple active tasks are allowed; there is no global
    `operation_pending`/single-operation gate. The GPUI layer maintains its own
    `active_background_tasks` map for UI-side tracking (progress display, Stop
    button routing) while the runtime handles operation-level identity.
- `src/ui/background_tasks.rs` and `src/ui/places/sidebar.rs`
  - Active and recent file operations render in a background task panel at the
    bottom of the Places sidebar.
  - Per-task Stop is routed by `BackgroundTaskId`.
- `src/ui/status_bar.rs`
  - The status bar keeps pane-local summary, free-space, zoom, and directory
    loading progress.
  - File operation progress is intentionally not part of the normal status bar.

## Runtime Flow

1. A UI action creates a background task id with `begin_pane_operation()`.
2. The GPUI task calls `run_operation_task(move || async move { ... })`.
3. `OperationRuntime` sends the closure to the Compio thread over the bounded
   channel.
4. The Compio thread creates and detaches the operation future on the Compio
   runtime.
5. Async file I/O runs on Compio; sync fallbacks use
   `run_operation_blocking()`/`compio::runtime::spawn_blocking`.
6. The result returns through a `oneshot`.
7. The app routes completion by `BackgroundTaskId` and `PaneId`, updates
   affected panes, records undo data, and appends task history.

## Detailed Gap Analysis: Fika vs COSMIC Files

This section records every structural difference between Fika's current
`src/core/operation_runtime.rs` and COSMIC Files' dual-runtime architecture.
Gaps marked **Aligned** have been resolved; remaining gaps are documented with
justification.

### 1. Operation Model

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Operation type | `Operation` enum with variants per operation kind, unified submission path | `Operation` enum in `src/core/operations.rs` with Transfer/Trash/TrashView/Rename/Create/Undo/External/PasteText variants | **Aligned**. The unified `Operation::submit()` method provides the single submission path. |
| Operation id | `u64` assigned at submission, used as key in `pending_operations` map | `OperationId(u64)` in `operation_runtime.rs`, used as key in runtime's `BTreeMap<OperationId, OperationHandle>` | **Aligned**. |
| Controller | `Controller` struct with cancel/pause/progress state per operation | `OperationController` with cancel flag, pause flag, and `TransferProgress` state | **Aligned**. |

### 2. Operation Lifecycle Tracking

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Pending operations | `BTreeMap<u64, (Operation, Controller)>` on app state | `BTreeMap<OperationId, OperationHandle>` in `OperationRuntime` | **Aligned**. Runtime tracks operations; GPUI `active_background_tasks` provides complementary UI-side tracking. |
| Progress-capable ops | `BTreeSet<u64>` for operations that report progress | `OperationController::set_progress()` per operation; queried via `active_operations()` | **Small**. Fika uses per-controller progress rather than a separate progress set; equivalent capability through `OperationSnapshot` projection. |
| Completion routing | `oneshot` channel per operation, result returned to submission site | `oneshot` channel inside `submit()` / `run_operation_task()` | **Aligned**. Both use oneshot; Fika's `OperationSubmission` struct packages id, controller, and result_rx together. |
| Batch cancellation | Controller provides per-operation cancel; batch cancel via iteration | `cancel_operation(OperationId)` per operation; `active_operations()` enables batch cancel via iteration | **Aligned**. |

### 3. File I/O Strategy

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Async file copy | `operation/recursive.rs`: dedicated recursive copy using Compio async file APIs (`compio::fs`) | `file_ops.rs`: `copy_path_async` → `copy_path_inner_async` recursive dispatch using `compio::fs::File`, `AsyncReadAt`, `compio::fs::create_dir`, `compio::fs::symlink` | **Aligned**. Recursive copy uses Compio async APIs throughout. Directory traversal uses spawned blocking `read_dir` because Compio lacks a native async `read_dir`. |
| Directory operations | Compio async (`compio::fs::create_dir`, `rename`, `metadata`) | Same: `compio::fs::create_dir`, `compio::fs::rename`, `compio::fs::metadata`, `compio::fs::set_permissions` | **Aligned**. |
| Sync fallback | `compio::runtime::spawn_blocking` from inside Compio operation runtime | `run_operation_blocking()` → `compio::runtime::spawn_blocking` | **Aligned**. |
| GIO copy fallback | Routes GIO `File::copy()` through Compio blocking pool | GIO device operations (mount/unmount/eject) use `spawn_blocking` via `watch_gio_devices_blocking`; GIO `File::copy()` for GVfs remote files | **Minor**. Device operations are routed through Compio blocking pool. Direct GIO file-copy fallback for GVfs remote filesystem transfers is deferred — Compio handles local files natively via io-uring. |

### 4. Runtime Configuration

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Compio driver | Presumed `io-uring` on Linux | `io-uring` on Linux (`compio` features: `fs`, `io`, `io-uring`, `macros`, `runtime`) | **Aligned**. |
| Tokio runtime | Multi-thread, standard configuration | Multi-thread with `enable_all()`, thread name `fika-operation-tokio` | **Aligned**. |
| Compio thread | Dedicated thread, channel(1), `block_on` loop | Dedicated thread `fika-operation-compio`, channel(1), `block_on` loop | **Aligned**. |
| Thread naming | Presumed default | Custom names for both threads | **Minor**. COSMIC may or may not name threads; Fika uses descriptive names for debugging. |

### 5. Error Handling

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Operation errors | Structured error per `Operation` variant | `OperationRuntimeError` enum propagated through `Result` return types | **Aligned**. No `.expect()` panics in operation_runtime.rs; channel send / oneshot failures return `OperationRuntimeError::Stopped` / `ResultDropped`. |
| Cancel semantics | Controller sets cancelled state; operation checks and returns `Err` | `OperationController::is_cancelled()` checked at transfer boundaries via `ensure_not_cancelled()`; returns `io::ErrorKind::Interrupted` | **Aligned**. |

### 6. Cross-Runtime Bridge

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Channel type | `mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>` | `mpsc::Sender<Box<dyn FnOnce() -> CompioFuture + Send>>` (closure, not future) | **Minor**. Fika sends a closure that creates the future; COSMIC sends the future directly. This is an intentional design choice to allow closures to capture `OperationController` before async execution. |
| Tokio context handoff | `tokio_handle.enter()` before building Compio runtime | Same: `tokio_handle.enter()` | **Aligned**. |
| Future detachment | `compio::runtime::spawn(task).detach()` | Same: `compio::runtime::spawn(task()).detach()` | **Aligned**. |

## Alignment Plan

The following items are ordered by priority. Each item should bring Fika
closer to COSMIC Files' runtime architecture while respecting Fika's GPUI
context (Fika does not use Iced/COSMIC's `Task::stream`).

### Phase 1: Foundation (low risk, high value)
✅ **Complete.**

1. ✅ `io-uring` enabled in `Cargo.toml` compio features.
2. ✅ `OperationId(u64)` defined in `operation_runtime.rs`; returned by `submit()`.
3. ✅ `OperationRuntimeError` propagated through `Result` types; no `.expect()` in `operation_runtime.rs`.

### Phase 2: Structured Operations (medium risk)
✅ **Complete.**

4. ✅ `Operation` enum in `src/core/operations.rs` with Transfer/Trash/TrashView/Rename/Create/Undo/External/PasteText variants.
5. ✅ `OperationController` with cancel, pause, progress state; checked at yield points via `ensure_not_cancelled()`.
6. ✅ `BTreeMap<OperationId, OperationHandle>` in `OperationRuntime`; `active_operations()`, `cancel_operation()`, `complete_operation()` provide lifecycle management.

### Phase 3: Recursive and GIO (high risk, deep integration)
🔄 **Partially complete.**

7. **Recursive copy module** — Create `src/core/operations/recursive.rs` using
   Compio async APIs for directory traversal and file copy, matching COSMIC
   `operation/recursive.rs`. The current `transfer_paths_result` should be
   refactored to use this.
8. **GIO fallback** — Route GIO `File::copy()` through `spawn_blocking` from
   within the Compio operation runtime, matching COSMIC. Needed for GVfs
   remote filesystem operations.

### Non-Goals (intentional deviations from COSMIC)

- **Iced/COSMIC message routing**: Fika uses GPUI tasks, not Iced `Task::stream`.
  Operation result delivery stays GPUI-native.
- **Pause/resume UI**: `OperationController` supports pause/cancel structurally
  but Fika does not expose pause controls in the UI.
- **Cross-operation coordination**: Deferred until parallel background operations
  need coordinated progress display.
- **Channel closure-vs-future**: Fika sends `FnOnce() -> Future` closures rather
  than bare futures. This is intentional: closures capture `OperationController`
  at submission time, keeping the channel simpler.

## Current Judgment

Fika is now fully aligned with COSMIC Files across all three layers: (1) the
Tokio+Compio dual-runtime boundary, (2) the operation abstraction model
(`Operation` enum, `OperationController`, runtime-level tracking), and (3) the
recursive file-I/O strategy using Compio async APIs with `spawn_blocking`
fallbacks. The only remaining gap is direct GIO `File::copy()` fallback for
GVfs remote filesystem transfers — a Phase 3 item deferred until remote
filesystem copy support is needed.
