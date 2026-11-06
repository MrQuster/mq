//! Bounded non-blocking `EventBus` (`flume`), progress snapshots, per-core CPU
//! telemetry payloads, and Minecraft game log classification.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::secret::redact_text;

/// Default bounded capacity of the [`EventBus`] ring channel.
pub const DEFAULT_EVENT_BUS_CAPACITY: usize = 4096;

/// Instantaneous utilization percentage for a single logical CPU core (`0.0..=100.0`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CoreCpuLoad {
    /// Zero-based logical core index (`cpu0`, `cpu1`, ...).
    pub core_id: usize,
    /// Busy utilization percentage (`0.0..=100.0`).
    pub usage_pct: f32,
}

/// Alias matching system prompt terminology.
pub type CoreLoad = CoreCpuLoad;

/// Aggregated system CPU and process memory telemetry snapshot sampled from `/proc/stat`
/// and `/proc/self/status`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CpuSnapshot {
    /// Overall busy CPU percentage across all cores (`0.0..=100.0`).
    pub total_pct: f32,
    /// Per-core busy percentages ordered by `core_id`.
    pub per_core: Vec<CoreCpuLoad>,
    /// Current resident set size (`VmRSS`) of the launcher process in MiB.
    pub rss_mib: f64,
}

/// Live throughput and completion snapshot for an active installation or verification phase.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PhaseProgress {
    /// Human-readable or canonical phase identifier.
    pub phase: String,
    /// Number of completed work items (files/blobs/entries).
    pub completed_items: u64,
    /// Total expected work items in this phase.
    pub total_items: u64,
    /// Number of bytes processed so far.
    pub bytes_done: u64,
    /// Total expected bytes in this phase (`0` if unknown).
    pub bytes_total: u64,
    /// Smoothed item throughput (`files/sec`).
    pub files_per_sec: f64,
    /// Smoothed byte throughput (`MiB/sec`).
    pub mib_per_sec: f64,
    /// Number of currently executing worker tasks.
    pub active_tasks: usize,
}

/// Alias matching architecture and system prompt terminology.
pub type ProgressSnapshot = PhaseProgress;

impl PhaseProgress {
    /// Returns item completion fraction clamped to `0.0..=1.0`.
    #[inline]
    pub fn item_fraction(&self) -> f32 {
        if self.total_items == 0 {
            0.0
        } else {
            (self.completed_items as f64 / self.total_items as f64).clamp(0.0, 1.0) as f32
        }
    }

    /// Returns byte completion fraction clamped to `0.0..=1.0`.
    #[inline]
    pub fn byte_fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            0.0
        } else {
            (self.bytes_done as f64 / self.bytes_total as f64).clamp(0.0, 1.0) as f32
        }
    }
}

/// Summary metrics recorded upon completion of an installation or benchmark phase.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseMetrics {
    /// Canonical phase name.
    pub phase: String,
    /// Wall-clock duration of the phase in milliseconds.
    pub elapsed_ms: u64,
    /// Number of items completed during the phase.
    pub completed_items: u64,
    /// Number of bytes processed during the phase.
    pub bytes_done: u64,
    /// Number of content-addressable storage cache hits.
    pub cas_hits: u64,
    /// Number of in-flight deduplication hits.
    pub dedup_hits: u64,
}

/// Severity level classified from a raw Minecraft `stdout`/`stderr` log line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GameLogLevel {
    #[default]
    Info,
    Warn,
    Error,
    Debug,
}

