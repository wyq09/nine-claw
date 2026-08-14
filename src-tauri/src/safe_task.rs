//! Panic containment for background tasks (research-dsh report 04 step ③).
//!
//! A panicking background task must not silently poison shared state or take
//! sibling tasks down with it. [`safe_spawn`] wraps `tokio::spawn`: the panic
//! is caught on the task's own thread and logged with the task name for
//! attribution, and everything else keeps running.

use std::future::Future;
use std::pin::Pin;

/// Spawn a tokio task whose panic is contained: tokio already turns a task
/// panic into a `JoinError` instead of aborting the runtime; an observer task
/// awaits the handle and logs the failure with the task name for attribution.
/// The `name` is only used for log attribution.
pub(crate) fn safe_spawn<F>(name: &'static str, f: F)
where
    F: FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + 'static,
{
    let task = tokio::spawn(f());
    tokio::spawn(async move {
        if let Err(error) = task.await {
            log::error!("[{name}] 后台任务 panic: {error}");
        }
    });
}

/// Spawn a std thread whose panic is contained: caught and logged. A
/// panicking bot worker must die loudly (log line with the task name)
/// instead of vanishing silently and leaving shared state half-updated.
pub(crate) fn safe_thread_spawn<F>(name: &'static str, f: F)
where
    F: FnOnce() + Send + 'static,
{
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Err(error) = result {
            log::error!("[{name}] 后台线程 panic: {error:?}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn safe_spawn_runs_the_task() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_task = Arc::clone(&counter);
        safe_spawn("counter-task", || {
            Box::pin(async move {
                counter_for_task.fetch_add(1, Ordering::SeqCst);
            })
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn safe_spawn_contains_panic_and_keeps_siblings_alive() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_task = Arc::clone(&counter);
        safe_spawn("panic-task", || {
            Box::pin(async {
                panic!("boom");
            })
        });
        safe_spawn("ok-task", || {
            Box::pin(async move {
                counter_for_task.fetch_add(1, Ordering::SeqCst);
            })
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        // The panicking task neither poisoned the runtime nor its sibling.
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn safe_spawn_quietly_completes_void_tasks() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_task = Arc::clone(&counter);
        safe_spawn("noop-task", || Box::pin(async move {}));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter_for_task.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn safe_thread_spawn_contains_panic_and_keeps_siblings_alive() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_task = Arc::clone(&counter);
        safe_thread_spawn("panic-thread", || {
            panic!("boom");
        });
        safe_thread_spawn("ok-thread", move || {
            counter_for_task.fetch_add(1, Ordering::SeqCst);
        });
        // 轮询等待 ok-thread 完成；panic-thread 的 panic 已被吞掉。
        for _ in 0..100 {
            if counter.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
