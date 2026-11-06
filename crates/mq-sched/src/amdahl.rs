//! Amdahl's law & Karp-Flatt empirical serial-fraction scaling calculator and Markdown formatter.

#![deny(clippy::await_holding_lock)]

use crate::telemetry::PhaseCpuReport;
use std::fmt::Write as _;

/// Scaling measurement row for a benchmark phase at a given thread count.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchPhaseRow {
    pub threads: usize,
    pub elapsed_ms: f64,
    pub speedup: f64,
    pub efficiency_pct: f64,
    pub serial_fraction: f64,
    pub mean_cpu_pct: f32,
    pub per_core_pct: Vec<f32>,
}

/// Alias for [`BenchPhaseRow`].
pub type ScalingRow = BenchPhaseRow;

/// Complete multi-thread scaling table for a named benchmark phase.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalingTable {
    pub phase_name: String,
    pub rows: Vec<BenchPhaseRow>,
}

impl ScalingTable {
    /// Builds a `ScalingTable` from `(threads, elapsed_ms, PhaseCpuReport)` measurements,
    /// using the `threads == 1` entry (or the first entry if absent) as the `T(1)` baseline.
    pub fn from_measurements(
        phase_name: impl Into<String>,
        measurements: &[(usize, f64, PhaseCpuReport)],
    ) -> Self {
        Self {
            phase_name: phase_name.into(),
            rows: compute_scaling_rows(measurements),
        }
    }

    /// Formats the scaling table as a GitHub-flavored Markdown report.
    pub fn to_markdown(&self) -> String {
        format_scaling_markdown(&self.phase_name, &self.rows)
    }

    /// Returns the theoretical maximum speedup at infinite cores derived from the highest
    /// thread-count row (`1.0 / serial_fraction.max(0.0001)`).
    pub fn max_theoretical_speedup(&self) -> f64 {
        self.rows
            .iter()
            .rev()
            .find(|r| r.threads > 1)
            .map(|r| theoretical_max_speedup(r.serial_fraction))
            .unwrap_or(1.0)
    }
}

/// Computes speedup `S(N) = T(1) / T(N)`.
#[inline]
pub fn compute_speedup(t1_ms: f64, tn_ms: f64) -> f64 {
    if tn_ms <= 0.0 || t1_ms <= 0.0 {
        0.0
    } else {
        t1_ms / tn_ms
    }
}

/// Computes the Karp-Flatt / Amdahl empirical serial fraction:
/// `f = (1/S(N) - 1/N) / (1 - 1/N)`, clamped to `0.0..=1.0`
/// (`0.0` for `threads <= 1` or superlinear speedup `S(N) >= N`).
#[inline]
pub fn estimate_serial_fraction(speedup: f64, threads: usize) -> f64 {
    if threads <= 1 {
        return 0.0;
    }
    let n = threads as f64;
    if speedup >= n {
        return 0.0;
    }
    if speedup <= 0.0 {
        return 1.0;
    }
    let inv_n = 1.0 / n;
    (((1.0 / speedup) - inv_n) / (1.0 - inv_n)).clamp(0.0, 1.0)
}

/// Computes Amdahl's theoretical maximum speedup at infinite cores:
/// `1.0 / serial_fraction.max(0.0001)`.
#[inline]
pub fn theoretical_max_speedup(serial_fraction: f64) -> f64 {
    1.0 / serial_fraction.clamp(0.0, 1.0).max(0.0001)
}

/// Predicts speedup at `threads` given serial fraction `f` via Amdahl's law:
/// `S(N) = 1 / (f + (1 - f) / N)`.
#[inline]
pub fn amdahl_predicted_speedup(serial_fraction: f64, threads: usize) -> f64 {
    let n = threads.max(1) as f64;
    let f = serial_fraction.clamp(0.0, 1.0);
    1.0 / (f + (1.0 - f) / n)
}

/// Converts raw `(threads, elapsed_ms, PhaseCpuReport)` measurements into [`BenchPhaseRow`]s.
pub fn compute_scaling_rows(measurements: &[(usize, f64, PhaseCpuReport)]) -> Vec<BenchPhaseRow> {
    if measurements.is_empty() {
        return Vec::new();
    }

    let baseline_ms = measurements
        .iter()
        .find(|(threads, _, _)| *threads == 1)
        .map(|(_, ms, _)| *ms)
        .unwrap_or(measurements[0].1);

    measurements
        .iter()
        .map(|(threads, elapsed_ms, cpu)| {
            let n = (*threads).max(1);
            let speedup = if n == 1 && (*elapsed_ms - baseline_ms).abs() <= f64::EPSILON {
                1.0
            } else {
                compute_speedup(baseline_ms, *elapsed_ms)
            };
            let efficiency_pct = (speedup / n as f64) * 100.0;
            let serial_fraction = estimate_serial_fraction(speedup, n);

            BenchPhaseRow {
                threads: n,
                elapsed_ms: *elapsed_ms,
                speedup,
                efficiency_pct,
                serial_fraction,
                mean_cpu_pct: cpu.mean_total_pct,
                per_core_pct: cpu.per_core_mean_pct.clone(),
            }
        })
        .collect()
}

/// Generates a power-of-two thread count ladder `[1, 2, 4, 8, ..., max_threads]`,
/// always starting at `1` and ending at `max_threads.max(1)` without duplicates.
pub fn thread_ladder(max_threads: usize) -> Vec<usize> {
    let limit = max_threads.max(1);
    let mut ladder = Vec::new();
    let mut cur = 1usize;
    while cur < limit {
        ladder.push(cur);
        match cur.checked_mul(2) {
            Some(next) => cur = next,
            None => break,
        }
    }
    ladder.push(limit);
    ladder
}

