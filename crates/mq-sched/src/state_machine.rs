#![deny(clippy::await_holding_lock)]
//! Lock-free file lifecycle state machine with epoch validation and sharded
//! O(1) concurrent in-flight download deduplication.

use std::fmt;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use tokio::sync::watch;

/// Lifecycle state of an artifact or asset during installation/verification.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileState {
    /// Queued and waiting to be downloaded or verified on disk.
    Pending = 0,
    /// Network stream is actively writing to a temporary file.
    Downloading = 1,
    /// Checksum (SHA1/SHA256/SHA512) verification is running on CPU workers.
    Verifying = 2,
    /// Artifact is verified and atomically finalized at its destination path.
    Done = 3,
    /// Download or checksum verification failed.
    Failed = 4,
    /// Transition aborted because its cancellation epoch was superseded.
    Cancelled = 5,
}

impl FileState {
    /// Converts a raw `u8` tag back into a [`FileState`].
    #[must_use]
    pub const fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Pending),
            1 => Some(Self::Downloading),
            2 => Some(Self::Verifying),
            3 => Some(Self::Done),
            4 => Some(Self::Failed),
            5 => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Returns `true` if this state represents a terminal outcome (`Done`, `Failed`, `Cancelled`).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }

    /// Returns `true` if `self -> next` is a legal lifecycle transition.
    ///
    /// Enforces that `Done` can only be entered from `Verifying` and that `Done`
    /// is strictly final unless explicitly reset.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Pending,
                Self::Downloading | Self::Verifying | Self::Failed | Self::Cancelled
            ) | (
                Self::Downloading,
                Self::Verifying | Self::Failed | Self::Cancelled
            ) | (Self::Verifying, Self::Done | Self::Failed | Self::Cancelled)
                | (Self::Failed | Self::Cancelled, Self::Pending)
        )
    }
}

impl fmt::Display for FileState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Pending => "Pending",
            Self::Downloading => "Downloading",
            Self::Verifying => "Verifying",
            Self::Done => "Done",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        };
        f.write_str(label)
    }
}

/// Errors produced by [`AtomicFileState::try_transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTransitionError {
    /// Operation's `current_epoch` no longer matches `active_epoch`; non-`Done`
    /// state was atomically transitioned to [`FileState::Cancelled`].
    StaleEpoch,
    /// Atomic CAS observed `actual` instead of `expected`.
    UnexpectedState {
        /// Expected starting state.
        expected: FileState,
        /// Actual observed state.
        actual: FileState,
    },
    /// Requested `from -> to` edge violates the state machine invariant.
    InvalidTransition {
        /// Starting state.
        from: FileState,
        /// Rejected target state.
        to: FileState,
    },
}

impl fmt::Display for StateTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleEpoch => f.write_str("stale cancellation epoch; file marked Cancelled"),
            Self::UnexpectedState { expected, actual } => {
                write!(
                    f,
                    "unexpected file state: expected {expected}, observed {actual}"
                )
            }
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid file state transition: {from} -> {to}")
            }
        }
    }
}

impl std::error::Error for StateTransitionError {}

/// Lock-free atomic state machine for a single file's download/verify lifecycle.
#[derive(Debug)]
pub struct AtomicFileState {
    state: AtomicU8,
}

/// Alias matching architecture documentation for [`AtomicFileState`].
pub type FileStateMachine = AtomicFileState;

impl Default for AtomicFileState {
    fn default() -> Self {
        Self::new(FileState::Pending)
    }
}

impl AtomicFileState {
    /// Creates an [`AtomicFileState`] initialized to `initial`.
    #[must_use]
    pub const fn new(initial: FileState) -> Self {
        Self {
            state: AtomicU8::new(initial as u8),
        }
    }

    /// Loads the current [`FileState`] with [`Ordering::Acquire`].
    #[must_use]
    pub fn load(&self) -> FileState {
        let raw = self.state.load(Ordering::Acquire);
        FileState::from_u8(raw).expect("AtomicFileState only stores valid FileState discriminants")
    }

