//! Linux `/proc/stat` per-core CPU telemetry, `/proc/self/status` RSS readers,
//! `/proc/self/fd` descriptor counting, and scoped phase CPU profiling.

#![deny(clippy::await_holding_lock)]

use std::fs;
use std::io;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const KIB_PER_MIB: f64 = 1024.0;

/// Raw jiffies sample for either the aggregate `cpu` line (`core_id == None`)
/// or a specific logical core `cpuN` (`core_id == Some(N)`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreSample {
    pub core_id: Option<usize>,
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CoreSample {
    /// Active CPU jiffies: `user + nice + system + irq + softirq + steal`.
    /// Note: Linux already accounts `guest` inside `user` and `guest_nice` inside `nice`.
    #[inline]
    pub fn busy(&self) -> u64 {
        self.user
            .saturating_add(self.nice)
            .saturating_add(self.system)
            .saturating_add(self.irq)
            .saturating_add(self.softirq)
            .saturating_add(self.steal)
    }

    /// Total CPU jiffies: `busy + idle + iowait`.
    #[inline]
    pub fn total(&self) -> u64 {
        self.busy()
            .saturating_add(self.idle)
            .saturating_add(self.iowait)
    }

    /// Computes exact kernel-accounted utilization percentage (`0.0..=100.0`)
    /// over the interval from `prev` to `self`.
    #[inline]
    pub fn utilization_since(&self, prev: &Self) -> f32 {
        let delta_busy = self.busy().saturating_sub(prev.busy());
        let delta_total = self.total().saturating_sub(prev.total());
        if delta_total == 0 {
            return 0.0;
        }
        ((delta_busy as f64 / delta_total as f64) * 100.0).clamp(0.0, 100.0) as f32
    }
}

/// Parsed snapshot of `/proc/stat` containing both aggregate `cpu` and per-core `cpu0..cpuN` counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcStatRaw {
    pub total: CoreSample,
    pub cores: Vec<CoreSample>,
}

impl ProcStatRaw {
    /// Computes `(total_pct, per_core_pct)` delta utilization from `prev` to `self`,
    /// with every value clamped to `0.0..=100.0`.
    pub fn utilization_delta(&self, prev: &Self) -> (f32, Vec<f32>) {
        let total_pct = self.total.utilization_since(&prev.total);
        let per_core_pct = self
            .cores
            .iter()
            .enumerate()
            .map(|(idx, curr_core)| {
                let prev_core = prev
                    .cores
                    .get(idx)
                    .filter(|c| c.core_id == curr_core.core_id)
                    .or_else(|| prev.cores.iter().find(|c| c.core_id == curr_core.core_id));
                match prev_core {
                    Some(p) => curr_core.utilization_since(p),
                    None => 0.0,
                }
            })
            .collect();
        (total_pct, per_core_pct)
    }
}

/// Parses Linux `/proc/stat` contents into aggregate `cpu` and sorted `cpu0..cpuN` jiffies counters.
pub fn parse_proc_stat(contents: &str) -> ProcStatRaw {
    let mut total = CoreSample::default();
    let mut cores = Vec::new();

    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let Some(label) = parts.next() else {
            continue;
        };

        if label == "cpu" {
            total = parse_cpu_fields(None, &mut parts);
        } else if let Some(suffix) = label.strip_prefix("cpu") {
            if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(core_id) = suffix.parse::<usize>() {
                    cores.push(parse_cpu_fields(Some(core_id), &mut parts));
                }
            }
        }
    }

    cores.sort_by_key(|c| c.core_id);
    ProcStatRaw { total, cores }
}

fn parse_cpu_fields<'a>(
    core_id: Option<usize>,
    parts: &mut impl Iterator<Item = &'a str>,
) -> CoreSample {
    let mut next_u64 = || {
        parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    };
    CoreSample {
        core_id,
        user: next_u64(),
        nice: next_u64(),
        system: next_u64(),
        idle: next_u64(),
        iowait: next_u64(),
        irq: next_u64(),
        softirq: next_u64(),
        steal: next_u64(),
    }
}

/// Reads and parses `/proc/stat` from the local Linux procfs.
pub fn read_proc_stat() -> io::Result<ProcStatRaw> {
    let contents = fs::read_to_string("/proc/stat")?;
    Ok(parse_proc_stat(&contents))
}

/// Parses a `kB` memory counter (such as `VmRSS` or `VmHWM`) from `/proc/self/status` text.
pub fn parse_status_kb(contents: &str, key: &str) -> Option<u64> {
    let target = key.trim_end_matches(':');
    for line in contents.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == target {
                return v
                    .split_whitespace()
                    .next()
                    .and_then(|tok| tok.parse::<u64>().ok());
            }
        }
    }
    None
}