impl GameLogLevel {
    /// Returns the uppercase label (`"INFO"`, `"WARN"`, `"ERROR"`, `"DEBUG"`).
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Debug => "DEBUG",
        }
    }

    /// Classifies a raw Minecraft or JVM log line into a [`GameLogLevel`].
    ///
    /// Recognizes Log4j2 bracketed tags (`[Render thread/INFO]`, `[main/WARN]`,
    /// `[Server thread/ERROR]`, `[Worker-Main-1/DEBUG]`, `[main/FATAL]`), legacy
    /// `[WARNING]`/`[SEVERE]` tags, and unstructured JVM exception/stacktrace lines.
    pub fn classify(line: &str) -> Self {
        // 1. Check explicit bracketed thread/level markers first.
        if line.contains("/ERROR]")
            || line.contains("/FATAL]")
            || line.contains("/SEVERE]")
            || line.contains("[ERROR]")
            || line.contains("[FATAL]")
            || line.contains("[SEVERE]")
            || line.contains("[STDERR]")
        {
            return Self::Error;
        }
        if line.contains("/WARN]")
            || line.contains("/WARNING]")
            || line.contains("[WARN]")
            || line.contains("[WARNING]")
        {
            return Self::Warn;
        }
        if line.contains("/DEBUG]")
            || line.contains("/TRACE]")
            || line.contains("[DEBUG]")
            || line.contains("[TRACE]")
        {
            return Self::Debug;
        }

        // 2. Catch stacktraces and uncaught exceptions even if emitted on stdout.
        if line.contains("Exception")
            || line.contains("Caused by:")
            || line.contains("ERROR")
            || line.contains("FATAL")
            || line.contains("SEVERE")
        {
            return Self::Error;
        }

        if line.contains("/INFO]") || line.contains("[INFO]") {
            return Self::Info;
        }

        if line.contains("WARN") {
            return Self::Warn;
        }
        if line.contains("DEBUG") || line.contains("TRACE") {
            return Self::Debug;
        }

        Self::Info
    }

    /// Alias for [`GameLogLevel::classify`].
    #[inline]
    pub fn from_line(line: &str) -> Self {
        Self::classify(line)
    }
}

impl fmt::Display for GameLogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured, timestamped log entry emitted by a running Minecraft instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLogEntry {
    /// Instance slug or identifier that produced the line.
    pub instance: String,
    /// UTC epoch timestamp in milliseconds when the line was captured.
    pub timestamp_ms: i64,
    /// Classified log severity level.
    pub level: GameLogLevel,
    /// Scrubbed log text line.
    pub line: String,
}

impl GameLogEntry {
    /// Creates a [`GameLogEntry`] stamped with the current UTC time and auto-classified level.
    pub fn from_raw_line(instance: impl Into<String>, raw_line: impl Into<String>) -> Self {
        let line = raw_line.into();
        let level = GameLogLevel::classify(&line);
        Self {
            instance: instance.into(),
            timestamp_ms: Utc::now().timestamp_millis(),
            level,
            line,
        }
    }

    /// Creates a [`GameLogEntry`] after scrubbing any sensitive tokens via [`redact_text`].
    pub fn from_redacted_line(
        instance: impl Into<String>,
        raw_line: &str,
        secrets: &[&str],
    ) -> Self {
        let level = GameLogLevel::classify(raw_line);
        let line = redact_text(raw_line, secrets);
        Self {
            instance: instance.into(),
            timestamp_ms: Utc::now().timestamp_millis(),
            level,
            line,
        }
    }
}

/// Asynchronous telemetry, progress, lifecycle, and log events published across [`EventBus`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    PhaseStarted { phase: String, total_items: u64 },
    Progress(PhaseProgress),
    PhaseCompleted { phase: String, elapsed_ms: u64 },
    TaskError { context: String, message: String },
    Cancelled { epoch: u64 },
    CpuTelemetry(CpuSnapshot),
    GameLog(GameLogEntry),
    WatchdogWarning { operation: String, elapsed_ms: f64 },
}

/// Bounded, cloneable, lock-free MPMC event bus backed by `flume`.
///
/// Publishing is strictly non-blocking: if the bounded ring reaches capacity,
/// the oldest queued event is evicted so producer threads (`tokio` I/O tasks,
/// `rayon` CPU workers, or `/proc/stat` samplers) never stall.
#[derive(Clone)]
pub struct EventBus {
    tx: flume::Sender<Event>,
    rx: flume::Receiver<Event>,
    capacity: usize,
    dropped: Arc<AtomicU64>,
}

