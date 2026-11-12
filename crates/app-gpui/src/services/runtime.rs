//! The application-owned tokio runtime. Async core calls run here; results are awaited from
//! GPUI foreground tasks, so the UI thread never blocks.

use std::{future::Future, sync::OnceLock};
use tokio::runtime::{Builder, Runtime};

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("translator-runtime")
            .build()
            .expect("tokio runtime")
    })
}

const TASK_FAILED: &str = "Внутрішня помилка програми. Спробуйте ще раз.";

/// Runs a future on the runtime; the returned future can be awaited on any executor.
pub async fn run<T, F>(future: F) -> Result<T, String>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    runtime().spawn(future).await.map_err(|_| TASK_FAILED.into())
}

/// Runs blocking work (disk, OS enumeration) off the UI thread.
pub async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    runtime()
        .spawn_blocking(work)
        .await
        .map_err(|_| TASK_FAILED.into())
}
