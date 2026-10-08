//! Work that runs after the response has gone out (e.g. password-reset delivery), counted so
//! tests can wait for it.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
pub struct Detached {
    running: Arc<AtomicUsize>,
}

/// Decrements the counter however the task ends (a panic included).
struct Guard(Arc<AtomicUsize>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Detached {
    pub fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
        self.running.fetch_add(1, Ordering::SeqCst);
        let guard = Guard(self.running.clone());
        tokio::spawn(async move {
            work.await;
            drop(guard);
        });
    }

    pub fn idle(&self) -> bool {
        self.running.load(Ordering::SeqCst) == 0
    }

    /// Waits up to `limit` for running work to finish (graceful shutdown); returns how many tasks
    /// were still running when it gave up.
    pub async fn drain(&self, limit: std::time::Duration) -> usize {
        let deadline = tokio::time::Instant::now() + limit;
        while !self.idle() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        self.running.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn idle_again_after_the_work_ends_even_by_panic() {
        let d = Detached::default();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        d.spawn(async move {
            let _ = rx.await;
            panic!("boom");
        });
        assert!(!d.idle());
        let _ = tx.send(());
        for _ in 0..200 {
            if d.idle() {
                break;
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert!(d.idle());
    }

    #[tokio::test]
    async fn drain_waits_for_work_and_reports_what_it_abandons() {
        let d = Detached::default();
        d.spawn(tokio::time::sleep(std::time::Duration::from_millis(30)));
        assert_eq!(d.drain(std::time::Duration::from_secs(5)).await, 0);
        d.spawn(std::future::pending());
        assert_eq!(d.drain(std::time::Duration::from_millis(50)).await, 1);
    }
}