impl Default for EventBus {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Creates a bounded [`EventBus`] with [`DEFAULT_EVENT_BUS_CAPACITY`] (`4096`).
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_EVENT_BUS_CAPACITY)
    }

    /// Creates a bounded [`EventBus`] with the specified maximum capacity (clamped to `>= 1`).
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.max(1);
        let (tx, rx) = flume::bounded(cap);
        Self {
            tx,
            rx,
            capacity: cap,
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Publishes an event without ever blocking the caller.
    ///
    /// If the internal bounded queue is full, the oldest queued event is evicted
    /// to make room for the newer event.
    pub fn publish(&self, event: Event) {
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(flume::TrySendError::Full(event)) => {
                let mut evicted = 0u64;
                if self.rx.try_recv().is_ok() {
                    evicted += 1;
                }
                if let Err(flume::TrySendError::Full(_)) = self.tx.try_send(event) {
                    evicted += 1;
                }
                if evicted > 0 {
                    self.dropped.fetch_add(evicted, Ordering::Relaxed);
                }
            }
            Err(flume::TrySendError::Disconnected(_)) => {}
        }
    }

    /// Drains up to `max` pending events from the bus without blocking.
    pub fn try_drain(&self, max: usize) -> Vec<Event> {
        if max == 0 {
            return Vec::new();
        }
        let initial_cap = self.rx.len().min(max);
        let mut events = Vec::with_capacity(initial_cap);
        for _ in 0..max {
            match self.rx.try_recv() {
                Ok(ev) => events.push(ev),
                Err(_) => break,
            }
        }
        events
    }

    /// Drains all currently queued events without blocking.
    #[inline]
    pub fn drain_all(&self) -> Vec<Event> {
        self.try_drain(usize::MAX)
    }

    /// Attempts to pop a single queued event without blocking.
    #[inline]
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Borrows the underlying `flume::Sender<Event>`.
    #[inline]
    pub fn sender(&self) -> &flume::Sender<Event> {
        &self.tx
    }

    /// Borrows the underlying `flume::Receiver<Event>`.
    #[inline]
    pub fn receiver(&self) -> &flume::Receiver<Event> {
        &self.rx
    }

    /// Returns the number of events currently queued in the bus.
    #[inline]
    pub fn len(&self) -> usize {
        self.rx.len()
    }

    /// Returns `true` if no events are currently queued.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }

    /// Returns the bounded capacity of this bus.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the total number of events evicted due to backpressure overflow.
    #[inline]
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBus")
            .field("capacity", &self.capacity)
            .field("queued", &self.rx.len())
            .field("dropped", &self.dropped_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_log_level_classifier_handles_minecraft_and_jvm_lines() {
        assert_eq!(
            GameLogLevel::classify("[14:22:01] [Render thread/INFO]: Setting user: Player123"),
            GameLogLevel::Info
        );
        assert_eq!(
            GameLogLevel::classify(
                "[14:22:02] [Render thread/WARN]: Ambiguity between arguments [teleport, destination]"
            ),
            GameLogLevel::Warn
        );
        assert_eq!(
            GameLogLevel::classify(
                "[14:22:03] [Server thread/ERROR]: Encountered an unexpected exception"
            ),
            GameLogLevel::Error
        );
        assert_eq!(
            GameLogLevel::classify("[14:22:04] [Worker-Main-3/DEBUG]: Preparing spawn area: 0%"),
            GameLogLevel::Debug
        );
        assert_eq!(
            GameLogLevel::classify(
                "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main"
            ),
            GameLogLevel::Error
        );
        assert_eq!(
            GameLogLevel::classify("Caused by: java.io.FileNotFoundException: options.txt"),
            GameLogLevel::Error
        );
        assert_eq!(
            GameLogLevel::classify("WARN: Unknown configuration key"),
            GameLogLevel::Warn
        );
        assert_eq!(
            GameLogLevel::classify("ERROR: Failed to initialize OpenGL context"),
            GameLogLevel::Error
        );
    }

    #[test]
    fn event_bus_is_bounded_and_never_blocks_on_overflow() {
        let bus = EventBus::with_capacity(4);
        assert_eq!(bus.capacity(), 4);
        assert!(bus.is_empty());

        for epoch in 0..10 {
            bus.publish(Event::Cancelled { epoch });
        }

        assert_eq!(bus.len(), 4);
        assert_eq!(bus.dropped_count(), 6);

        let first_two = bus.try_drain(2);
        assert_eq!(
            first_two,
            vec![Event::Cancelled { epoch: 6 }, Event::Cancelled { epoch: 7 }]
        );

        let rest = bus.try_drain(10);
        assert_eq!(
            rest,
            vec![Event::Cancelled { epoch: 8 }, Event::Cancelled { epoch: 9 }]
        );
        assert!(bus.is_empty());
    }

    #[test]
    fn game_log_entry_redacts_secrets_and_preserves_level() {
        let entry = GameLogEntry::from_redacted_line(
            "fabric-1.21",
            "[12:00:00] [Render thread/WARN]: Token super_secret_token_999 rejected",
            &["super_secret_token_999"],
        );
        assert_eq!(entry.instance, "fabric-1.21");
        assert_eq!(entry.level, GameLogLevel::Warn);
        assert_eq!(
            entry.line,
            "[12:00:00] [Render thread/WARN]: Token [REDACTED] rejected"
        );
        assert!(!entry.line.contains("super_secret_token_999"));
    }
}
