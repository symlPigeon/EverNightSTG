use std::time::{Duration, Instant};

// ── Frame profile types ───────────────────────────────────────────────────────

/// Per-phase timing breakdown for a single `World::step_profiled()` call.
#[derive(Debug, Default, Clone)]
pub struct WorldFrameProfile {
    pub pre_update: Duration,
    pub spawn_commit: Duration,
    pub post_spawn_commit: Duration,
    pub pre_movement: Duration,
    pub movement: Duration,
    pub post_movement: Duration,
    pub pre_collision: Duration,
    pub collision: Duration,
    pub post_collision: Duration,
    pub pre_lifetime: Duration,
    pub lifetime: Duration,
    pub post_lifetime: Duration,
    pub post_update: Duration,
}

/// Per-phase timing breakdown for a single `App::step_profiled()` call.
///
/// Adds [`script_on_frame`](AppFrameProfile::script_on_frame) between
/// `post_collision` and `pre_lifetime` to capture the scripting engine cost.
#[derive(Debug, Default, Clone)]
pub struct AppFrameProfile {
    pub pre_update: Duration,
    pub spawn_commit: Duration,
    pub post_spawn_commit: Duration,
    pub pre_movement: Duration,
    pub movement: Duration,
    pub post_movement: Duration,
    pub pre_collision: Duration,
    pub collision: Duration,
    pub post_collision: Duration,
    pub script_on_frame: Duration,
    pub pre_lifetime: Duration,
    pub lifetime: Duration,
    pub post_lifetime: Duration,
    pub post_update: Duration,
}

/// Trait that allows [`ProfileAccumulator`] and display functions to work
/// generically over any frame profile type.
pub trait FrameProfile {
    /// Returns all phases as `(name, duration)` pairs in execution order.
    fn phases(&self) -> Vec<(&'static str, Duration)>;

    /// Sum of all phase durations.
    fn total(&self) -> Duration {
        self.phases().iter().map(|(_, d)| *d).sum()
    }
}

impl FrameProfile for WorldFrameProfile {
    fn phases(&self) -> Vec<(&'static str, Duration)> {
        vec![
            ("pre_update", self.pre_update),
            ("spawn_commit", self.spawn_commit),
            ("post_spawn_commit", self.post_spawn_commit),
            ("pre_movement", self.pre_movement),
            ("movement", self.movement),
            ("post_movement", self.post_movement),
            ("pre_collision", self.pre_collision),
            ("collision", self.collision),
            ("post_collision", self.post_collision),
            ("pre_lifetime", self.pre_lifetime),
            ("lifetime", self.lifetime),
            ("post_lifetime", self.post_lifetime),
            ("post_update", self.post_update),
        ]
    }
}

impl FrameProfile for AppFrameProfile {
    fn phases(&self) -> Vec<(&'static str, Duration)> {
        vec![
            ("pre_update", self.pre_update),
            ("spawn_commit", self.spawn_commit),
            ("post_spawn_commit", self.post_spawn_commit),
            ("pre_movement", self.pre_movement),
            ("movement", self.movement),
            ("post_movement", self.post_movement),
            ("pre_collision", self.pre_collision),
            ("collision", self.collision),
            ("post_collision", self.post_collision),
            ("script_on_frame", self.script_on_frame),
            ("pre_lifetime", self.pre_lifetime),
            ("lifetime", self.lifetime),
            ("post_lifetime", self.post_lifetime),
            ("post_update", self.post_update),
        ]
    }
}

// ── Statistics ────────────────────────────────────────────────────────────────

/// Descriptive statistics for a set of duration samples.
#[derive(Debug, Clone)]
pub struct Stats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
}

impl Stats {
    /// Computes stats from a **sorted** slice.
    ///
    /// # Panics
    /// Panics if `sorted` is empty.
    pub fn from_sorted(sorted: &[Duration]) -> Self {
        let n = sorted.len();
        assert!(n > 0, "Stats::from_sorted: empty slice");
        let sum: Duration = sorted.iter().sum();
        // percentile index: saturate so index 0 maps to first element
        let idx = |pct: usize| sorted[(n * pct).saturating_sub(1) / 100];
        Self {
            min: sorted[0],
            max: sorted[n - 1],
            mean: sum / n as u32,
            p50: idx(50),
            p95: idx(95),
            p99: idx(99),
        }
    }
}

// ── Profile accumulator ───────────────────────────────────────────────────────

/// Collects per-frame [`FrameProfile`] samples and computes per-phase
/// [`Stats`] across all collected frames.
pub struct ProfileAccumulator<P> {
    frames: Vec<P>,
}

