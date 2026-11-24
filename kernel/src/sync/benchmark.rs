//! Synchronization Primitives Performance Telemetry & Microbenchmark Subsystem.
//!
//! Phase 1B: SpinMutex Latency, Contention Telemetry & Semaphore Overhead.
//!
//! Provides on-demand microbenchmarks measuring raw CPU instruction cycles
//! via bare-metal x86 RDTSC for:
//! 1. `SpinMutex` uncontended acquisition and release latency (both `lock()` and `try_lock()`).
//! 2. `SpinMutex` non-blocking contention latency and attempt telemetry (`try_lock()` against held lock).
//! 3. `Semaphore` permit acquisition, release, and exhausted-permit rejection overhead.
//! 4. Multi-permit batch acquisition and release throughput.
//! 5. Cooperative yield dispatch overhead (`int 48` round-trip).
//!
//! Architectural Invariants:
//! - All microbenchmarks are on-demand only (never executed in the desktop rendering loop).
//! - Zero memory allocations while holding synchronization locks.
//! - All synchronization state is symmetrically restored before returning.
//! - Cycles are reported strictly as raw CPU clock cycles and NEVER converted to nanoseconds.

use super::mutex::SpinMutex;
use super::semaphore::Semaphore;

/// Default repetitions per benchmark measurement for median outlier filtering.
pub const BENCHMARK_REPETITIONS: usize = 5;

/// Uncontended SpinMutex latency measurement report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpinMutexLatencyReport {
    /// Median CPU clock cycles to acquire uncontended lock via `lock()`.
    pub lock_acquire_cycles_median: u64,
    /// Median CPU clock cycles to release lock via `drop(guard)`.
    pub lock_release_cycles_median: u64,
    /// Median CPU clock cycles to acquire uncontended lock via `try_lock()`.
    pub try_lock_acquire_cycles_median: u64,
    /// Median CPU clock cycles to release try_lock guard.
    pub try_lock_release_cycles_median: u64,
    /// Number of repetitions evaluated.
    pub repetitions: usize,
    /// Whether all acquisitions succeeded and mutex is verified unlocked upon return.
    pub success: bool,
}

/// Contention measurement report for SpinMutex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpinMutexContentionReport {
    /// Total acquisition attempts made during contention test.
    pub attempts: u64,
    /// Successful acquisitions (0 during held-lock test).
    pub successful_acquisitions: u64,
    /// Failed acquisitions due to contention (lock held).
    pub failed_acquisitions: u64,
    /// Median CPU clock cycles spent detecting contention in `try_lock()`.
    pub contention_detection_cycles_median: u64,
    /// Number of repetitions evaluated.
    pub repetitions: usize,
}

/// Semaphore performance measurement report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemaphoreLatencyReport {
    /// Median CPU clock cycles to acquire permit via `try_acquire()` when permits available.
    pub acquire_cycles_median: u64,
    /// Median CPU clock cycles to release permit via `release()`.
    pub release_cycles_median: u64,
    /// Total permits attempted during unavailable test.
    pub unavailable_attempts: u64,
    /// Failed acquisitions when count == 0.
    pub unavailable_failures: u64,
    /// Median CPU clock cycles to detect permit exhaustion in `try_acquire()`.
    pub unavailable_detection_cycles_median: u64,
    /// Batch throughput: median cycles for 10 sequential permit acquisitions.
    pub batch_acquire_cycles_median: u64,
    /// Batch throughput: median cycles for 10 sequential permit releases.
    pub batch_release_cycles_median: u64,
    /// Number of repetitions evaluated.
    pub repetitions: usize,
    /// Initial permit count restored upon exit.
    pub permits_restored: bool,
}

/// Cooperative yield dispatch overhead report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooperativeYieldReport {
    /// Median CPU clock cycles for voluntary `yield_now()` round-trip.
    pub yield_cycles_median: u64,
    /// Number of repetitions evaluated.
    pub repetitions: usize,
    /// Whether cooperative yield was executed or bypassed on host.
    pub executed: bool,
}

/// Comprehensive report for the entire synchronization benchmark suite.
#[derive(Debug, Clone, Copy)]
pub struct SynchronizationBenchmarkSuiteReport {
    pub mutex_latency: SpinMutexLatencyReport,
    pub mutex_contention: SpinMutexContentionReport,
    pub semaphore_latency: SemaphoreLatencyReport,
    pub cooperative_yield: CooperativeYieldReport,
}

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

