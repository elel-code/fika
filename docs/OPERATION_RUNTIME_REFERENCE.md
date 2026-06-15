# Operation Runtime Reference

This document records the COSMIC Files operation-runtime reference and Fika's
current mapping. The short answer is: Fika now matches COSMIC Files at the
runtime boundary, but it does not copy COSMIC's full operation/controller model.

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
    the result through a Tokio `oneshot`.
  - `run_operation_blocking()` uses `compio::runtime::spawn_blocking` for sync
    fallbacks that still belong to the file-operation pipeline.
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
    `operation_pending`/single-operation gate.
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
The target is byte-for-byte alignment where possible; deviations must be
documented with justification.

### 1. Operation Model

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Operation type | `Operation` enum with variants per operation kind, unified submission path | Per-operation result types (`TransferTaskResult`, `TrashSelectionResult`, `UndoTaskResult`, etc.), no unified type | **Large**. No `Operation` enum means the runtime cannot introspect, pause, or generically control operations. |
| Operation id | `u64` assigned at submission, used as key in `pending_operations` map | `BackgroundTaskId` assigned in `main.rs` (GPUI layer), not part of runtime | **Large**. Operation identity is in the UI layer, not the runtime. The runtime cannot track operation lifecycle by id. |
| Controller | `Controller` struct with cancel/pause/progress state per operation | `AtomicBool` cancel flag (ad-hoc), `Arc<Mutex<TransferProgress>>` for progress | **Large**. No unified controller means cancel/progress logic is duplicated per operation kind. |

### 2. Operation Lifecycle Tracking

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Pending operations | `BTreeMap<u64, (Operation, Controller)>` on app state | `BTreeMap<BackgroundTaskId, ActiveBackgroundTaskRecord>` in `main.rs` | **Medium**. Fika tracks at GPUI level but the runtime itself has no knowledge. |
| Progress-capable ops | `BTreeSet<u64>` for operations that report progress | `OperationProgressHandle` passed per operation via GPUI task | **Medium**. COSMIC's set allows batch progress queries; Fika's handle is per-operation. |
| Completion routing | `oneshot` channel per operation, result returned to submission site | `oneshot` channel inside `run_operation_task()` | **Small**. Both use oneshot; Fika's is buried inside the helper. |
| Batch cancellation | Controller provides per-operation cancel; batch cancel via iteration | No batch cancel; only per-operation `AtomicBool` | **Medium**. Cancelling all operations requires tracking ids in UI layer. |

### 3. File I/O Strategy

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Async file copy | `operation/recursive.rs`: dedicated recursive copy using Compio async file APIs (`compio::fs`) | `file_ops.rs`: uses `compio::fs::File`, `AsyncReadAt`, `AsyncWriteAtExt` for single-file copy; recursive traversal is sync-first | **Medium**. Fika has compio I/O for individual files but recursive logic lacks a dedicated compio-backed module. |
| Directory operations | Compio async (`compio::fs::create_dir`, `rename`, `metadata`) | Same: `compio::fs::create_dir`, `compio::fs::rename`, `compio::fs::metadata` | **Aligned**. |
| Sync fallback | `compio::runtime::spawn_blocking` from inside Compio operation runtime | `run_operation_blocking()` → `compio::runtime::spawn_blocking` | **Aligned**. |
| GIO copy fallback | Routes GIO `File::copy()` through Compio blocking pool | Not implemented | **Missing**. GIO fallback for GVfs remote files is absent. |

### 4. Runtime Configuration

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Compio driver | Presumed `io-uring` on Linux | `polling` driver only | **Large**. `io-uring` is disabled; `compio` Cargo features: `fs`, `io`, `macros`, `polling`, `runtime`. |
| Tokio runtime | Multi-thread, standard configuration | Multi-thread with `enable_all()`, thread name `fika-operation-tokio` | **Aligned**. |
| Compio thread | Dedicated thread, channel(1), `block_on` loop | Dedicated thread `fika-operation-compio`, channel(1), `block_on` loop | **Aligned**. |
| Thread naming | Presumed default | Custom names for both threads | **Minor**. COSMIC may or may not name threads. |

