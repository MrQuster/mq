#![deny(clippy::await_holding_lock)]
//! Dedicated `tokio` <-> `rayon` runtime bridge.
//!
//! Offloads heavy CPU work (SHA1/SHA256/SHA512 hashing, ZIP/JAR extraction, JSON
//! parsing, PNG thumbnail decoding) strictly onto a named `rayon::ThreadPool`
//! (`"mq-cpu-{i}"`) and bridges completion back to `tokio` via `oneshot` or
//! bounded `flume` channels. Never uses `block_on` or `tokio::task::spawn_blocking`.

use std::any::Any;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::epoch::EpochGuard;

/// Errors returned by [`ComputePool`] CPU dispatch operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputeError {
    /// Failed to initialize the underlying `rayon::ThreadPool`.
    PoolBuild(String),
    /// The CPU closure panicked on a `rayon` worker thread; panic was caught safely.
    Panicked(String),
    /// The associated [`EpochGuard`] was cancelled before, during, or after execution.
    Cancelled,
    /// The internal `oneshot` channel closed unexpectedly without producing a value.
    ChannelClosed,
}

impl fmt::Display for ComputeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PoolBuild(msg) => write!(f, "failed to build rayon compute pool: {msg}"),
            Self::Panicked(msg) => write!(f, "rayon CPU job panicked: {msg}"),
            Self::Cancelled => f.write_str("rayon CPU job cancelled by epoch"),
            Self::ChannelClosed => f.write_str("rayon CPU bridge oneshot channel closed"),
        }
    }
}

impl std::error::Error for ComputeError {}

fn extract_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown non-string panic payload".to_string()
    }
}

/// Bounded CPU worker pool wrapping an `Arc<rayon::ThreadPool>`.
///
/// Worker threads are named `"mq-cpu-{i}"` for clear attribution in `/proc/stat`,
/// `perf`, and flamegraphs.
#[derive(Debug, Clone)]
pub struct ComputePool {
    pool: Arc<rayon::ThreadPool>,
    num_threads: usize,
}

impl Default for ComputePool {
    fn default() -> Self {
        Self::new(0)
    }
}

impl ComputePool {
    /// Builds a [`ComputePool`] with `num_threads` workers named `"mq-cpu-{i}"`.
    ///
    /// Passing `0` automatically selects `std::thread::available_parallelism()`.
    ///
    /// # Panics
    ///
    /// Panics only if OS thread creation fails; use [`ComputePool::try_new`] for
    /// fallible construction.
    #[must_use]
    pub fn new(num_threads: usize) -> Self {
        Self::try_new(num_threads).expect("failed to initialize mq-cpu rayon ThreadPool")
    }

    /// Fallibly builds a [`ComputePool`] with `num_threads` workers named `"mq-cpu-{i}"`.
    ///
    /// Passing `0` automatically selects `std::thread::available_parallelism()`.
    pub fn try_new(num_threads: usize) -> Result<Self, ComputeError> {
        let resolved_threads = if num_threads == 0 {
            std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(4)
        } else {
            num_threads
        };

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(resolved_threads)
            .thread_name(|idx| format!("mq-cpu-{idx}"))
            .build()
            .map_err(|err| ComputeError::PoolBuild(err.to_string()))?;

        Ok(Self {
            pool: Arc::new(pool),
            num_threads: resolved_threads,
        })
    }

    /// Number of worker threads configured in this pool.
    #[must_use]
    pub fn num_threads(&self) -> usize {
        self.num_threads
    }

    /// Borrows the underlying [`rayon::ThreadPool`].
    #[must_use]
    pub fn pool(&self) -> &rayon::ThreadPool {
        &self.pool
    }

    /// Dispatches `job` onto the `rayon` pool in FIFO order and awaits its result
    /// asynchronously in `tokio` via a `oneshot` channel.
    ///
    /// Any panic inside `job` is caught via `catch_unwind` and returned as
    /// [`ComputeError::Panicked`] rather than aborting the worker thread.
    pub async fn spawn_cpu<F, R>(&self, job: F) -> Result<R, ComputeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = oneshot::channel::<Result<R, ComputeError>>();

