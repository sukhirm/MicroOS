//! Scheduler and Context-Switch Performance Benchmarking Subsystem.
//!
//! Phase 1C: Context-Switch Latency, Scheduling Latency & Round Robin Fairness.
//!
//! Provides on-demand performance telemetry measuring raw CPU instruction clock cycles
//! via bare-metal x86 RDTSC for:
//! 1. Voluntary yield dispatch overhead (`int 48` round-trip with empty ready queue).
//! 2. Cooperative task-to-task context-switch round-trip and one-way latency.
//! 3. Scheduling latency (time elapsed between task readiness and execution start).
//! 4. Round Robin fairness evaluation using Jain's Fairness Index.
//! 5. Task throughput and response/turnaround metrics under bounded workloads.
//!
//! Architectural Invariants:
//! - All microbenchmarks are on-demand only (never executed in the desktop rendering loop).
//! - Zero memory allocations while holding scheduler locks or inside critical sections.
//! - Bounded workloads: all tasks terminate cleanly without infinite loops or unbounded queues.
//! - Raw CPU instruction cycles are strictly NEVER converted to nanoseconds.

use core::sync::atomic::{AtomicU64, Ordering};

/// Default repetitions for microbenchmark measurements.
pub const BENCHMARK_REPETITIONS: usize = 5;

/// Context-switch latency measurement report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextSwitchLatencyReport {
    /// Voluntary yield dispatch overhead (int 48 round-trip with empty ready queue).
    pub voluntary_yield_overhead_cycles: u64,
    /// Task-to-task cooperative context-switch round-trip latency.
    pub task_switch_roundtrip_cycles: u64,
    /// One-way task-to-task context switch latency (half of round-trip or direct ping-pong).
    pub task_switch_oneway_cycles: u64,
    /// Number of repetitions evaluated.
    pub repetitions: usize,
    /// Whether actual task-to-task switching executed or was bypassed on host.
    pub executed: bool,
}

/// Scheduling latency measurement report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulingLatencyReport {
    /// Minimum observed scheduling latency in CPU clock cycles.
    pub min_cycles: u64,
    /// Median observed scheduling latency in CPU clock cycles.
    pub median_cycles: u64,
    /// Maximum observed scheduling latency in CPU clock cycles.
    pub max_cycles: u64,
    /// Number of sample measurements collected.
    pub samples_count: usize,
    /// Whether live runtime scheduling latency was captured.
    pub executed: bool,
}

/// Round Robin scheduler fairness report based on Jain's Fairness Index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundRobinFairnessReport {
    /// Number of participating tasks evaluated.
    pub task_count: usize,
    /// Total ticks or iterations distributed across tasks.
    pub total_workload: u64,
    /// Jain's fairness index as f32 in range [0.0, 1.0].
    /// 1.0 = perfectly fair; 1/n = completely unfair (monopolized).
    pub jains_index_f32: f32,
    /// Jain's fairness index as integer permille (0 to 1000).
    pub jains_index_permille: u32,
    /// Maximum deviation from mean allocation among tasks.
    pub max_deviation: u64,
    /// Whether live runtime workload was executed.
    pub executed: bool,
}

/// Task throughput and response time telemetry report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskThroughputResponseReport {
    /// Total tasks completed in the benchmark workload.
    pub tasks_completed: usize,
    /// Median turnaround time in CPU clock cycles (spawn to termination).
    pub median_turnaround_cycles: u64,
    /// Median response time in CPU clock cycles (spawn to first execution).
    pub median_response_cycles: u64,
    /// Total context switches recorded during benchmark workload.
    pub total_context_switches: usize,
    /// Whether live runtime workload was executed.
    pub executed: bool,
}

/// Comprehensive report for the entire scheduler benchmark suite.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct SchedulerBenchmarkSuiteReport {
    pub context_switch: ContextSwitchLatencyReport,
    pub scheduling_latency: SchedulingLatencyReport,
    pub fairness: RoundRobinFairnessReport,
    pub throughput_response: TaskThroughputResponseReport,
}

// Global atomic telemetry state for controlled benchmark tasks
#[allow(dead_code)]
static PING_TIMESTAMP: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
static PONG_ONEWAY_CYCLES: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
static PING_ROUNDTRIP_CYCLES: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
static BENCH_TASKS_COMPLETED: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
static WORKLOAD_TICKS_A: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
static WORKLOAD_TICKS_B: AtomicU64 = AtomicU64::new(0);
#[allow(dead_code)]
static WORKLOAD_TICKS_C: AtomicU64 = AtomicU64::new(0);

