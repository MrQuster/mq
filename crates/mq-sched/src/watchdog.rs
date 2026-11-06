//! UI frame budget Watchdog (>8ms warning + atomic counter + non-blocking alert channel).

#![deny(clippy::await_holding_lock)]

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Default UI thread frame budget (8ms, half of a 60Hz 16.6ms frame).
pub const DEFAULT_UI_BUDGET: Duration = Duration::from_millis(8);

/// Alert emitted when a guarded UI operation exceeds the configured watchdog budget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WatchdogAlert {
    pub operation: &'static str,
    pub elapsed_ms: f64,
}

/// Non-blocking UI operation watchdog tracking frame-budget violations.
pub struct UiWatchdog {
    budget: Duration,
    violations: AtomicU64,
    alert_tx: Option<flume::Sender<WatchdogAlert>>,
}

impl fmt::Debug for UiWatchdog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiWatchdog")
            .field("budget", &self.budget)
            .field("violations", &self.violation_count())
            .field("has_alert_channel", &self.alert_tx.is_some())
            .finish()
    }
}

impl Default for UiWatchdog {
    fn default() -> Self {
        Self::new()
    }
}

impl UiWatchdog {
    /// Creates a `UiWatchdog` with the default 8ms budget and no alert channel.
    pub fn new() -> Self {
        Self::with_budget(DEFAULT_UI_BUDGET)
    }

    /// Creates a `UiWatchdog` with a custom frame budget and no alert channel.
    pub fn with_budget(budget: Duration) -> Self {
        Self {
            budget,
            violations: AtomicU64::new(0),
            alert_tx: None,
        }
    }

    /// Creates a `UiWatchdog` with a custom budget and a bounded `flume::Sender<WatchdogAlert>`.
    pub fn with_alerts(budget: Duration, alert_tx: flume::Sender<WatchdogAlert>) -> Self {
        Self {
            budget,
            violations: AtomicU64::new(0),
            alert_tx: Some(alert_tx),
        }
    }

    /// Creates a `UiWatchdog` paired with a bounded `flume::Receiver<WatchdogAlert>`.
    pub fn bounded_channel(
        budget: Duration,
        capacity: usize,
    ) -> (Self, flume::Receiver<WatchdogAlert>) {
        let (tx, rx) = flume::bounded(capacity);
        (Self::with_alerts(budget, tx), rx)
    }

    /// Returns the configured frame budget.
    #[inline]
    pub fn budget(&self) -> Duration {
        self.budget
    }

    /// Begins timing `operation`, checking elapsed time against `self.budget` on drop.
    #[inline]
    pub fn guard(&self, operation: &'static str) -> WatchdogGuard<'_> {
        WatchdogGuard {
            watchdog: self,
            operation,
            started_at: Instant::now(),
        }
    }

    /// Executes closure `f` under a `WatchdogGuard` for `operation`.
    #[inline]
    pub fn scope<T>(&self, operation: &'static str, f: impl FnOnce() -> T) -> T {
        let _guard = self.guard(operation);
        f()
    }

    /// Returns the cumulative number of budget violations recorded since creation or last reset.
    #[inline]
    pub fn violation_count(&self) -> u64 {
        self.violations.load(Ordering::Relaxed)
    }

    /// Resets the cumulative violation counter to zero.
    #[inline]
    pub fn reset(&self) {
        self.violations.store(0, Ordering::Relaxed);
    }
}

/// RAII scope guard created by [`UiWatchdog::guard`].
#[derive(Debug)]
pub struct WatchdogGuard<'a> {
    watchdog: &'a UiWatchdog,
    operation: &'static str,
    started_at: Instant,
}

/// Alias for [`WatchdogGuard`].
pub type WatchdogScope<'a> = WatchdogGuard<'a>;

impl Drop for WatchdogGuard<'_> {
    fn drop(&mut self) {
        let elapsed = self.started_at.elapsed();
        if elapsed > self.watchdog.budget {
            self.watchdog.violations.fetch_add(1, Ordering::Relaxed);
            let operation = self.operation;
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            tracing::warn!(
                operation,
                elapsed_ms,
                budget_ms = self.watchdog.budget.as_secs_f64() * 1000.0,
                "UI thread operation exceeded frame watchdog budget"
            );
            if let Some(tx) = &self.watchdog.alert_tx {
                let _ = tx.try_send(WatchdogAlert {
                    operation,
                    elapsed_ms,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn fast_ui_operation_passes_without_violation() {
        let (watchdog, rx) = UiWatchdog::bounded_channel(DEFAULT_UI_BUDGET, 8);
        assert_eq!(watchdog.budget(), Duration::from_millis(8));
        {
            let _guard = watchdog.guard("render_virtual_rows");
        }
        assert_eq!(watchdog.violation_count(), 0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn slow_ui_operation_exceeding_8ms_records_violation_and_alert() {
        let (watchdog, rx) = UiWatchdog::bounded_channel(DEFAULT_UI_BUDGET, 4);
        watchdog.scope("sync_tree_rebuild", || {
            thread::sleep(Duration::from_millis(14));
        });

        assert_eq!(watchdog.violation_count(), 1);
        let alert = rx.try_recv().expect("expected WatchdogAlert");
        assert_eq!(alert.operation, "sync_tree_rebuild");
        assert!(
            alert.elapsed_ms > 8.0,
            "expected >8ms, got {}",
            alert.elapsed_ms
        );

        watchdog.reset();
        assert_eq!(watchdog.violation_count(), 0);
    }

    #[test]
    fn full_alert_channel_never_blocks_ui_thread() {
        let (watchdog, rx) = UiWatchdog::bounded_channel(Duration::from_millis(2), 1);
        for _ in 0..3 {
            let _g = watchdog.guard("heavy_paint");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(watchdog.violation_count(), 3);
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }
}