impl<P: FrameProfile> ProfileAccumulator<P> {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Record one frame's profile.
    pub fn push(&mut self, profile: P) {
        self.frames.push(profile);
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Computes per-phase [`Stats`] across all collected frames.
    ///
    /// Returns `(phase_name, stats)` pairs in execution order.
    /// Performs a single O(N·P) pass over frames, then sorts each column.
    pub fn phase_stats(&self) -> Vec<(&'static str, Stats)> {
        if self.frames.is_empty() {
            return vec![];
        }

        let first_phases = self.frames[0].phases();
        let phase_count = first_phases.len();

        // Pre-allocate one duration vec per phase.
        let mut columns: Vec<Vec<Duration>> = (0..phase_count)
            .map(|_| Vec::with_capacity(self.frames.len()))
            .collect();

        for frame in &self.frames {
            for (i, (_, d)) in frame.phases().iter().enumerate() {
                columns[i].push(*d);
            }
        }

        first_phases
            .into_iter()
            .zip(columns.iter_mut())
            .map(|((name, _), col)| {
                col.sort();
                (name, Stats::from_sorted(col))
            })
            .collect()
    }

    /// [`Stats`] computed over the per-frame total durations.
    pub fn total_stats(&self) -> Stats {
        let mut totals: Vec<Duration> = self.frames.iter().map(|f| f.total()).collect();
        totals.sort();
        Stats::from_sorted(&totals)
    }
}

impl<P: FrameProfile> Default for ProfileAccumulator<P> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Bench harness ─────────────────────────────────────────────────────────────

/// Result of a single timed benchmark run.
pub struct BenchResult {
    pub name: &'static str,
    #[allow(dead_code)]
    pub warmup: u32,
    pub iterations: u32,
    pub total: Duration,
}

impl BenchResult {
    pub fn us_per_iter(&self) -> f64 {
        self.total.as_nanos() as f64 / self.iterations as f64 / 1_000.0
    }

    pub fn iters_per_sec(&self) -> f64 {
        1_000_000_000.0 / (self.total.as_nanos() as f64 / self.iterations as f64)
    }
}

/// Runs `f` for `warmup` iterations (discarded), then times `iterations` more.
pub fn bench<F: FnMut()>(
    name: &'static str,
    warmup: u32,
    iterations: u32,
    mut f: F,
) -> BenchResult {
    for _ in 0..warmup {
        f();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    BenchResult {
        name,
        warmup,
        iterations,
        total: start.elapsed(),
    }
}

// ── Display ───────────────────────────────────────────────────────────────────

/// Prints a standard throughput table for a slice of [`BenchResult`]s.
pub fn print_results(results: &[BenchResult]) {
    println!(
        "{:<48} {:>8} {:>14} {:>12}",
        "benchmark", "iters", "us/iter", "iter/s"
    );
    println!("{}", "-".repeat(86));
    for r in results {
        println!(
            "{:<48} {:>8} {:>14.2} {:>12.0}",
            r.name,
            r.iterations,
            r.us_per_iter(),
            r.iters_per_sec(),
        );
    }
    println!();
}

fn dur_us(d: Duration) -> f64 {
    d.as_nanos() as f64 / 1_000.0
}

/// Prints a per-phase statistical breakdown from a [`ProfileAccumulator`].
///
/// Columns: phase, mean μs, p50, p95, p99, min, max, % of total mean.
pub fn print_profile_report<P: FrameProfile>(name: &str, acc: &ProfileAccumulator<P>) {
    if acc.is_empty() {
        println!("[{name}] no frames collected");
        return;
    }

    let phase_stats = acc.phase_stats();
    let total = acc.total_stats();
    let total_mean_us = dur_us(total.mean);

    println!("=== Profile: {name} ({} frames) ===", acc.len());
    println!(
        "{:<24} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>6}",
        "phase", "mean μs", "p50 μs", "p95 μs", "p99 μs", "min μs", "max μs", "%"
    );
    println!("{}", "-".repeat(93));

    for (phase, stats) in &phase_stats {
        let mean_us = dur_us(stats.mean);
        let pct = if total_mean_us > 0.0 {
            mean_us / total_mean_us * 100.0
        } else {
            0.0
        };
        println!(
            "{:<24} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>5.1}%",
            phase,
            mean_us,
            dur_us(stats.p50),
            dur_us(stats.p95),
            dur_us(stats.p99),
            dur_us(stats.min),
            dur_us(stats.max),
            pct,
        );
    }

    println!("{}", "-".repeat(93));
    println!(
        "{:<24} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>5.1}%",
        "TOTAL",
        total_mean_us,
        dur_us(total.p50),
        dur_us(total.p95),
        dur_us(total.p99),
        dur_us(total.min),
        dur_us(total.max),
        100.0,
    );
    println!();
}
