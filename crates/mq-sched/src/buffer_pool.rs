#![deny(clippy::await_holding_lock)]
//! Bounded reusable byte-chunk buffer pool for streaming I/O and hashing.
//!
//! Prevents unbounded heap growth under high download concurrency (Lesson E4)
//! by capping both the number of idle recycled buffers in a lock-free bounded
//! channel and the number of concurrently leased async buffers via a semaphore.

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Minimum recommended chunk size (64 KiB).
pub const MIN_CHUNK_SIZE: usize = 64 * 1024;

/// Default chunk size (128 KiB).
pub const DEFAULT_CHUNK_SIZE: usize = 128 * 1024;

/// Maximum recommended chunk size (256 KiB).
pub const MAX_CHUNK_SIZE: usize = 256 * 1024;

/// Default maximum number of idle chunks retained in the pool (128 * 128 KiB = 16 MiB).
pub const DEFAULT_MAX_POOLED_CHUNKS: usize = 128;

/// Snapshot of [`BufferPool`] allocation and reuse counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPoolStats {
    /// Byte length of each chunk managed by the pool.
    pub chunk_size: usize,
    /// Maximum idle chunks retained in the bounded recycle channel.
    pub max_pooled_chunks: usize,
    /// Maximum concurrent async leases allowed by the backpressure semaphore.
    pub max_leased_chunks: usize,
    /// Current number of idle chunks ready for immediate reuse.
    pub pooled_available: usize,
    /// Current number of checked-out [`PooledBuffer`] instances.
    pub active_leases: usize,
    /// Cumulative number of fresh heap allocations performed.
    pub allocated_total: u64,
    /// Cumulative number of checkouts satisfied from recycled buffers.
    pub reused_total: u64,
    /// Cumulative number of buffers freed on drop because the idle pool was full.
    pub dropped_overflow_total: u64,
}

struct BufferPoolInner {
    chunk_size: usize,
    max_pooled_chunks: usize,
    max_leased_chunks: usize,
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
    semaphore: Arc<Semaphore>,
    active_leases: AtomicUsize,
    allocated_total: AtomicU64,
    reused_total: AtomicU64,
    dropped_overflow_total: AtomicU64,
}

impl fmt::Debug for BufferPoolInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferPoolInner")
            .field("chunk_size", &self.chunk_size)
            .field("max_pooled_chunks", &self.max_pooled_chunks)
            .field("max_leased_chunks", &self.max_leased_chunks)
            .field("pooled_available", &self.rx.len())
            .field("active_leases", &self.active_leases.load(Ordering::Relaxed))
            .finish()
    }
}

/// Thread-safe bounded pool of fixed-size reusable byte buffers.
///
/// Cloning a `BufferPool` is cheap (`Arc` handle) and shares the same underlying
/// bounded recycle queue and async concurrency semaphore.
#[derive(Debug, Clone)]
pub struct BufferPool {
    inner: Arc<BufferPoolInner>,
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferPool {
    /// Creates a pool with [`DEFAULT_CHUNK_SIZE`] (128 KiB) and
    /// [`DEFAULT_MAX_POOLED_CHUNKS`] (128 chunks = 16 MiB cap).
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(
            DEFAULT_CHUNK_SIZE,
            DEFAULT_MAX_POOLED_CHUNKS,
            DEFAULT_MAX_POOLED_CHUNKS,
        )
    }

    /// Creates a pool with custom `chunk_size` and `max_pooled_chunks`.
    ///
    /// Sets the async semaphore lease limit equal to `max_pooled_chunks`.
    /// Values of `0` are clamped to `1`.
    #[must_use]
    pub fn with_capacity(chunk_size: usize, max_pooled_chunks: usize) -> Self {
        Self::with_limits(chunk_size, max_pooled_chunks, max_pooled_chunks)
    }

    /// Creates a pool with explicit chunk size, idle pool bound, and async lease bound.
    ///
    /// Values of `0` are clamped to `1`.
    #[must_use]
    pub fn with_limits(
        chunk_size: usize,
        max_pooled_chunks: usize,
        max_leased_chunks: usize,
    ) -> Self {
        let chunk_size = chunk_size.max(1);
        let max_pooled_chunks = max_pooled_chunks.max(1);
        let max_leased_chunks = max_leased_chunks.max(1);
        let (tx, rx) = bounded(max_pooled_chunks);
        Self {
            inner: Arc::new(BufferPoolInner {
                chunk_size,
                max_pooled_chunks,
                max_leased_chunks,
                tx,
                rx,
                semaphore: Arc::new(Semaphore::new(max_leased_chunks)),
                active_leases: AtomicUsize::new(0),
                allocated_total: AtomicU64::new(0),
                reused_total: AtomicU64::new(0),
                dropped_overflow_total: AtomicU64::new(0),
            }),
        }
    }