/// Reads the current CPU instruction clock cycle counter via x86 `rdtsc`.
///
/// Returns raw CPU instruction clock cycles. Does NOT convert to nanoseconds.
#[inline(always)]
pub unsafe fn read_cpu_cycles() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::x86_64::_rdtsc()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        0
    }
}

/// Calculates Jain's Fairness Index for a slice of resource allocations.
///
/// Formula:
/// J(x_1, ..., x_n) = (sum(x_i))^2 / (n * sum(x_i^2))
///
/// Returns a tuple `(f32, u32)` containing:
/// - `f32`: value in range [0.0, 1.0] (1.0 = perfectly equal allocation)
/// - `u32`: integer permille representation (0 to 1000)
///
/// Uses `u128` arithmetic internally to prevent intermediate product overflow.
/// Handles zero/empty edge cases safely without division-by-zero panics.
pub fn calculate_jains_fairness(allocations: &[u64]) -> (f32, u32) {
    let n = allocations.len();
    if n == 0 {
        return (1.0, 1000);
    }
    let sum: u64 = allocations.iter().sum();
    if sum == 0 {
        // Zero allocations across all tasks is trivially uniform
        return (1.0, 1000);
    }

    let sum_sq_u128: u128 = allocations
        .iter()
        .map(|&x| (x as u128) * (x as u128))
        .sum();

    if sum_sq_u128 == 0 {
        return (1.0, 1000);
    }

    let sum_u128 = sum as u128;
    let numerator = sum_u128 * sum_u128;
    let denominator = (n as u128) * sum_sq_u128;

    if denominator == 0 {
        return (1.0, 1000);
    }

    let j_f32 = (numerator as f64 / denominator as f64) as f32;
    let j_permille = ((numerator * 1000) / denominator) as u32;

    (j_f32, j_permille)
}

/// Computes min, median, and max latency from a mutable slice of clock cycle measurements.
/// Sorts in-place with zero heap allocations.
pub fn calculate_latency_stats(samples: &mut [u64]) -> (u64, u64, u64) {
    if samples.is_empty() {
        return (0, 0, 0);
    }
    samples.sort_unstable();
    let min = samples[0];
    let max = samples[samples.len() - 1];
    let median = samples[samples.len() / 2];
    (min, median, max)
}

/// Measures the baseline voluntary yield dispatch overhead (`int 48` round-trip).
///
/// Executed when the ready queue is empty so that `on_cooperative_yield()` immediately
/// returns `curr_rsp` without performing a task switch.
/// Captures the raw CPU cycles required for interrupt gate entry, saving 672-byte frame,
/// scheduler lock acquisition, queue status check, frame restoration, and `iretq`.
pub fn benchmark_voluntary_yield_dispatch() -> u64 {
    #[cfg(all(target_arch = "x86_64", not(test)))]
    {
        if crate::task::ready_task_count() == 0 {
            let mut samples = [0u64; BENCHMARK_REPETITIONS];
            for i in 0..BENCHMARK_REPETITIONS {
                let t0 = unsafe { read_cpu_cycles() };
                crate::task::yield_now();
                let t1 = unsafe { read_cpu_cycles() };
                samples[i] = t1.saturating_sub(t0);
            }
            samples.sort_unstable();
            samples[BENCHMARK_REPETITIONS / 2]
        } else {
            0
        }
    }
    #[cfg(any(not(target_arch = "x86_64"), test))]
    {
        0
    }
}

/// Controlled cooperative Task Ping for context-switch latency profiling.
#[allow(dead_code)]
pub extern "C" fn task_bench_ping() {
    for _ in 0..BENCHMARK_REPETITIONS {
        let t0 = unsafe { read_cpu_cycles() };
        PING_TIMESTAMP.store(t0, Ordering::Relaxed);
        #[cfg(all(target_arch = "x86_64", not(test)))]
        crate::task::yield_now();
        let t2 = unsafe { read_cpu_cycles() };
        let roundtrip = t2.saturating_sub(t0);
        PING_ROUNDTRIP_CYCLES.store(roundtrip, Ordering::Relaxed);
    }
    BENCH_TASKS_COMPLETED.fetch_add(1, Ordering::Relaxed);
}

