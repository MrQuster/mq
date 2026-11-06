#![deny(clippy::await_holding_lock)]
//! Monotonic cancellation epochs paired with `CancellationToken`.
//!
//! Enables instant invalidation of in-flight async downloads, CPU jobs, and file
//! state transitions without risking ABA races when an installation or mod sync
//! is cancelled and immediately restarted.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

/// Error returned when an operation observes a cancelled or superseded epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpochCancelledError {
    /// The epoch carried by the cancelled operation.
    pub epoch: u64,
}

impl fmt::Display for EpochCancelledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "operation cancelled at epoch {}", self.epoch)
    }
}

impl std::error::Error for EpochCancelledError {}

/// Immutable snapshot of an active epoch and its associated [`CancellationToken`].
///
/// Cheap to clone and pass into async tasks or across thread boundaries.
#[derive(Debug, Clone)]
pub struct EpochGuard {
    /// Monotonic epoch number captured when this snapshot was taken.
    pub epoch: u64,
    /// Cooperative cancellation token tied to this specific epoch.
    pub token: CancellationToken,
}

impl EpochGuard {
    /// Constructs an [`EpochGuard`] from an explicit epoch and token.
    #[must_use]
    pub fn new(epoch: u64, token: CancellationToken) -> Self {
        Self { epoch, token }
    }

    /// Returns the numeric epoch value.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Borrows the underlying [`CancellationToken`].
    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Returns `true` if this epoch's token has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Resolves as soon as this epoch's [`CancellationToken`] is cancelled.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// Returns `Err(EpochCancelledError)` if this epoch has been cancelled.
    pub fn check(&self) -> Result<(), EpochCancelledError> {
        if self.token.is_cancelled() {
            Err(EpochCancelledError { epoch: self.epoch })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct CancelEpochInner {
    epoch: AtomicU64,
    token: RwLock<CancellationToken>,
}

/// Thread-safe monotonic cancellation coordinator.
///
/// Starts at epoch `1`. Calling [`CancelEpoch::cancel_and_advance`] atomically
/// cancels the previous [`CancellationToken`], installs a fresh token, and
/// increments the [`AtomicU64`] counter with [`Ordering::SeqCst`].
///
/// The internal `parking_lot::RwLock` is held only for nanoseconds during
/// [`CancelEpoch::snapshot`] and [`CancelEpoch::cancel_and_advance`] and is
/// never held across `.await`.
#[derive(Debug, Clone)]
pub struct CancelEpoch {
    inner: Arc<CancelEpochInner>,
}

impl Default for CancelEpoch {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelEpoch {
    /// Creates a new [`CancelEpoch`] initialized at epoch `1`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CancelEpochInner {
                epoch: AtomicU64::new(1),
                token: RwLock::new(CancellationToken::new()),
            }),
        }
    }

    /// Returns the current active epoch number.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.inner.epoch.load(Ordering::SeqCst)
    }

    /// Returns a reference to the underlying [`AtomicU64`] for lock-free CAS
    /// validation in `AtomicFileState`.
    #[must_use]
    pub fn atomic_epoch(&self) -> &AtomicU64 {
        &self.inner.epoch
    }

    /// Captures a linearizable `(epoch, CancellationToken)` snapshot.
    ///
    /// Acquires the read lock for nanoseconds only to clone the token alongside
    /// its matching epoch counter.
    #[must_use]
    pub fn snapshot(&self) -> EpochGuard {
        let guard = self.inner.token.read();
        let epoch = self.inner.epoch.load(Ordering::SeqCst);
        let token = guard.clone();
        drop(guard);
        EpochGuard { epoch, token }
    }

    /// Cancels all tasks running under the current epoch, installs a fresh
    /// [`CancellationToken`], increments the epoch with [`Ordering::SeqCst`],
    /// and returns the new active epoch.
    pub fn cancel_and_advance(&self) -> u64 {
        let mut guard = self.inner.token.write();
        guard.cancel();
        *guard = CancellationToken::new();
        let new_epoch = self.inner.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        drop(guard);
        new_epoch
    }

    /// Returns `true` if `expected_epoch` still matches the active epoch.
    #[must_use]
    pub fn is_valid(&self, expected_epoch: u64) -> bool {
        self.inner.epoch.load(Ordering::Acquire) == expected_epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn starts_at_one_and_advances_monotonically() {
        let ce = CancelEpoch::new();
        assert_eq!(ce.epoch(), 1);
        assert!(ce.is_valid(1));
        assert!(!ce.is_valid(0));

        let g1 = ce.snapshot();
        assert_eq!(g1.epoch(), 1);
        assert!(!g1.is_cancelled());
        assert!(g1.check().is_ok());

        let next = ce.cancel_and_advance();
        assert_eq!(next, 2);
        assert_eq!(ce.epoch(), 2);
        assert!(!ce.is_valid(1));
        assert!(ce.is_valid(2));
        assert!(g1.is_cancelled());
        assert_eq!(g1.check(), Err(EpochCancelledError { epoch: 1 }));

        let g2 = ce.snapshot();
        assert_eq!(g2.epoch(), 2);
        assert!(!g2.is_cancelled());
    }

    #[tokio::test]
    async fn snapshot_wakes_awaiters_immediately_on_advance() {
        let ce = CancelEpoch::new();
        let guard = ce.snapshot();

        let waiter = tokio::spawn(async move {
            guard.cancelled().await;
            guard.epoch
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!waiter.is_finished());

        let new_epoch = ce.cancel_and_advance();
        assert_eq!(new_epoch, 2);

        let cancelled_epoch = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("cancellation wakeup must be immediate")
            .expect("task must not panic");
        assert_eq!(cancelled_epoch, 1);
    }

    #[test]
    fn concurrent_snapshots_and_advances_remain_consistent() {
        let ce = CancelEpoch::new();
        std::thread::scope(|s| {
            let writer = ce.clone();
            s.spawn(move || {
                for _ in 0..500 {
                    writer.cancel_and_advance();
                }
            });

            for _ in 0..4 {
                let reader = ce.clone();
                s.spawn(move || {
                    let mut prev_epoch = 1;
                    for _ in 0..1000 {
                        let snap = reader.snapshot();
                        assert!(snap.epoch >= prev_epoch);
                        prev_epoch = snap.epoch;
                        // If the active epoch has already moved past snap.epoch,
                        // snap.token MUST already be cancelled.
                        if reader.epoch() > snap.epoch {
                            assert!(snap.is_cancelled());
                        }
                    }
                });
            }
        });

        assert_eq!(ce.epoch(), 501);
    }
}