        self.pool.spawn_fifo(move || {
            let outcome = match catch_unwind(AssertUnwindSafe(job)) {
                Ok(val) => Ok(val),
                Err(panic_payload) => {
                    let msg = extract_panic_message(panic_payload);
                    tracing::error!(panic = %msg, "caught panic in mq-cpu worker");
                    Err(ComputeError::Panicked(msg))
                }
            };
            let _ = tx.send(outcome);
        });

        match rx.await {
            Ok(outcome) => outcome,
            Err(_) => Err(ComputeError::ChannelClosed),
        }
    }

    /// Dispatches `job` onto the `rayon` pool while honoring `epoch_guard`.
    ///
    /// Checks cancellation:
    /// 1. Before enqueuing onto `rayon`,
    /// 2. Inside the `rayon` worker immediately before running `job` (skipping queued work),
    /// 3. Concurrently via `tokio::select!` while awaiting completion, and
    /// 4. Immediately after `job` finishes before returning `Ok`.
    pub async fn spawn_cpu_cancellable<F, R>(
        &self,
        epoch_guard: &EpochGuard,
        job: F,
    ) -> Result<R, ComputeError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        if epoch_guard.is_cancelled() {
            return Err(ComputeError::Cancelled);
        }

        let (tx, mut rx) = oneshot::channel::<Result<R, ComputeError>>();
        let worker_token = epoch_guard.token.clone();

        self.pool.spawn_fifo(move || {
            if worker_token.is_cancelled() {
                let _ = tx.send(Err(ComputeError::Cancelled));
                return;
            }

            let outcome = match catch_unwind(AssertUnwindSafe(job)) {
                Ok(val) => {
                    if worker_token.is_cancelled() {
                        Err(ComputeError::Cancelled)
                    } else {
                        Ok(val)
                    }
                }
                Err(panic_payload) => {
                    let msg = extract_panic_message(panic_payload);
                    tracing::error!(panic = %msg, "caught panic in cancellable mq-cpu worker");
                    Err(ComputeError::Panicked(msg))
                }
            };
            let _ = tx.send(outcome);
        });

        tokio::select! {
            biased;
            () = epoch_guard.token.cancelled() => Err(ComputeError::Cancelled),
            recv_res = &mut rx => {
                match recv_res {
                    Ok(Ok(val)) => {
                        if epoch_guard.is_cancelled() {
                            Err(ComputeError::Cancelled)
                        } else {
                            Ok(val)
                        }
                    }
                    Ok(Err(err)) => Err(err),
                    Err(_) => Err(ComputeError::ChannelClosed),
                }
            }
        }
    }

    /// Synchronously installs `op` into this pool's `rayon` context for batch
    /// `par_iter` operations.
    pub fn install_sync<F, R>(&self, op: F) -> R
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        self.pool.install(op)
    }

    /// Bridges a streaming `rayon` producer to an async `tokio` consumer via a
    /// bounded `flume` channel with backpressure.
    #[must_use]
    pub fn spawn_cpu_stream<T, F>(
        &self,
        capacity: usize,
        producer: F,
    ) -> flume::Receiver<Result<T, ComputeError>>
    where
        T: Send + 'static,
        F: FnOnce(&flume::Sender<Result<T, ComputeError>>) + Send + 'static,
    {
        let (tx, rx) = flume::bounded(capacity.max(1));
        self.pool.spawn_fifo(move || {
            if let Err(panic_payload) = catch_unwind(AssertUnwindSafe(|| producer(&tx))) {
                let msg = extract_panic_message(panic_payload);
                tracing::error!(panic = %msg, "caught panic in streaming mq-cpu worker");
                let _ = tx.send(Err(ComputeError::Panicked(msg)));
            }
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epoch::CancelEpoch;
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Barrier;
    use std::time::Duration;

    #[tokio::test]
    async fn spawn_cpu_runs_on_named_rayon_threads_and_supports_parallelism() {
        let pool = ComputePool::new(4);
        assert_eq!(pool.num_threads(), 4);
        assert_eq!(pool.pool().current_num_threads(), 4);

        let thread_name = pool
            .spawn_cpu(|| {
                std::thread::current()
                    .name()
                    .unwrap_or("unnamed")
                    .to_string()
            })
            .await
            .expect("spawn_cpu must succeed");
        assert!(
            thread_name.starts_with("mq-cpu-"),
            "expected mq-cpu-* thread, got {thread_name}"
        );

        // Prove true multi-thread parallelism: 4 tasks rendezvous at a 4-party barrier.
        let barrier = Arc::new(Barrier::new(4));
        let mut tasks = Vec::with_capacity(4);
        for i in 0..4usize {
            let p = pool.clone();
            let b = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                p.spawn_cpu(move || {
                    b.wait();
                    i * 10
                })
                .await
            }));
        }

        let mut sum = 0usize;
        for t in tasks {
            sum += t.await.unwrap().unwrap();
        }
        assert_eq!(sum, 60);

        // Verify install_sync + par_iter inside spawn_cpu.
        let p_clone = pool.clone();
        let par_sum: u64 = pool
            .spawn_cpu(move || p_clone.install_sync(|| (1u64..=1000).into_par_iter().sum()))
            .await
            .expect("par_iter sum must succeed");
        assert_eq!(par_sum, 500_500);
    }

    #[tokio::test]
    async fn spawn_cpu_catches_worker_panics_cleanly() {
        let pool = ComputePool::new(2);

        let err = pool
            .spawn_cpu(|| -> u32 {
                panic!("corrupt zip central directory");
            })
            .await;

        assert_eq!(
            err,
            Err(ComputeError::Panicked(
                "corrupt zip central directory".to_string()
            ))
        );

        // Pool remains completely healthy after catching a panic.
        let next = pool.spawn_cpu(|| 42u32).await;
        assert_eq!(next, Ok(42));
    }

    #[tokio::test]
    async fn spawn_cpu_cancellable_aborts_before_and_during_execution() {
        let pool = ComputePool::new(2);
        let epoch = CancelEpoch::new();
        let guard1 = epoch.snapshot();

        // Pre-cancelled epoch does not even execute the closure.
        epoch.cancel_and_advance();
        let ran = Arc::new(AtomicBool::new(false));
        let ran_clone = Arc::clone(&ran);
        let res = pool
            .spawn_cpu_cancellable(&guard1, move || {
                ran_clone.store(true, Ordering::SeqCst);
                123
            })
            .await;
        assert_eq!(res, Err(ComputeError::Cancelled));
        assert!(!ran.load(Ordering::SeqCst));

        // Mid-flight cancellation returns immediately to Tokio caller via select!.
        let guard2 = epoch.snapshot();
        let (started_tx, started_rx) = oneshot::channel::<()>();
        let pool_clone = pool.clone();
        let join = tokio::spawn(async move {
            pool_clone
                .spawn_cpu_cancellable(&guard2, move || {
                    let _ = started_tx.send(());
                    std::thread::sleep(Duration::from_millis(200));
                    999
                })
                .await
        });

        started_rx.await.expect("worker must start");
        epoch.cancel_and_advance();

        let cancelled_res = tokio::time::timeout(Duration::from_millis(80), join)
            .await
            .expect("tokio caller must wake immediately on epoch cancel")
            .expect("task must not panic");
        assert_eq!(cancelled_res, Err(ComputeError::Cancelled));
    }

    #[tokio::test]
    async fn spawn_cpu_stream_bridges_via_bounded_flume() {
        let pool = ComputePool::new(2);
        let rx = pool.spawn_cpu_stream(2, |tx| {
            for i in 0..5u32 {
                if tx.send(Ok(i * 2)).is_err() {
                    break;
                }
            }
        });

        let mut collected = Vec::new();
        while let Ok(item) = rx.recv_async().await {
            collected.push(item.expect("stream item ok"));
        }
        assert_eq!(collected, vec![0, 2, 4, 6, 8]);
    }
}
