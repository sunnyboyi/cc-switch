//! Shared services use the desktop runtime unchanged, or a standalone Tokio runtime.

#[cfg(all(test, feature = "desktop"))]
pub use tauri::async_runtime::handle;
#[cfg(feature = "desktop")]
pub use tauri::async_runtime::{block_on, spawn, spawn_blocking};

#[cfg(not(feature = "desktop"))]
mod headless {
    use std::{future::Future, sync::OnceLock};
    use tokio::runtime::Runtime;
    use tokio::task::JoinHandle;

    fn runtime() -> &'static Runtime {
        static RUNTIME: OnceLock<Runtime> = OnceLock::new();
        RUNTIME.get_or_init(|| Runtime::new().expect("failed to initialize service runtime"))
    }

    #[cfg(test)]
    pub fn handle() -> tokio::runtime::Handle {
        runtime().handle().clone()
    }

    pub fn block_on<F: Future>(future: F) -> F::Output {
        runtime().block_on(future)
    }

    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        runtime().spawn(future)
    }

    pub fn spawn_blocking<F, R>(function: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        runtime().spawn_blocking(function)
    }
}

#[cfg(all(test, not(feature = "desktop")))]
pub use headless::handle;
#[cfg(not(feature = "desktop"))]
pub use headless::{block_on, spawn, spawn_blocking};