    /// Pre-allocates up to `count` idle buffers into the bounded recycle channel.
    pub fn prewarm(&self, count: usize) {
        let target = count.min(self.inner.max_pooled_chunks);
        for _ in 0..target {
            if self.inner.tx.is_full() {
                break;
            }
            let buf = vec![0u8; self.inner.chunk_size];
            if self.inner.tx.try_send(buf).is_ok() {
                self.inner.allocated_total.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
    }

    /// Asynchronously acquires a [`PooledBuffer`], applying backpressure via the
    /// bounded [`Semaphore`] when `max_leased_chunks` are already checked out.
    pub async fn acquire(&self) -> PooledBuffer {
        let permit = Arc::clone(&self.inner.semaphore)
            .acquire_owned()
            .await
            .expect("BufferPool semaphore is never closed");
        self.lease_with_permit(Some(permit))
    }

    /// Attempts to acquire a [`PooledBuffer`] without waiting if an async lease
    /// permit is immediately available.
    #[must_use]
    pub fn try_acquire(&self) -> Option<PooledBuffer> {
        let permit = Arc::clone(&self.inner.semaphore).try_acquire_owned().ok()?;
        Some(self.lease_with_permit(Some(permit)))
    }

    /// Synchronously checks out a [`PooledBuffer`] for CPU / `rayon` workers.
    ///
    /// Never blocks or invokes async primitives. Tries to claim an available
    /// semaphore permit opportunistically; if all async permits are currently
    /// held, allocates a temporary chunk that is cleanly freed on drop if the
    /// bounded recycle channel is full.
    #[must_use]
    pub fn checkout_sync(&self) -> PooledBuffer {
        let permit = Arc::clone(&self.inner.semaphore).try_acquire_owned().ok();
        self.lease_with_permit(permit)
    }

    fn lease_with_permit(&self, permit: Option<OwnedSemaphorePermit>) -> PooledBuffer {
        let mut buf = match self.inner.rx.try_recv() {
            Ok(recycled) => {
                self.inner.reused_total.fetch_add(1, Ordering::Relaxed);
                recycled
            }
            Err(_) => {
                self.inner.allocated_total.fetch_add(1, Ordering::Relaxed);
                vec![0u8; self.inner.chunk_size]
            }
        };
        if buf.len() != self.inner.chunk_size {
            buf.resize(self.inner.chunk_size, 0);
        }
        self.inner.active_leases.fetch_add(1, Ordering::Relaxed);
        PooledBuffer {
            buf: Some(buf),
            pool: Arc::clone(&self.inner),
            _permit: permit,
        }
    }

    /// Fixed size in bytes of each buffer chunk.
    #[must_use]
    pub fn chunk_size(&self) -> usize {
        self.inner.chunk_size
    }

    /// Maximum number of idle buffers retained in the bounded pool.
    #[must_use]
    pub fn max_pooled_chunks(&self) -> usize {
        self.inner.max_pooled_chunks
    }

    /// Maximum number of concurrent async leases permitted before backpressure.
    #[must_use]
    pub fn max_leased_chunks(&self) -> usize {
        self.inner.max_leased_chunks
    }

    /// Maximum idle resident memory retained by the pool in bytes.
    #[must_use]
    pub fn max_resident_bytes(&self) -> usize {
        self.inner.chunk_size * self.inner.max_pooled_chunks
    }

    /// Current number of recycled buffers sitting idle in the bounded channel.
    #[must_use]
    pub fn available_pooled(&self) -> usize {
        self.inner.rx.len()
    }

    /// Current number of checked-out [`PooledBuffer`] handles.
    #[must_use]
    pub fn active_leases(&self) -> usize {
        self.inner.active_leases.load(Ordering::Relaxed)
    }

    /// Returns a snapshot of pool counters for telemetry and testing.
    #[must_use]
    pub fn stats(&self) -> BufferPoolStats {
        BufferPoolStats {
            chunk_size: self.inner.chunk_size,
            max_pooled_chunks: self.inner.max_pooled_chunks,
            max_leased_chunks: self.inner.max_leased_chunks,
            pooled_available: self.inner.rx.len(),
            active_leases: self.inner.active_leases.load(Ordering::Relaxed),
            allocated_total: self.inner.allocated_total.load(Ordering::Relaxed),
            reused_total: self.inner.reused_total.load(Ordering::Relaxed),
            dropped_overflow_total: self.inner.dropped_overflow_total.load(Ordering::Relaxed),
        }
    }
}

/// RAII lease over a fixed-size byte buffer from a [`BufferPool`].
///
/// Automatically returns its backing `Vec<u8>` to the bounded recycle channel on
/// [`Drop`]. If the recycle channel is already at capacity, the `Vec<u8>` is
/// dropped immediately so resident memory stays strictly bounded.
pub struct PooledBuffer {
    buf: Option<Vec<u8>>,
    pool: Arc<BufferPoolInner>,
    _permit: Option<OwnedSemaphorePermit>,
}

impl PooledBuffer {
    /// Returns the fixed length of the chunk in bytes (`chunk_size`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Returns `true` only if the chunk has zero length.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Borrows the underlying chunk as an immutable byte slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.buf
            .as_deref()
            .expect("PooledBuffer backing Vec exists until Drop")
    }

    /// Borrows the underlying chunk as a mutable byte slice.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.buf
            .as_deref_mut()
            .expect("PooledBuffer backing Vec exists until Drop")
    }
}