fn read_rss_pair_mib() -> (f64, f64) {
    let Ok(contents) = fs::read_to_string("/proc/self/status") else {
        return (0.0, 0.0);
    };
    let rss_mib = parse_status_kb(&contents, "VmRSS")
        .map(|kb| kb as f64 / KIB_PER_MIB)
        .unwrap_or(0.0);
    let peak_mib = parse_status_kb(&contents, "VmHWM")
        .map(|kb| kb as f64 / KIB_PER_MIB)
        .unwrap_or(0.0);
    (rss_mib, peak_mib)
}

/// Reads current resident set size (`VmRSS`) in MiB from `/proc/self/status`.
pub fn read_current_rss_mib() -> f64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| parse_status_kb(&s, "VmRSS"))
        .map(|kb| kb as f64 / KIB_PER_MIB)
        .unwrap_or(0.0)
}

/// Reads peak high-water mark resident set size (`VmHWM`) in MiB from `/proc/self/status`.
pub fn read_peak_rss_mib() -> f64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| parse_status_kb(&s, "VmHWM"))
        .map(|kb| kb as f64 / KIB_PER_MIB)
        .unwrap_or(0.0)
}

/// Counts currently open file descriptors for this process via `/proc/self/fd`.
pub fn count_open_fds() -> usize {
    fs::read_dir("/proc/self/fd")
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// Point-in-time delta CPU utilization and process memory telemetry snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuTelemetrySnapshot {
    pub total_pct: f32,
    pub per_core_pct: Vec<f32>,
    pub rss_mib: f64,
    pub peak_rss_mib: f64,
}

/// Stateful `/proc/stat` delta calculator reporting total and per-core CPU utilization.
#[derive(Debug, Clone)]
pub struct CpuSampler {
    prev: ProcStatRaw,
}

impl CpuSampler {
    /// Initializes a `CpuSampler` by capturing an initial `/proc/stat` baseline.
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            prev: read_proc_stat()?,
        })
    }

    /// Creates a `CpuSampler` from a synthetic or pre-parsed baseline.
    pub fn from_raw(baseline: ProcStatRaw) -> Self {
        Self { prev: baseline }
    }

    /// Reads `/proc/stat` and `/proc/self/status`, returning utilization over the delta since
    /// the previous sample and advancing the internal baseline.
    pub fn sample(&mut self) -> io::Result<CpuTelemetrySnapshot> {
        let current = read_proc_stat()?;
        let (total_pct, per_core_pct) = current.utilization_delta(&self.prev);
        self.prev = current;
        let (rss_mib, peak_rss_mib) = read_rss_pair_mib();
        Ok(CpuTelemetrySnapshot {
            total_pct,
            per_core_pct,
            rss_mib,
            peak_rss_mib: peak_rss_mib.max(rss_mib),
        })
    }

    /// Advances the sampler using an explicit `ProcStatRaw` snapshot (useful for deterministic tests).
    pub fn sample_raw(&mut self, current: ProcStatRaw) -> (f32, Vec<f32>) {
        let delta = current.utilization_delta(&self.prev);
        self.prev = current;
        delta
    }
}

/// Aggregated CPU and memory report produced at the end of a `ScopedCpuProfiler` phase.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseCpuReport {
    pub elapsed_ms: f64,
    pub mean_total_pct: f32,
    pub per_core_mean_pct: Vec<f32>,
    pub per_core_peak_pct: Vec<f32>,
    pub peak_rss_mib: f64,
}

struct BgSampleStats {
    per_core_peak_pct: Vec<f32>,
    peak_rss_mib: f64,
}

/// Lightweight RAII/scoped CPU profiler that polls `/proc/stat` every 25ms on a background
/// thread to capture peak per-core utilization and peak RSS, while computing exact start-to-end
/// kernel jiffies deltas for mean utilization across the phase window.
pub struct ScopedCpuProfiler {
    start_instant: Instant,
    start_stat: ProcStatRaw,
    stop_tx: Option<Sender<()>>,
    worker: Option<JoinHandle<BgSampleStats>>,
}

impl ScopedCpuProfiler {
    /// Starts profiling with the default 25ms background polling interval.
    pub fn start() -> io::Result<Self> {
        Self::start_with_interval(DEFAULT_POLL_INTERVAL)
    }

