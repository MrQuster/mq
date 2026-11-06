//! `mq-sched`: Runtime bridge (`tokio` <-> `rayon`), DAG scheduler, cancellation epochs,
//! reusable buffer pool, lock-free file state machine, `/proc/stat` per-core CPU telemetry,
//! Amdahl scaling analysis, and UI frame Watchdog.

#![deny(clippy::await_holding_lock)]

pub mod amdahl;
pub mod bridge;
pub mod buffer_pool;
pub mod dag;
pub mod epoch;
pub mod state_machine;
pub mod telemetry;
pub mod watchdog;

pub use amdahl::{
    amdahl_predicted_speedup, compute_scaling_rows, compute_speedup, estimate_serial_fraction,
    format_scaling_markdown, theoretical_max_speedup, thread_ladder, BenchPhaseRow, ScalingRow,
    ScalingTable,
};
pub use bridge::{ComputeError, ComputePool};
pub use buffer_pool::{
    BufferPool, BufferPoolStats, PooledBuffer, DEFAULT_CHUNK_SIZE, DEFAULT_MAX_POOLED_CHUNKS,
    MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
pub use dag::{
    BoxedNodeFuture, DagError, DagExecutor, DagNodeSpec, DagPlan, DagTaskFn, ExecutableDag,
    ExecutableNode,
};
pub use epoch::{CancelEpoch, EpochCancelledError, EpochGuard};
pub use state_machine::{
    wait_for_inflight, AtomicFileState, FileState, FileStateMachine, InFlightLeaderGuard,
    InFlightMap, InFlightResult, InFlightSlot, StateTransitionError,
};
pub use telemetry::{
    count_open_fds, parse_proc_stat, read_current_rss_mib, read_peak_rss_mib, read_proc_stat,
    CoreSample, CpuSampler, CpuTelemetrySnapshot, PhaseCpuReport, ProcStatRaw, ScopedCpuProfiler,
};
pub use watchdog::{UiWatchdog, WatchdogAlert, WatchdogGuard, WatchdogScope, DEFAULT_UI_BUDGET};