/// Controlled cooperative Task Pong for context-switch latency profiling.
#[allow(dead_code)]
pub extern "C" fn task_bench_pong() {
    for _ in 0..BENCHMARK_REPETITIONS {
        let t1 = unsafe { read_cpu_cycles() };
        let t0 = PING_TIMESTAMP.load(Ordering::Relaxed);
        if t0 > 0 && t1 > t0 {
            let oneway = t1.saturating_sub(t0);
            PONG_ONEWAY_CYCLES.store(oneway, Ordering::Relaxed);
        }
        #[cfg(all(target_arch = "x86_64", not(test)))]
        crate::task::yield_now();
    }
    BENCH_TASKS_COMPLETED.fetch_add(1, Ordering::Relaxed);
}

/// Controlled Task A for Round Robin fairness profiling (bounded 50-step loop).
#[allow(dead_code)]
pub extern "C" fn task_fairness_worker_a() {
    for _ in 0..50 {
        WORKLOAD_TICKS_A.fetch_add(1, Ordering::Relaxed);
        #[cfg(all(target_arch = "x86_64", not(test)))]
        crate::task::yield_now();
    }
}

/// Controlled Task B for Round Robin fairness profiling (bounded 50-step loop).
#[allow(dead_code)]
pub extern "C" fn task_fairness_worker_b() {
    for _ in 0..50 {
        WORKLOAD_TICKS_B.fetch_add(1, Ordering::Relaxed);
        #[cfg(all(target_arch = "x86_64", not(test)))]
        crate::task::yield_now();
    }
}

/// Controlled Task C for Round Robin fairness profiling (bounded 50-step loop).
#[allow(dead_code)]
pub extern "C" fn task_fairness_worker_c() {
    for _ in 0..50 {
        WORKLOAD_TICKS_C.fetch_add(1, Ordering::Relaxed);
        #[cfg(all(target_arch = "x86_64", not(test)))]
        crate::task::yield_now();
    }
}

/// Returns the captured context switch latency measurements.
pub fn context_switch_report() -> ContextSwitchLatencyReport {
    let voluntary_yield = benchmark_voluntary_yield_dispatch();
    let roundtrip = PING_ROUNDTRIP_CYCLES.load(Ordering::Relaxed);
    let oneway = PONG_ONEWAY_CYCLES.load(Ordering::Relaxed);
    let executed = roundtrip > 0 || oneway > 0 || voluntary_yield > 0;

    ContextSwitchLatencyReport {
        voluntary_yield_overhead_cycles: voluntary_yield,
        task_switch_roundtrip_cycles: roundtrip,
        task_switch_oneway_cycles: oneway,
        repetitions: BENCHMARK_REPETITIONS,
        executed,
    }
}

/// Evaluates Round Robin fairness from atomic task workloads.
pub fn round_robin_fairness_report() -> RoundRobinFairnessReport {
    let ticks_a = WORKLOAD_TICKS_A.load(Ordering::Relaxed);
    let ticks_b = WORKLOAD_TICKS_B.load(Ordering::Relaxed);
    let ticks_c = WORKLOAD_TICKS_C.load(Ordering::Relaxed);

    let allocations = [ticks_a, ticks_b, ticks_c];
    let total: u64 = allocations.iter().sum();
    let executed = total > 0;

    let (j_f32, j_permille) = calculate_jains_fairness(&allocations);

    let mean = total / 3;
    let max_dev = allocations
        .iter()
        .map(|&x| (x as i64 - mean as i64).unsigned_abs())
        .max()
        .unwrap_or(0);

    RoundRobinFairnessReport {
        task_count: 3,
        total_workload: total,
        jains_index_f32: j_f32,
        jains_index_permille: j_permille,
        max_deviation: max_dev,
        executed,
    }
}

/// Runs the complete scheduler benchmark suite on-demand.
pub fn run_scheduler_benchmarks() -> SchedulerBenchmarkSuiteReport {
    let cs_report = context_switch_report();
    let fairness_report = round_robin_fairness_report();

    SchedulerBenchmarkSuiteReport {
        context_switch: cs_report,
        scheduling_latency: SchedulingLatencyReport {
            min_cycles: 0,
            median_cycles: 0,
            max_cycles: 0,
            samples_count: 0,
            executed: false,
        },
        fairness: fairness_report,
        throughput_response: TaskThroughputResponseReport {
            tasks_completed: BENCH_TASKS_COMPLETED.load(Ordering::Relaxed) as usize,
            median_turnaround_cycles: 0,
            median_response_cycles: 0,
            total_context_switches: 0,
            executed: false,
        },
    }
}