    /// Starts profiling with a custom background polling interval.
    pub fn start_with_interval(poll_interval: Duration) -> io::Result<Self> {
        let start_stat = read_proc_stat()?;
        let start_instant = Instant::now();
        let initial_cores = start_stat.cores.len();
        let (init_rss, init_hwm) = read_rss_pair_mib();
        let initial_peak_rss = init_hwm.max(init_rss);

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let prev_for_thread = start_stat.clone();

        let worker = thread::Builder::new()
            .name("mq-cpu-profiler".into())
            .spawn(move || {
                let mut prev = prev_for_thread;
                let mut per_core_peak_pct = vec![0.0_f32; initial_cores];
                let mut peak_rss_mib = initial_peak_rss;

                loop {
                    match stop_rx.recv_timeout(poll_interval) {
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => {
                            if let Ok(curr) = read_proc_stat() {
                                let (_, core_pcts) = curr.utilization_delta(&prev);
                                if per_core_peak_pct.len() < core_pcts.len() {
                                    per_core_peak_pct.resize(core_pcts.len(), 0.0);
                                }
                                for (peak, &pct) in
                                    per_core_peak_pct.iter_mut().zip(core_pcts.iter())
                                {
                                    *peak = peak.max(pct);
                                }
                                prev = curr;
                            }
                            let (rss, hwm) = read_rss_pair_mib();
                            peak_rss_mib = peak_rss_mib.max(rss).max(hwm);
                        }
                    }
                }

                BgSampleStats {
                    per_core_peak_pct,
                    peak_rss_mib,
                }
            })?;

        Ok(Self {
            start_instant,
            start_stat,
            stop_tx: Some(stop_tx),
            worker: Some(worker),
        })
    }

    /// Stops the background sampler and computes the exact `PhaseCpuReport` over the phase window.
    pub fn finish(mut self) -> PhaseCpuReport {
        self.stop_and_collect()
    }

    /// Alias for [`ScopedCpuProfiler::finish`].
    pub fn stop(self) -> PhaseCpuReport {
        self.finish()
    }

    fn stop_and_collect(&mut self) -> PhaseCpuReport {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        let bg = self
            .worker
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_else(|| BgSampleStats {
                per_core_peak_pct: vec![0.0; self.start_stat.cores.len()],
                peak_rss_mib: 0.0,
            });

        let elapsed_ms = self.start_instant.elapsed().as_secs_f64() * 1000.0;
        let end_stat = read_proc_stat().unwrap_or_else(|_| self.start_stat.clone());
        let (mean_total_pct, per_core_mean_pct) = end_stat.utilization_delta(&self.start_stat);

        let mut per_core_peak_pct = bg.per_core_peak_pct;
        if per_core_peak_pct.len() < per_core_mean_pct.len() {
            per_core_peak_pct.resize(per_core_mean_pct.len(), 0.0);
        }
        for (peak, &mean) in per_core_peak_pct.iter_mut().zip(per_core_mean_pct.iter()) {
            *peak = peak.max(mean).clamp(0.0, 100.0);
        }

        let (end_rss, end_hwm) = read_rss_pair_mib();
        let peak_rss_mib = bg.peak_rss_mib.max(end_rss).max(end_hwm);

        PhaseCpuReport {
            elapsed_ms,
            mean_total_pct,
            per_core_mean_pct,
            per_core_peak_pct,
            peak_rss_mib,
        }
    }
}

impl Drop for ScopedCpuProfiler {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::fs::File;

    const SAMPLE_STAT_T0: &str = "\
cpu  1000 100 300 8000 500 50 50 0 20 10
cpu0 500 50 150 4000 250 25 25 0 10 5
cpu1 500 50 150 4000 250 25 25 0 10 5
ctxt 123456
btime 1700000000
processes 4200
";

    const SAMPLE_STAT_T1: &str = "\
cpu  1120 110 340 8050 530 60 60 10 25 10
cpu1 520 50 160 4050 270 25 25 0 10 5
cpu0 600 60 180 4000 260 35 35 10 15 5
ctxt 124000
";