/// Formats benchmark scaling rows into a clean Markdown table with columns:
/// `Threads | Time (ms) | Speedup | Efficiency | Serial Fraction (Amdahl) | Avg CPU % | Per-Core CPU %`.
pub fn format_scaling_markdown(phase_name: &str, rows: &[BenchPhaseRow]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "### {phase_name}\n");
    let _ = writeln!(
        out,
        "| Threads | Time (ms) | Speedup | Efficiency | Serial Fraction (Amdahl) | Avg CPU % | Per-Core CPU % |"
    );
    let _ = writeln!(out, "| ---: | ---: | ---: | ---: | ---: | ---: | :--- |");

    for row in rows {
        let serial_col = if row.threads <= 1 {
            "baseline".to_string()
        } else {
            let max_s = theoretical_max_speedup(row.serial_fraction);
            format!("{:.4} (max {:.2}x)", row.serial_fraction, max_s)
        };

        let per_core_col = format_per_core(&row.per_core_pct);
        let _ = writeln!(
            out,
            "| {} | {:.2} | {:.2}x | {:.1}% | {} | {:.1}% | {} |",
            row.threads,
            row.elapsed_ms,
            row.speedup,
            row.efficiency_pct,
            serial_col,
            row.mean_cpu_pct,
            per_core_col,
        );
    }

    out
}

fn format_per_core(per_core_pct: &[f32]) -> String {
    let mut s = String::from("[");
    for (i, &pct) in per_core_pct.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(s, "{pct:.1}%");
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_ladder_powers_of_two_and_non_power_cap() {
        assert_eq!(thread_ladder(0), vec![1]);
        assert_eq!(thread_ladder(1), vec![1]);
        assert_eq!(thread_ladder(2), vec![1, 2]);
        assert_eq!(thread_ladder(4), vec![1, 2, 4]);
        assert_eq!(thread_ladder(6), vec![1, 2, 4, 6]);
        assert_eq!(thread_ladder(8), vec![1, 2, 4, 8]);
        assert_eq!(thread_ladder(12), vec![1, 2, 4, 8, 12]);
    }

    #[test]
    fn amdahl_serial_fraction_and_theoretical_ceiling_math() {
        // Suppose f = 0.20 (20% serial). At N = 4:
        // S(4) = 1 / (0.2 + 0.8/4) = 2.5 => T(4) = 40ms for T(1) = 100ms.
        let s4 = compute_speedup(100.0, 40.0);
        assert!((s4 - 2.5).abs() < 1e-9);

        let f = estimate_serial_fraction(s4, 4);
        assert!((f - 0.2).abs() < 1e-9);
        assert!((theoretical_max_speedup(f) - 5.0).abs() < 1e-9);
        assert!((amdahl_predicted_speedup(f, 4) - 2.5).abs() < 1e-9);

        // Superlinear speedup clamps serial fraction to 0.0 and ceiling to 10_000x.
        let f_super = estimate_serial_fraction(4.5, 4);
        assert_eq!(f_super, 0.0);
        assert!((theoretical_max_speedup(f_super) - 10_000.0).abs() < 1e-9);

        // Severe contention (S(4) < 1.0) clamps serial fraction to 1.0.
        let f_slow = estimate_serial_fraction(0.8, 4);
        assert_eq!(f_slow, 1.0);
        assert!((theoretical_max_speedup(f_slow) - 1.0).abs() < 1e-9);

        // Baseline N = 1 returns 0.0 without dividing by zero.
        assert_eq!(estimate_serial_fraction(1.0, 1), 0.0);
    }

    #[test]
    fn scaling_table_and_markdown_generation() {
        let mk_report = |elapsed_ms: f64, total: f32, cores: Vec<f32>| PhaseCpuReport {
            elapsed_ms,
            mean_total_pct: total,
            per_core_mean_pct: cores.clone(),
            per_core_peak_pct: cores,
            peak_rss_mib: 32.0,
        };

        let measurements = vec![
            (1, 100.0, mk_report(100.0, 25.0, vec![99.5, 0.2, 0.1, 0.2])),
            (2, 55.0, mk_report(55.0, 48.0, vec![96.0, 95.8, 0.1, 0.1])),
            (4, 40.0, mk_report(40.0, 82.5, vec![83.0, 82.5, 82.0, 82.5])),
        ];

        let table = ScalingTable::from_measurements("Stage 2: Parse & Hash", &measurements);
        assert_eq!(table.rows.len(), 3);
        assert!((table.rows[2].speedup - 2.5).abs() < 1e-9);
        assert!((table.rows[2].efficiency_pct - 62.5).abs() < 1e-9);
        assert!((table.rows[2].serial_fraction - 0.2).abs() < 1e-9);
        assert!((table.max_theoretical_speedup() - 5.0).abs() < 1e-9);

        let md = table.to_markdown();
        assert!(md.contains("### Stage 2: Parse & Hash"));
        assert!(md.contains(
            "| Threads | Time (ms) | Speedup | Efficiency | Serial Fraction (Amdahl) | Avg CPU % | Per-Core CPU % |"
        ));
        assert!(md.contains("0.2000 (max 5.00x)"));
        assert!(md.contains("[83.0%, 82.5%, 82.0%, 82.5%]"));
    }
}
