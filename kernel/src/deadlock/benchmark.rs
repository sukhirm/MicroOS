//! Benchmark Harness and Research Data Preservation.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! ### Separation of Data
//! 1. `ORIGINAL_RESEARCH_BENCHMARKS`: Fixed, immutable baseline measurements from the student's
//!    original research report (`CA3_R14` and `deadlock_investigation_v2`).
//! 2. `MicroOsBenchmarkMeasurement`: Live bare-metal CPU cycle measurements captured inside
//!    the Micro-OS kernel using the x86 `rdtsc` instruction.

extern crate alloc;

use alloc::vec::Vec;
use super::avoidance::BankerResult;
use super::detection::DetectionResult;
use super::workload::{generate_banker_workload, generate_detection_workload, DEFAULT_RESOURCE_TYPES, WORKLOAD_SIZES};

/// Represents an immutable row from the original research paper benchmarks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OriginalBenchmarkEntry {
    /// Process count evaluated ($n$).
    pub processes: usize,
    /// Resource type count ($m$).
    pub resources: usize,
    /// Median execution time in microseconds for Banker's Algorithm.
    pub banker_median_us: f32,
    /// Median execution time in microseconds for Deadlock Detection.
    pub detection_median_us: f32,
    /// Data source citation.
    pub source: &'static str,
}

/// Baseline measurements preserved directly from `CA3_R14/r14_results.csv`.
pub const ORIGINAL_CA3_BENCHMARKS: &[OriginalBenchmarkEntry] = &[
    OriginalBenchmarkEntry { processes: 5,   resources: 3, banker_median_us: 6.2,   detection_median_us: 8.5,   source: "CA-3 Report (Python/Host)" },
    OriginalBenchmarkEntry { processes: 10,  resources: 3, banker_median_us: 12.0,  detection_median_us: 16.1,  source: "CA-3 Report (Python/Host)" },
    OriginalBenchmarkEntry { processes: 20,  resources: 3, banker_median_us: 27.6,  detection_median_us: 31.9,  source: "CA-3 Report (Python/Host)" },
    OriginalBenchmarkEntry { processes: 40,  resources: 3, banker_median_us: 44.3,  detection_median_us: 60.7,  source: "CA-3 Report (Python/Host)" },
    OriginalBenchmarkEntry { processes: 80,  resources: 3, banker_median_us: 88.9,  detection_median_us: 122.5, source: "CA-3 Report (Python/Host)" },
    OriginalBenchmarkEntry { processes: 120, resources: 3, banker_median_us: 189.6, detection_median_us: 180.7, source: "CA-3 Report (Python/Host)" },
];

/// Baseline measurements preserved directly from `deadlock_results_v2/results.csv` (3 resources).
pub const ORIGINAL_V2_BENCHMARKS: &[OriginalBenchmarkEntry] = &[
    OriginalBenchmarkEntry { processes: 5,  resources: 3, banker_median_us: 1.7,  detection_median_us: 4.3,   source: "Investigation v2 (3 resources)" },
    OriginalBenchmarkEntry { processes: 10, resources: 3, banker_median_us: 8.8,  detection_median_us: 14.0,  source: "Investigation v2 (3 resources)" },
    OriginalBenchmarkEntry { processes: 20, resources: 3, banker_median_us: 17.7, detection_median_us: 24.75, source: "Investigation v2 (3 resources)" },
    OriginalBenchmarkEntry { processes: 40, resources: 3, banker_median_us: 31.6, detection_median_us: 49.3,  source: "Investigation v2 (3 resources)" },
    OriginalBenchmarkEntry { processes: 80, resources: 3, banker_median_us: 54.5, detection_median_us: 90.95, source: "Investigation v2 (3 resources)" },
];

/// Live bare-metal cycle measurement captured in Micro-OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicroOsBenchmarkMeasurement {
    /// Process count evaluated ($n \in \{5, 10, 20, 50, 100\}$).
    pub processes: usize,
    /// Resource type count ($m = 3$).
    pub resources: usize,
    /// Median CPU clock cycles elapsed during Banker's Safety Check.
    pub banker_median_cycles: u64,
    /// Median CPU clock cycles elapsed during Deadlock Detection.
    pub detection_median_cycles: u64,
    /// Number of element comparisons performed by Banker's Algorithm.
    pub banker_comparisons: usize,
    /// Number of element comparisons performed by Deadlock Detection.
    pub detection_comparisons: usize,
    /// Whether the Banker state was evaluated as safe.
    pub banker_safe: bool,
    /// Number of processes detected as deadlocked in the detection trial.
    pub deadlocked_count: usize,
}

/// Executes bare-metal in-kernel benchmark suite across required workloads (5, 10, 20, 50, 100 processes).
///
/// Uses x86 `rdtsc` to measure exact instruction clock cycles over multiple repetitions,
/// selecting the median sample to reject outliers caused by timer interrupts or cache misses.
pub fn run_microos_benchmarks() -> Vec<MicroOsBenchmarkMeasurement> {
    const REPETITIONS: usize = 5;
    let mut results = Vec::with_capacity(WORKLOAD_SIZES.len());

    for &n in &WORKLOAD_SIZES {
        let banker_workload = match generate_banker_workload(n, DEFAULT_RESOURCE_TYPES) {
            Ok(w) => w,
            Err(_) => continue,
        };
        let detection_workload = match generate_detection_workload(n, DEFAULT_RESOURCE_TYPES, false) {
            Ok(w) => w,
            Err(_) => continue,
        };

        // 1. Measure Banker's Algorithm
        let mut banker_cycles_samples = [0u64; REPETITIONS];
        let mut last_banker_res = BankerResult {
            is_safe: false,
            safe_sequence: Vec::new(),
            resource_comparisons: 0,
            iteration_passes: 0,
        };

        // Warmup pass
        core::hint::black_box(banker_workload.is_safe());

        for rep in 0..REPETITIONS {
            let start = unsafe { core::arch::x86_64::_rdtsc() };
            let res = core::hint::black_box(banker_workload.is_safe());
            let elapsed = unsafe { core::arch::x86_64::_rdtsc() } - start;
            banker_cycles_samples[rep] = elapsed;
            last_banker_res = res;
        }
        banker_cycles_samples.sort_unstable();
        let banker_median_cycles = banker_cycles_samples[REPETITIONS / 2];

        // 2. Measure Deadlock Detection
        let mut detection_cycles_samples = [0u64; REPETITIONS];
        let mut last_detection_res = DetectionResult {
            deadlocked_processes: Vec::new(),
            is_deadlocked: false,
            resource_comparisons: 0,
            reduction_passes: 0,
        };

        // Warmup pass
        core::hint::black_box(detection_workload.detect_deadlock());

        for rep in 0..REPETITIONS {
            let start = unsafe { core::arch::x86_64::_rdtsc() };
            let res = core::hint::black_box(detection_workload.detect_deadlock());
            let elapsed = unsafe { core::arch::x86_64::_rdtsc() } - start;
            detection_cycles_samples[rep] = elapsed;
            last_detection_res = res;
        }
        detection_cycles_samples.sort_unstable();
        let detection_median_cycles = detection_cycles_samples[REPETITIONS / 2];

        results.push(MicroOsBenchmarkMeasurement {
            processes: n,
            resources: DEFAULT_RESOURCE_TYPES,
            banker_median_cycles,
            detection_median_cycles,
            banker_comparisons: last_banker_res.resource_comparisons,
            detection_comparisons: last_detection_res.resource_comparisons,
            banker_safe: last_banker_res.is_safe,
            deadlocked_count: last_detection_res.deadlocked_processes.len(),
        });
    }

    results
}