### 5. Error Handling

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Operation errors | Structured error per `Operation` variant | `.expect()` panics on channel send failures and oneshot recv failures | **Medium**. Fika panics if the runtime stops; COSMIC likely propagates errors. |
| Cancel semantics | Controller sets cancelled state; operation checks and returns `Err` | `AtomicBool` checked at transfer boundaries; returns partial result | **Small**. Both support cancellation; Fika returns partial progress instead of an error. |

### 6. Cross-Runtime Bridge

| Layer | COSMIC Files | Fika (current) | Gap |
|-------|-------------|----------------|-----|
| Channel type | `mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>` | `mpsc::Sender<Box<dyn FnOnce() -> CompioFuture + Send>>` (closure, not future) | **Minor**. Fika sends a closure that creates the future; COSMIC sends the future directly. |
| Tokio context handoff | `tokio_handle.enter()` before building Compio runtime | Same: `tokio_handle.enter()` | **Aligned**. |
| Future detachment | `compio::runtime::spawn(task).detach()` | Same: `compio::runtime::spawn(task()).detach()` | **Aligned**. |

## Alignment Plan

The following items are ordered by priority. Each item should bring Fika
closer to COSMIC Files' runtime architecture while respecting Fika's GPUI
context (Fika does not use Iced/COSMIC's `Task::stream`).

### Phase 1: Foundation (low risk, high value)

1. **Enable `io-uring`** — Change `compio` features in `Cargo.toml` from
   `polling` to `io-uring` on Linux. This is a drop-in performance improvement
   with no API changes and matches COSMIC's presumed configuration.
2. **Introduce `OperationId`** — Add a core `OperationId(u64)` type to
   `operation_runtime.rs`. Return it from `submit()` alongside the `oneshot`
   receiver. This gives the runtime operation-level identity without the full
   `Operation` enum.
3. **Non-panicking error paths** — Replace `.expect()` calls with proper error
   propagation so a runtime shutdown can be handled gracefully by the GPUI
   layer.

### Phase 2: Structured Operations (medium risk)

4. **Define `Operation` enum** — Create a core `Operation` enum with variants
   matching current task types (`Transfer`, `Trash`, `Rename`, `Create`,
   `Undo`, `TrashView`). Each variant carries its input parameters. This
   unifies the submission path.
5. **Add `OperationController`** — A core struct with cancel flag, progress
   state, and pause capability. Operations check the controller at yield
   points. Replaces `AtomicBool` + `Arc<Mutex<TransferProgress>>` ad-hoc
   pattern.
6. **Runtime-level operation tracking** — Move `BTreeMap<OperationId,
   (Operation, OperationController)>` from `main.rs` into
   `OperationRuntime`. The GPUI layer queries the runtime for active
   operations instead of maintaining its own `active_background_tasks`.

### Phase 3: Recursive and GIO (high risk, deep integration)

7. **Recursive copy module** — Create `src/core/operations/recursive.rs` using
   Compio async APIs for directory traversal and file copy, matching COSMIC
   `operation/recursive.rs`. The current `transfer_paths_result` should be
   refactored to use this.
8. **GIO fallback** — Route GIO `File::copy()` through `spawn_blocking` from
   within the Compio operation runtime, matching COSMIC. Needed for GVfs
   remote filesystem operations.

### Non-Goals (intentional deviations from COSMIC)

- **Iced/COSMIC message routing**: Fika uses GPUI tasks, not Iced
  `Task::stream`. The operation result delivery will stay GPUI-native.
- **Pause/resume**: COSMIC's pause capability is not a current Fika
  requirement. The `OperationController` should support it structurally but
  Fika doesn't need to implement pause UI.
- **Cross-operation coordination**: COSMIC's `progress_operations: BTreeSet`
  for batch progress queries is deferred until Fika has parallel background
  operations that need coordinated progress display.

## Current Judgment (revised)

Fika is aligned with COSMIC Files for the thread/runtime boundary: Tokio
context plus a dedicated Compio operation thread, bounded `channel(1)`,
`block_on` receive loop, `spawn(task).detach()`, and `spawn_blocking` for
sync fallbacks. Compio async file APIs are already in use for copy/move/link.

However, Fika is **significantly behind** in the operation abstraction layer:
no `Operation` enum, no `Controller`, operation ids managed at the UI layer
rather than the runtime, and `io-uring` is not enabled. These gaps prevent
the runtime from being a self-contained operation execution engine and force
the GPUI layer to manage operation lifecycle details that belong in core.