/// Measures uncontended SpinMutex acquisition and release latency across 5 repetitions.
///
/// Separate measurements are collected for both `lock()` and `try_lock()`.
/// Uses warm-up and median filtering to eliminate outlier timing spikes.
pub fn benchmark_spin_mutex_latency() -> SpinMutexLatencyReport {
    let mutex = SpinMutex::new(0u64);

    // Warm-up pass to populate CPU caches
    {
        let guard = mutex.lock();
        drop(guard);
        if let Some(guard) = mutex.try_lock() {
            drop(guard);
        }
    }

    let mut lock_acquire_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut lock_release_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut try_acquire_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut try_release_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut all_ok = true;

    for i in 0..BENCHMARK_REPETITIONS {
        // 1. Measure uncontended lock() acquisition
        let t0 = unsafe { read_cpu_cycles() };
        let guard = mutex.lock();
        let t1 = unsafe { read_cpu_cycles() };
        lock_acquire_samples[i] = t1.saturating_sub(t0);

        // 2. Measure lock() release
        let t2 = unsafe { read_cpu_cycles() };
        drop(guard);
        let t3 = unsafe { read_cpu_cycles() };
        lock_release_samples[i] = t3.saturating_sub(t2);

        // 3. Measure uncontended try_lock() acquisition
        let t4 = unsafe { read_cpu_cycles() };
        let try_res = mutex.try_lock();
        let t5 = unsafe { read_cpu_cycles() };
        try_acquire_samples[i] = t5.saturating_sub(t4);

        if let Some(guard) = try_res {
            // 4. Measure try_lock() release
            let t6 = unsafe { read_cpu_cycles() };
            drop(guard);
            let t7 = unsafe { read_cpu_cycles() };
            try_release_samples[i] = t7.saturating_sub(t6);
        } else {
            all_ok = false;
        }
    }

    lock_acquire_samples.sort_unstable();
    lock_release_samples.sort_unstable();
    try_acquire_samples.sort_unstable();
    try_release_samples.sort_unstable();

    SpinMutexLatencyReport {
        lock_acquire_cycles_median: lock_acquire_samples[BENCHMARK_REPETITIONS / 2],
        lock_release_cycles_median: lock_release_samples[BENCHMARK_REPETITIONS / 2],
        try_lock_acquire_cycles_median: try_acquire_samples[BENCHMARK_REPETITIONS / 2],
        try_lock_release_cycles_median: try_release_samples[BENCHMARK_REPETITIONS / 2],
        repetitions: BENCHMARK_REPETITIONS,
        success: all_ok && !mutex.is_locked(),
    }
}

/// Measures SpinMutex non-blocking contention latency and attempt telemetry via `try_lock()`.
///
/// Holds the mutex and measures the latency and failure response of `try_lock()` attempts.
/// Symmetrically drops the holding guard before returning.
pub fn benchmark_spin_mutex_contention() -> SpinMutexContentionReport {
    let mutex = SpinMutex::new(0u64);

    // Acquire lock first to induce held state
    let guard = mutex.lock();

    let mut contention_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut failed_count = 0u64;
    let mut success_count = 0u64;

    for i in 0..BENCHMARK_REPETITIONS {
        let t0 = unsafe { read_cpu_cycles() };
        let res = mutex.try_lock();
        let t1 = unsafe { read_cpu_cycles() };
        contention_samples[i] = t1.saturating_sub(t0);

        if res.is_none() {
            failed_count += 1;
        } else {
            success_count += 1;
            drop(res);
        }
    }

    // Release held guard and restore mutex to unlocked state
    drop(guard);

    contention_samples.sort_unstable();

    SpinMutexContentionReport {
        attempts: BENCHMARK_REPETITIONS as u64,
        successful_acquisitions: success_count,
        failed_acquisitions: failed_count,
        contention_detection_cycles_median: contention_samples[BENCHMARK_REPETITIONS / 2],
        repetitions: BENCHMARK_REPETITIONS,
    }
}