    /// Unconditionally resets the state (e.g., when re-queueing for a fresh install pass).
    pub fn store(&self, state: FileState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Atomically transitions any non-`Done` state to [`FileState::Cancelled`].
    fn cancel_unless_done(&self) {
        let mut current = self.state.load(Ordering::Acquire);
        while current != FileState::Done as u8 && current != FileState::Cancelled as u8 {
            match self.state.compare_exchange_weak(
                current,
                FileState::Cancelled as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    /// Attempts a lock-free `expected -> next` CAS guarded by `active_epoch`.
    ///
    /// If `current_epoch != active_epoch.load(Ordering::Acquire)`, atomically
    /// marks the file [`FileState::Cancelled`] (unless already [`FileState::Done`])
    /// and returns `Err(StateTransitionError::StaleEpoch)`.
    pub fn try_transition(
        &self,
        expected: FileState,
        next: FileState,
        current_epoch: u64,
        active_epoch: &AtomicU64,
    ) -> Result<FileState, StateTransitionError> {
        if current_epoch != active_epoch.load(Ordering::Acquire) {
            self.cancel_unless_done();
            return Err(StateTransitionError::StaleEpoch);
        }

        if !expected.can_transition_to(next) {
            return Err(StateTransitionError::InvalidTransition {
                from: expected,
                to: next,
            });
        }

        match self.state.compare_exchange(
            expected as u8,
            next as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // Re-verify active_epoch after CAS so a concurrent epoch advance
                // cannot allow a stale transition to slip through.
                if current_epoch != active_epoch.load(Ordering::Acquire) {
                    if next != FileState::Done {
                        self.cancel_unless_done();
                    }
                    return Err(StateTransitionError::StaleEpoch);
                }
                Ok(next)
            }
            Err(actual_raw) => {
                let actual = FileState::from_u8(actual_raw)
                    .expect("AtomicFileState only stores valid FileState discriminants");
                Err(StateTransitionError::UnexpectedState { expected, actual })
            }
        }
    }
}

/// Shared notification slot for an in-flight deduplicated download/operation.
#[derive(Debug)]
pub struct InFlightSlot {
    tx: watch::Sender<Option<Result<(), String>>>,
    // Keep at least one Receiver alive so `watch::Sender` retains its final value.
    _rx_anchor: watch::Receiver<Option<Result<(), String>>>,
}

impl InFlightSlot {
    fn new() -> Self {
        let (tx, rx) = watch::channel(None);
        Self { tx, _rx_anchor: rx }
    }

    fn subscribe(&self) -> watch::Receiver<Option<Result<(), String>>> {
        self.tx.subscribe()
    }
}

/// RAII guard held by the single leader executing an in-flight operation for key `K`.
///
/// If dropped without calling [`InFlightLeaderGuard::complete_ok`] or
/// [`InFlightLeaderGuard::complete_err`] (for example due to task cancellation
/// or panic), `Drop` removes the key from the [`InFlightMap`] and broadcasts
/// `Err("leader cancelled or dropped")` so followers never deadlock.
#[derive(Debug)]
pub struct InFlightLeaderGuard<K: Eq + Hash + Clone> {
    key: Option<K>,
    slot: Arc<InFlightSlot>,
    map: Arc<DashMap<K, Arc<InFlightSlot>>>,
    completed: bool,
}

impl<K: Eq + Hash + Clone> InFlightLeaderGuard<K> {
    /// Borrows the deduplication key owned by this leader.
    #[must_use]
    pub fn key(&self) -> &K {
        self.key
            .as_ref()
            .expect("key exists until InFlightLeaderGuard finishes")
    }

    /// Marks the operation as succeeded, removes the key from the map, and wakes
    /// all waiting followers with `Ok(())`.
    pub fn complete_ok(self) {
        self.complete(Ok(()));
    }

    /// Marks the operation as failed, removes the key from the map, and wakes
    /// all waiting followers with `Err(err)`.
    pub fn complete_err(self, err: impl Into<String>) {
        self.complete(Err(err.into()));
    }

    /// Completes the operation with an explicit `Result<(), String>`.
    pub fn complete(mut self, result: Result<(), String>) {
        self.finish_internal(result);
    }

    fn finish_internal(&mut self, result: Result<(), String>) {
        if self.completed {
            return;
        }
        self.completed = true;
        if let Some(key) = self.key.take() {
            // Remove from DashMap BEFORE broadcasting on watch::Sender so any
            // subscriber registered while the entry was occupied is guaranteed
            // to observe the state transition without race windows.
            let slot_ref = &self.slot;
            self.map.remove_if(&key, |_, v| Arc::ptr_eq(v, slot_ref));
        }
        let _ = self.slot.tx.send_replace(Some(result));
    }
}

impl<K: Eq + Hash + Clone> Drop for InFlightLeaderGuard<K> {
    fn drop(&mut self) {
        if !self.completed {
            self.finish_internal(Err("leader cancelled or dropped".to_string()));
        }
    }
}

/// Outcome of registering interest in a key inside [`InFlightMap`].
#[derive(Debug)]
pub enum InFlightResult<K: Eq + Hash + Clone> {
    /// Caller won leadership and is responsible for performing the operation.
    Leader(InFlightLeaderGuard<K>),
    /// Another task is already executing this key; await this receiver for its result.
    Follower(watch::Receiver<Option<Result<(), String>>>),
}

/// Awaits completion of a follower receiver obtained from [`InFlightResult::Follower`].
///
/// Never holds a `watch::Ref` borrow across `.await`.
pub async fn wait_for_inflight(
    rx: &mut watch::Receiver<Option<Result<(), String>>>,
) -> Result<(), String> {
    loop {
        {
            let borrowed = rx.borrow_and_update();
            if let Some(ref outcome) = *borrowed {
                return outcome.clone();
            }
        }
        if rx.changed().await.is_err() {
            let borrowed = rx.borrow();
            return match &*borrowed {
                Some(outcome) => outcome.clone(),
                None => Err("leader cancelled or dropped".to_string()),
            };
        }
    }
}

/// Sharded O(1) concurrent in-flight deduplication map backed by [`DashMap`].
///
/// Ensures that identical SHA1/URL artifacts requested simultaneously across
/// multiple workers or modpack entries are downloaded and verified once.
#[derive(Debug, Clone)]
pub struct InFlightMap<K: Eq + Hash + Clone> {
    map: Arc<DashMap<K, Arc<InFlightSlot>>>,
}

impl<K: Eq + Hash + Clone> Default for InFlightMap<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Clone> InFlightMap<K> {
    /// Creates an empty sharded [`InFlightMap`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: Arc::new(DashMap::new()),
        }
    }

    /// Creates an empty sharded [`InFlightMap`] with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: Arc::new(DashMap::with_capacity(capacity)),
        }
    }

    /// Registers `key` for deduplication.
    ///
    /// First caller receives [`InFlightResult::Leader`]; concurrent callers for
    /// the same `key` receive [`InFlightResult::Follower`]. The `DashMap` shard
    /// lock is released before `acquire` returns and is never held across `.await`.
    #[must_use]
    pub fn acquire(&self, key: K) -> InFlightResult<K> {
        match self.map.entry(key.clone()) {
            Entry::Occupied(occupied) => {
                let rx = occupied.get().subscribe();
                drop(occupied);
                InFlightResult::Follower(rx)
            }
            Entry::Vacant(vacant) => {
                let slot = Arc::new(InFlightSlot::new());
                vacant.insert(Arc::clone(&slot));
                InFlightResult::Leader(InFlightLeaderGuard {
                    key: Some(key),
                    slot,
                    map: Arc::clone(&self.map),
                    completed: false,
                })
            }
        }
    }

    /// Current number of keys with active leaders in flight.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if there are no active leaders in flight.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns `true` if `key` currently has an active leader.
    #[must_use]
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    #[test]
    fn atomic_file_state_valid_transitions_and_stale_epoch_cancellation() {
        let active_epoch = AtomicU64::new(1);
        let fsm = AtomicFileState::new(FileState::Pending);
        assert_eq!(fsm.load(), FileState::Pending);

        // Reject skipping Verifying straight to Done.
        assert_eq!(
            fsm.try_transition(FileState::Pending, FileState::Done, 1, &active_epoch),
            Err(StateTransitionError::InvalidTransition {
                from: FileState::Pending,
                to: FileState::Done,
            })
        );

        // Pending -> Downloading -> Verifying -> Done under valid epoch 1.
        assert_eq!(
            fsm.try_transition(FileState::Pending, FileState::Downloading, 1, &active_epoch),
            Ok(FileState::Downloading)
        );
        assert_eq!(
            fsm.try_transition(FileState::Pending, FileState::Verifying, 1, &active_epoch),
            Err(StateTransitionError::UnexpectedState {
                expected: FileState::Pending,
                actual: FileState::Downloading,
            })
        );
        assert_eq!(
            fsm.try_transition(
                FileState::Downloading,
                FileState::Verifying,
                1,
                &active_epoch
            ),
            Ok(FileState::Verifying)
        );

        // Advance epoch to 2 while worker is still on epoch 1 -> transitions to Cancelled!
        active_epoch.store(2, Ordering::Release);
        assert_eq!(
            fsm.try_transition(FileState::Verifying, FileState::Done, 1, &active_epoch),
            Err(StateTransitionError::StaleEpoch)
        );
        assert_eq!(fsm.load(), FileState::Cancelled);

        // Reset in epoch 2 and complete full lifecycle.
        assert_eq!(
            fsm.try_transition(FileState::Cancelled, FileState::Pending, 2, &active_epoch),
            Ok(FileState::Pending)
        );
        assert_eq!(
            fsm.try_transition(FileState::Pending, FileState::Downloading, 2, &active_epoch),
            Ok(FileState::Downloading)
        );
        assert_eq!(
            fsm.try_transition(
                FileState::Downloading,
                FileState::Verifying,
                2,
                &active_epoch
            ),
            Ok(FileState::Verifying)
        );
        assert_eq!(
            fsm.try_transition(FileState::Verifying, FileState::Done, 2, &active_epoch),
            Ok(FileState::Done)
        );
        assert!(fsm.load().is_terminal());
    }

    #[tokio::test]
    async fn inflight_map_deduplicates_concurrent_callers_to_single_leader() {
        let inflight: InFlightMap<String> = InFlightMap::new();
        let leader_executions = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(16));

        let mut handles = Vec::with_capacity(16);
        for _ in 0..16 {
            let map = inflight.clone();
            let execs = Arc::clone(&leader_executions);
            let bar = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                bar.wait().await;
                match map.acquire("sha1:deadbeef".to_string()) {
                    InFlightResult::Leader(guard) => {
                        execs.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(25)).await;
                        guard.complete_ok();
                        Ok(())
                    }
                    InFlightResult::Follower(mut rx) => wait_for_inflight(&mut rx).await,
                }
            }));
        }

        for h in handles {
            let res = h.await.expect("task should not panic");
            assert_eq!(res, Ok(()));
        }

        assert_eq!(leader_executions.load(Ordering::SeqCst), 1);
        assert!(inflight.is_empty());
    }

    #[tokio::test]
    async fn inflight_leader_drop_notifies_followers_and_cleans_map() {
        let inflight: InFlightMap<&'static str> = InFlightMap::new();

        let leader = match inflight.acquire("asset-abc") {
            InFlightResult::Leader(g) => g,
            InFlightResult::Follower(_) => panic!("first caller must be leader"),
        };
        assert_eq!(leader.key(), &"asset-abc");
        assert_eq!(inflight.len(), 1);

        let mut follower_rx = match inflight.acquire("asset-abc") {
            InFlightResult::Follower(rx) => rx,
            InFlightResult::Leader(_) => panic!("second caller must be follower"),
        };

        let follower_task = tokio::spawn(async move { wait_for_inflight(&mut follower_rx).await });

        // Simulate abrupt task cancellation / drop without calling complete_ok().
        drop(leader);

        let outcome = tokio::time::timeout(Duration::from_secs(1), follower_task)
            .await
            .expect("follower must not deadlock when leader drops")
            .expect("follower task must not panic");

        assert_eq!(outcome, Err("leader cancelled or dropped".to_string()));
        assert!(!inflight.contains_key(&"asset-abc"));
        assert!(inflight.is_empty());

        // Subsequent caller for the same key can cleanly acquire leadership again.
        match inflight.acquire("asset-abc") {
            InFlightResult::Leader(g) => g.complete_ok(),
            InFlightResult::Follower(_) => panic!("new caller after drop must become leader"),
        }
        assert!(inflight.is_empty());
    }
}