impl AsRef<[u8]> for PooledBuffer {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for PooledBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl Deref for PooledBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl fmt::Debug for PooledBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PooledBuffer")
            .field("len", &self.len())
            .field("has_permit", &self._permit.is_some())
            .finish()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        self.pool.active_leases.fetch_sub(1, Ordering::Relaxed);
        if let Some(mut buf) = self.buf.take() {
            if buf.len() != self.pool.chunk_size {
                buf.resize(self.pool.chunk_size, 0);
            }
            if self.pool.tx.try_send(buf).is_err() {
                self.pool
                    .dropped_overflow_total
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        // `_permit` drops immediately after this method, waking any awaiting `acquire()`.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn default_pool_has_16_mib_cap() {
        let pool = BufferPool::new();
        assert_eq!(pool.chunk_size(), 128 * 1024);
        assert_eq!(pool.max_pooled_chunks(), 128);
        assert_eq!(pool.max_leased_chunks(), 128);
        assert_eq!(pool.max_resident_bytes(), 16 * 1024 * 1024);
    }

    #[test]
    fn sync_checkout_reuses_buffers_and_caps_idle_channel() {
        let pool = BufferPool::with_limits(MIN_CHUNK_SIZE, 4, 4);

        {
            let mut b1 = pool.checkout_sync();
            assert_eq!(b1.len(), MIN_CHUNK_SIZE);
            assert!(!b1.is_empty());
            b1[0] = 0xAB;
            b1[MIN_CHUNK_SIZE - 1] = 0xCD;
            assert_eq!(b1.as_ref()[0], 0xAB);
            assert_eq!(pool.active_leases(), 1);
        }

        assert_eq!(pool.available_pooled(), 1);
        assert_eq!(pool.active_leases(), 0);

        // Second checkout reuses the exact recycled allocation.
        {
            let mut b2 = pool.checkout_sync();
            b2.as_mut()[0] = 0x11;
            assert_eq!(b2[0], 0x11);
        }

        let stats = pool.stats();
        assert_eq!(stats.allocated_total, 1);
        assert_eq!(stats.reused_total, 1);
        assert_eq!(stats.pooled_available, 1);

        // Checkout 10 buffers simultaneously via sync API; only 4 may remain pooled on drop.
        let mut batch = Vec::with_capacity(10);
        for _ in 0..10 {
            batch.push(pool.checkout_sync());
        }
        assert_eq!(pool.active_leases(), 10);
        drop(batch);

        let stats_after = pool.stats();
        assert_eq!(stats_after.active_leases, 0);
        assert_eq!(stats_after.pooled_available, 4);
        assert_eq!(stats_after.dropped_overflow_total, 6);
    }

    #[tokio::test]
    async fn async_acquire_enforces_backpressure_and_recycles() {
        let pool = BufferPool::with_limits(64 * 1024, 2, 2);

        let b1 = pool.acquire().await;
        let b2 = pool.acquire().await;
        assert!(pool.try_acquire().is_none());
        assert_eq!(pool.active_leases(), 2);

        let pool_waiter = pool.clone();
        let waiter = tokio::spawn(async move {
            let mut b3 = pool_waiter.acquire().await;
            b3[42] = 7;
            b3.len()
        });

        // While both leases are held, the 3rd acquire must wait on the semaphore.
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(!waiter.is_finished());

        // Dropping b1 returns its Vec to the channel and releases one permit.
        drop(b1);

        let len = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("waiter should complete promptly")
            .expect("waiter task should not panic");
        assert_eq!(len, 64 * 1024);

        drop(b2);
        let stats = pool.stats();
        assert_eq!(stats.allocated_total, 2);
        assert_eq!(stats.reused_total, 1);
        assert_eq!(stats.pooled_available, 2);
        assert_eq!(stats.dropped_overflow_total, 0);
    }

    #[test]
    fn prewarm_populates_up_to_bound() {
        let pool = BufferPool::with_capacity(MAX_CHUNK_SIZE, 3);
        pool.prewarm(10);
        assert_eq!(pool.available_pooled(), 3);
        assert_eq!(pool.stats().allocated_total, 3);
    }
}