    #[test]
    fn synthetic_proc_stat_parsing_and_delta_math() {
        let s0 = parse_proc_stat(SAMPLE_STAT_T0);
        let s1 = parse_proc_stat(SAMPLE_STAT_T1);

        assert_eq!(s0.cores.len(), 2);
        assert_eq!(s1.cores.len(), 2);
        // Verify cores are sorted even when cpu1 appears before cpu0 in input.
        assert_eq!(s1.cores[0].core_id, Some(0));
        assert_eq!(s1.cores[1].core_id, Some(1));

        // t0 aggregate: busy = 1000+100+300+50+50+0 = 1500, idle+iowait = 8500, total = 10000
        assert_eq!(s0.total.busy(), 1500);
        assert_eq!(s0.total.total(), 10_000);

        // t1 cpu0:
        // t0 busy = 500+50+150+25+25+0 = 750, total = 750 + 4000 + 250 = 5000
        // t1 busy = 600+60+180+35+35+10 = 920, total = 920 + 4000 + 260 = 5180
        // delta_busy = 170, delta_total = 180 => 94.44444%
        let mut sampler = CpuSampler::from_raw(s0.clone());
        let (total_pct, per_core) = sampler.sample_raw(s1.clone());

        let expected_cpu0 = (170.0_f64 / 180.0_f64 * 100.0) as f32;
        // t1 cpu1:
        // t0 busy = 750, total = 5000
        // t1 busy = 520+50+160+25+25+0 = 780, total = 780 + 4050 + 270 = 5100
        // delta_busy = 30, delta_total = 100 => 30.0%
        let expected_cpu1 = 30.0_f32;
        // aggregate: delta_busy = 200, delta_total = 280 => 71.42857%
        let expected_total = (200.0_f64 / 280.0_f64 * 100.0) as f32;

        assert!((total_pct - expected_total).abs() < 1e-4);
        assert!((per_core[0] - expected_cpu0).abs() < 1e-4);
        assert!((per_core[1] - expected_cpu1).abs() < 1e-4);

        // Zero-delta sample must return 0.0 (never NaN).
        let (zero_total, zero_cores) = sampler.sample_raw(s1);
        assert_eq!(zero_total, 0.0);
        assert_eq!(zero_cores, vec![0.0, 0.0]);
    }

    #[test]
    fn parse_proc_status_kb_extracts_rss_and_hwm() {
        let status = "\
Name:\tmq-sched
State:\tR (running)
VmPeak:\t   65536 kB
VmHWM:\t   32768 kB
VmRSS:\t   16384 kB
Threads:\t4
";
        assert_eq!(parse_status_kb(status, "VmRSS"), Some(16384));
        assert_eq!(parse_status_kb(status, "VmHWM:"), Some(32768));
        assert_eq!(parse_status_kb(status, "NonExistent"), None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn live_linux_proc_stat_rss_and_fd_count() {
        let mut sampler = CpuSampler::new().expect("read /proc/stat baseline");
        let snap = sampler.sample().expect("sample /proc/stat");

        assert!(!snap.per_core_pct.is_empty());
        assert!((0.0..=100.0).contains(&snap.total_pct));
        for &pct in &snap.per_core_pct {
            assert!((0.0..=100.0).contains(&pct));
        }

        let rss = read_current_rss_mib();
        let hwm = read_peak_rss_mib();
        assert!(rss > 0.0, "expected positive VmRSS MiB, got {rss}");
        assert!(hwm >= rss * 0.5, "expected positive VmHWM MiB, got {hwm}");

        let fds_before = count_open_fds();
        assert!(
            fds_before >= 3,
            "expected at least stdin/stdout/stderr open"
        );
        {
            let _extra = File::open("/proc/stat").expect("open /proc/stat");
            let fds_during = count_open_fds();
            assert_eq!(fds_during, fds_before + 1);
        }
        let fds_after = count_open_fds();
        assert_eq!(fds_after, fds_before);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn scoped_cpu_profiler_tracks_multithreaded_busy_loop() {
        let profiler = ScopedCpuProfiler::start().expect("start profiler");

        let workers: Vec<_> = (0..2)
            .map(|_| {
                thread::spawn(|| {
                    let deadline = Instant::now() + Duration::from_millis(90);
                    let mut acc = 1u64;
                    while Instant::now() < deadline {
                        for i in 1..10_000u64 {
                            acc = std::hint::black_box(
                                acc.wrapping_mul(6364136223846793005).wrapping_add(i),
                            );
                        }
                    }
                    acc
                })
            })
            .collect();

        for w in workers {
            let _ = w.join().unwrap();
        }

        let report = profiler.finish();
        assert!(report.elapsed_ms >= 80.0);
        assert!(
            report.mean_total_pct > 0.0,
            "expected non-zero CPU utilization, got {:?}",
            report
        );
        assert!((0.0..=100.0).contains(&report.mean_total_pct));
        assert!(!report.per_core_mean_pct.is_empty());
        assert_eq!(
            report.per_core_mean_pct.len(),
            report.per_core_peak_pct.len()
        );
        assert!(report.per_core_mean_pct.iter().any(|&p| p > 0.0));
        for (&mean, &peak) in report
            .per_core_mean_pct
            .iter()
            .zip(report.per_core_peak_pct.iter())
        {
            assert!((0.0..=100.0).contains(&mean));
            assert!((0.0..=100.0).contains(&peak));
            assert!(peak >= mean);
        }
        assert!(report.peak_rss_mib > 0.0);
    }
}