/// Measures Semaphore permit acquisition, release, and exhaustion overhead.
///
/// Tests single-permit uncontended acquisition/release, unavailable permit detection (`try_acquire` when count == 0),
/// and 10-permit batch acquisition/release throughput.
/// All permit counts are verified symmetrically restored.
pub fn benchmark_semaphore_latency() -> SemaphoreLatencyReport {
    let sem = Semaphore::new(1);

    // Warm-up pass
    {
        if sem.try_acquire() {
            sem.release();
        }
    }

    let mut acquire_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut release_samples = [0u64; BENCHMARK_REPETITIONS];

    for i in 0..BENCHMARK_REPETITIONS {
        let t0 = unsafe { read_cpu_cycles() };
        let acquired = sem.try_acquire();
        let t1 = unsafe { read_cpu_cycles() };
        acquire_samples[i] = t1.saturating_sub(t0);

        if acquired {
            let t2 = unsafe { read_cpu_cycles() };
            sem.release();
            let t3 = unsafe { read_cpu_cycles() };
            release_samples[i] = t3.saturating_sub(t2);
        }
    }

    acquire_samples.sort_unstable();
    release_samples.sort_unstable();

    // Unavailable permit test: drain the permit first
    let drained = sem.try_acquire();
    let initial_drained = drained && sem.count() == 0;

    let mut unavailable_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut unavail_failures = 0u64;

    for i in 0..BENCHMARK_REPETITIONS {
        let t0 = unsafe { read_cpu_cycles() };
        let res = sem.try_acquire();
        let t1 = unsafe { read_cpu_cycles() };
        unavailable_samples[i] = t1.saturating_sub(t0);

        if !res {
            unavail_failures += 1;
        } else {
            sem.release();
        }
    }

    // Restore the drained permit
    if initial_drained {
        sem.release();
    }
    unavailable_samples.sort_unstable();

    // 10-permit batch throughput test
    let batch_sem = Semaphore::new(10);
    let mut batch_acquire_samples = [0u64; BENCHMARK_REPETITIONS];
    let mut batch_release_samples = [0u64; BENCHMARK_REPETITIONS];

    for rep in 0..BENCHMARK_REPETITIONS {
        let t0 = unsafe { read_cpu_cycles() };
        for _ in 0..10 {
            batch_sem.try_acquire();
        }
        let t1 = unsafe { read_cpu_cycles() };
        batch_acquire_samples[rep] = t1.saturating_sub(t0);

        let t2 = unsafe { read_cpu_cycles() };
        for _ in 0..10 {
            batch_sem.release();
        }
        let t3 = unsafe { read_cpu_cycles() };
        batch_release_samples[rep] = t3.saturating_sub(t2);
    }

    batch_acquire_samples.sort_unstable();
    batch_release_samples.sort_unstable();

    SemaphoreLatencyReport {
        acquire_cycles_median: acquire_samples[BENCHMARK_REPETITIONS / 2],
        release_cycles_median: release_samples[BENCHMARK_REPETITIONS / 2],
        unavailable_attempts: BENCHMARK_REPETITIONS as u64,
        unavailable_failures: unavail_failures,
        unavailable_detection_cycles_median: unavailable_samples[BENCHMARK_REPETITIONS / 2],
        batch_acquire_cycles_median: batch_acquire_samples[BENCHMARK_REPETITIONS / 2],
        batch_release_cycles_median: batch_release_samples[BENCHMARK_REPETITIONS / 2],
        repetitions: BENCHMARK_REPETITIONS,
        permits_restored: sem.count() == 1 && batch_sem.count() == 10,
    }
}

/// Measures voluntary cooperative yield round-trip latency (`int 48`).
///
/// In the bare-metal kernel, executes `yield_now()` when safe and measures the total cycles
/// for software interrupt entry, register save/restore, scheduler queue inspection, and iretq.
/// Under host testing, returns 0 cycles without executing privileged assembly.
pub fn benchmark_cooperative_yield_latency() -> CooperativeYieldReport {
    #[cfg(all(target_arch = "x86_64", not(test)))]
    {
        // Safe check: only invoke if ready queue is empty to avoid preempting queued tasks early
        if crate::task::ready_task_count() == 0 {
            let mut samples = [0u64; BENCHMARK_REPETITIONS];
            for i in 0..BENCHMARK_REPETITIONS {
                let t0 = unsafe { read_cpu_cycles() };
                crate::task::yield_now();
                let t1 = unsafe { read_cpu_cycles() };
                samples[i] = t1.saturating_sub(t0);
            }
            samples.sort_unstable();
            CooperativeYieldReport {
                yield_cycles_median: samples[BENCHMARK_REPETITIONS / 2],
                repetitions: BENCHMARK_REPETITIONS,
                executed: true,
            }
        } else {
            CooperativeYieldReport {
                yield_cycles_median: 0,
                repetitions: BENCHMARK_REPETITIONS,
                executed: false,
            }
        }
    }
    #[cfg(any(not(target_arch = "x86_64"), test))]
    {
        CooperativeYieldReport {
            yield_cycles_median: 0,
            repetitions: BENCHMARK_REPETITIONS,
            executed: false,
        }
    }
}

/// Executes the complete suite of synchronization performance benchmarks.
pub fn run_synchronization_benchmarks() -> SynchronizationBenchmarkSuiteReport {
    SynchronizationBenchmarkSuiteReport {
        mutex_latency: benchmark_spin_mutex_latency(),
        mutex_contention: benchmark_spin_mutex_contention(),
        semaphore_latency: benchmark_semaphore_latency(),
        cooperative_yield: benchmark_cooperative_yield_latency(),
    }
}
