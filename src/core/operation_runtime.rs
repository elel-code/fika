use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use tokio::runtime::{Builder as TokioRuntimeBuilder, Runtime as TokioRuntime};
use tokio::sync::{mpsc, oneshot};

type CompioFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;
type CompioTask = Box<dyn FnOnce() -> CompioFuture + Send + 'static>;

pub struct OperationRuntime {
    compio_tx: mpsc::Sender<CompioTask>,
    _tokio_runtime: TokioRuntime,
}

impl OperationRuntime {
    fn new() -> Self {
        let tokio_runtime = TokioRuntimeBuilder::new_multi_thread()
            .enable_all()
            .thread_name("fika-operation-tokio")
            .build()
            .expect("failed to create Fika operation Tokio runtime");
        let tokio_handle = tokio_runtime.handle().clone();
        let (compio_tx, mut compio_rx) = mpsc::channel::<CompioTask>(1);

        std::thread::Builder::new()
            .name("fika-operation-compio".to_string())
            .spawn(move || {
                let _tokio = tokio_handle.enter();
                compio::runtime::RuntimeBuilder::new()
                    .build()
                    .expect("failed to create Fika operation Compio runtime")
                    .block_on(async move {
                        while let Some(task) = compio_rx.recv().await {
                            compio::runtime::spawn(task()).detach();
                        }
                    });
            })
            .expect("failed to start Fika operation Compio thread");

        Self {
            compio_tx,
            _tokio_runtime: tokio_runtime,
        }
    }

    pub fn shared() -> &'static Self {
        static OPERATION_RUNTIME: OnceLock<OperationRuntime> = OnceLock::new();
        OPERATION_RUNTIME.get_or_init(Self::new)
    }

    pub async fn submit<F, Fut, T>(&self, task: F) -> oneshot::Receiver<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
    {
        let (result_tx, result_rx) = oneshot::channel();
        let compio_task: CompioTask = Box::new(move || {
            Box::pin(async move {
                let result = task().await;
                let _ = result_tx.send(result);
            })
        });
        self.compio_tx
            .send(compio_task)
            .await
            .expect("Fika operation runtime stopped");
        result_rx
    }
}

pub async fn run_operation_task<F, Fut, T>(task: F) -> T
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + 'static,
    T: Send + 'static,
{
    OperationRuntime::shared()
        .submit(task)
        .await
        .await
        .expect("Fika operation runtime stopped before returning a result")
}

pub async fn run_operation_blocking<F, T>(task: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    compio::runtime::spawn_blocking(task)
        .await
        .expect("Fika operation blocking worker stopped")
}

#[cfg(test)]
mod tests {
    use super::{run_operation_blocking, run_operation_task};

    #[test]
    fn operation_runtime_runs_compio_and_blocking_tasks() {
        let result = futures_lite::future::block_on(run_operation_task(|| async {
            let blocking = run_operation_blocking(|| 21_u8).await;
            blocking * 2
        }));

        assert_eq!(result, 42);
    }

    #[test]
    fn operation_runtime_accepts_multiple_submitted_tasks() {
        let first = futures_lite::future::block_on(run_operation_task(|| async { 1_u8 }));
        let second = futures_lite::future::block_on(run_operation_task(|| async { 2_u8 }));

        assert_eq!((first, second), (1, 2));
    }
}
