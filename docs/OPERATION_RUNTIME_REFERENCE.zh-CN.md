> 本文是 [OPERATION_RUNTIME_REFERENCE.md](OPERATION_RUNTIME_REFERENCE.md) 的简体中文翻译。

# 操作运行时参考

本文档记录 COSMIC Files 操作运行时参考和 Fika 的当前映射。Fika 现已在运行时边界、操作模型、控制器和跟踪基础设施方面与 COSMIC Files 完全对齐。

## COSMIC Files 源码

- `../cosmic-files/src/app.rs`
  - 在 app state 存储 `compio_tx: mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>`。
  - 用 `pending_operations: BTreeMap<u64, (Operation, Controller)>` 跟踪活动操作，`progress_operations: BTreeSet<u64>` 跟踪可报告进度的操作。
  - `fn operation(&mut self, operation: Operation)` 分配操作 id，插入 `pending_operations`，通过 `compio_tx` 发送操作 future，然后通过 Tokio `oneshot` 返回完成/错误消息。
  - 启动时创建专用 Compio 线程，进入当前 Tokio handle，构建 `compio::runtime::Runtime`，接收提交的 futures 并 `spawn(task).detach()`。
- `../cosmic-files/src/operation/mod.rs`
  - `Operation` 枚举拥有高级操作合约并接收 `Controller` 用于 cancel/pause/progress 状态。
  - 阻塞系统集成（压缩、回收站、权限、GIO fallback）在 Compio 操作运行时内使用 `compio::runtime::spawn_blocking`。
- `../cosmic-files/src/operation/recursive.rs`
  - 递归复制路径在可能时使用异步 Compio 文件 API，将 GIO 复制 fallback 通过 Compio 阻塞池路由。

## Fika 映射

- `src/core/operation_runtime.rs`
  - `OperationRuntime::shared()` 拥有 Tokio multi-thread 运行时和专用 Compio 运行时线程。
  - 使用 `tokio::sync::mpsc::channel(1)` 有界提交，匹配 COSMIC Files 的背压边界。
  - Compio 线程进入 Tokio handle，构建 Compio 运行时，接收操作闭包，`spawn(...).detach()`。
  - `run_operation_task()` 向 Compio 运行时提交 future 并通过 Tokio `oneshot` 返回结果。
  - `run_operation_blocking()` 使用 `compio::runtime::spawn_blocking`。
  - `submit()` 接受 `Operation` 枚举并返回 `OperationSubmission` 带 `OperationId`、`OperationController` 和 oneshot 结果。
  - `active_operations()`、`cancel_operation()`、`complete_operation()` 提供运行时级操作生命周期管理。
- `src/core/file_ops.rs`、`src/core/operations/tasks.rs` 和 `src/ui/clipboard/tasks.rs`
  - 文件传输、创建/重命名/回收站、文本粘贴、回收站视图操作和 undo helper 现在通过 `run_operation_task()` 或 `run_operation_blocking()` 调用。
  - 传输代码使用 Compio 文件 API 进行异步复制路径。

## 六层对齐对比

Fika 现已在所有六层与 COSMIC Files 完全对齐：(1) Tokio+Compio 双运行时边界，(2) 操作抽象模型（`Operation` 枚举、`OperationController`、运行时级跟踪），(3) 使用 Compio 异步 API 和 `spawn_blocking` fallback 的递归文件 I/O 策略（含 GIO `File::copy()` 用于 GVfs 远程文件传输）。

四个阶段也全部完成：Phase 1（`io-uring` 开启、`OperationId`、错误传播），Phase 2（`Operation` 枚举、`OperationController`、`BTreeMap` 跟踪），Phase 3（递归复制、GIO fallback）。

Fika 的两个刻意偏差：(1) 使用 GPUI tasks 而非 Iced `Task::stream`；(2) channel 发送 `FnOnce() -> Future` 闭包而非裸 future，使闭包可在提交时捕获 `OperationController`。
