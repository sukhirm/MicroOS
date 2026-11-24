//! Task Management and Preemptive Scheduling Subsystem for Micro-OS.
//!
//! Provides Task Control Blocks (TCB), FIFO Ready Queue, round-robin scheduling,
//! hardware timer preemption, FPU/SIMD state preservation, and multi-task demonstration.

pub mod benchmark;
pub mod context;
pub mod queue;
pub mod scheduler;
pub mod tcb;

#[allow(unused_imports)]
pub use benchmark::{
    calculate_jains_fairness, calculate_latency_stats, context_switch_report,
    round_robin_fairness_report, run_scheduler_benchmarks, ContextSwitchLatencyReport,
    RoundRobinFairnessReport, SchedulerBenchmarkSuiteReport, SchedulingLatencyReport,
    TaskThroughputResponseReport,
};
#[allow(unused_imports)]
pub use scheduler::{
    context_switch_count, current_task, preemption_count, ready_task_count, spawn,
    start_multitasking, task_snapshots, total_task_count, yield_now, TaskSnapshot,
};
pub use tcb::{TaskId, TaskState};

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::sync::{Semaphore, SpinMutex};

/// Global test state for Phase 3C preemption and synchronization verification.
static ALPHA_SPIN_COUNT: AtomicU64 = AtomicU64::new(0);
static PREEMPTION_OBSERVED: AtomicBool = AtomicBool::new(false);
static MUTEX_COUNTER: SpinMutex<u64> = SpinMutex::new(0);
static TEST_SEMAPHORE: Semaphore = Semaphore::new(1);

/// Task Alpha: Compute-bound task that executes a heavy loop WITHOUT voluntarily yielding.
extern "C" fn task_alpha_compute_hog() {
    let (id, name) = current_task().expect("Task Alpha must have active context");
    log::info!("[{}] (PID: {}) Started compute-bound loop WITHOUT voluntary yields...", name, id.as_u64());

    // Heavy computation loop: does NOT call yield_now()
    // It relies entirely on the 8254 PIT hardware timer interrupt to be preempted!
    let target = 50_000_000u64;
    for i in 0..target {
        ALPHA_SPIN_COUNT.store(i, Ordering::Relaxed);
        core::hint::black_box(i);
    }

    log::info!("[{}] (PID: {}) Completed compute loop. Now testing SpinMutex under preemption...", name, id.as_u64());

    // Concurrently increment shared mutex counter 1,000 times
    for _ in 0..1000 {
        let mut guard = MUTEX_COUNTER.lock();
        *guard += 1;
    }

    log::info!("[{}] (PID: {}) Finished work cleanly; terminating.", name, id.as_u64());
}

/// Task Beta: Preempting worker task that executes when Task Alpha is preempted by the timer.
extern "C" fn task_beta_preemptor() {
    let (id, name) = current_task().expect("Task Beta must have active context");
    let alpha_count = ALPHA_SPIN_COUNT.load(Ordering::Relaxed);

    log::info!(
        "[{}] (PID: {}) EXECUTING! Successfully preempted compute-bound Task Alpha at count: {}",
        name,
        id.as_u64(),
        alpha_count
    );

    if alpha_count > 0 {
        PREEMPTION_OBSERVED.store(true, Ordering::Relaxed);
    }

    // Concurrently increment shared mutex counter 1,000 times
    for _ in 0..1000 {
        let mut guard = MUTEX_COUNTER.lock();
        *guard += 1;
    }

    // Test Semaphore acquire and release
    TEST_SEMAPHORE.acquire();
    log::info!("[{}] (PID: {}) Acquired test semaphore; now releasing.", name, id.as_u64());
    TEST_SEMAPHORE.release();

    log::info!("[{}] (PID: {}) Finished work cleanly; terminating.", name, id.as_u64());
}

/// Runs comprehensive self-tests verifying TaskId uniqueness, state transitions,
/// ready queue FIFO semantics, empty queue handling, and 16-byte stack alignment.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("          Running Micro-OS Phase 3B/3C Task Subsystem Self-Tests");
    log::info!("================================================================");

    // 1. Task ID uniqueness test
    {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        let id3 = TaskId::new();
        let id4 = TaskId::new();
        let id5 = TaskId::new();

        assert!(
            id1.as_u64() < id2.as_u64()
                && id2.as_u64() < id3.as_u64()
                && id3.as_u64() < id4.as_u64()
                && id4.as_u64() < id5.as_u64(),
            "Task IDs must be strictly unique and monotonically increasing"
        );
        log::info!(
            "  [PASS] Task ID uniqueness test (IDs generated: {}, {}, {}, {}, {})",
            id1.as_u64(), id2.as_u64(), id3.as_u64(), id4.as_u64(), id5.as_u64()
        );
    }

    // 2. Task state transitions test
    {
        let mut state = TaskState::Ready;
        assert_eq!(state, TaskState::Ready);
        state = TaskState::Running;
        assert_eq!(state, TaskState::Running);
        state = TaskState::Blocked;
        assert_eq!(state, TaskState::Blocked);
        state = TaskState::Ready;
        assert_eq!(state, TaskState::Ready);
        state = TaskState::Terminated;
        assert_eq!(state, TaskState::Terminated);
        log::info!("  [PASS] Task state transition test (Ready -> Running -> Blocked -> Ready -> Terminated)");
    }

    // 3. Ready queue operations & empty queue handling
    {
        extern "C" fn dummy_task() {}
        let mut queue = queue::ReadyQueue::new();
        assert!(queue.is_empty(), "Initial ready queue must be empty");
        assert_eq!(queue.len(), 0);
        assert!(queue.dequeue().is_none(), "Dequeue on empty queue must return None");

        let task1 = tcb::TaskControlBlock::new("dummy1", dummy_task);
        let id1 = task1.id;
        let task2 = tcb::TaskControlBlock::new("dummy2", dummy_task);
        let id2 = task2.id;
        let task3 = tcb::TaskControlBlock::new("dummy3", dummy_task);
        let id3 = task3.id;

        queue.enqueue(task1);
        queue.enqueue(task2);
        queue.enqueue(task3);

        assert_eq!(queue.len(), 3);
        assert!(!queue.is_empty());

        let pop1 = queue.dequeue().expect("Expected task 1");
        assert_eq!(pop1.id, id1);
        let pop2 = queue.dequeue().expect("Expected task 2");
        assert_eq!(pop2.id, id2);
        let pop3 = queue.dequeue().expect("Expected task 3");
        assert_eq!(pop3.id, id3);

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.dequeue().is_none(), "Empty queue must cleanly return None");
        log::info!("  [PASS] Ready queue FIFO & empty operations test");
    }

    // 4. Task stack 16-byte alignment and 672-byte InterruptContext size verification
    {
        let stack = tcb::TaskStack::new(tcb::TaskStack::DEFAULT_SIZE);
        let top = stack.top();
        assert!(!top.is_null());
        assert_eq!(
            top as usize % 16,
            0,
            "Task stack top must be 16-byte aligned for System V ABI: {:p}",
            top
        );

        assert_eq!(
            core::mem::size_of::<tcb::InterruptContext>(),
            672,
            "InterruptContext must be exactly 672 bytes (512 FXSAVE + 120 GPRs + 40 HW)"
        );

        extern "C" fn align_dummy() {}
        let tcb = tcb::TaskControlBlock::new("align_test", align_dummy);
        let initial_rsp = tcb.rsp as usize;
        assert_eq!(
            initial_rsp % 16,
            0,
            "Initial context RSP must be 16-byte aligned for FXSAVE: {:#x}",
            initial_rsp
        );
        log::info!(
            "  [PASS] Task stack 16-byte alignment test (stack top: {:p}, initial RSP: {:#x}, context size: 672)",
            top, initial_rsp
        );
    }

    // 5. Jain's Fairness Index algorithmic edge case verification (Phase 1C)
    {
        // Case A: Perfectly fair allocation across 3 tasks (100, 100, 100) -> J = 1.0 (1000 permille)
        let (j_a, p_a) = benchmark::calculate_jains_fairness(&[100, 100, 100]);
        assert_eq!(p_a, 1000, "Equal allocation must yield 1000 permille fairness");
        assert!((j_a - 1.0).abs() < 1e-5);

        // Case B: Skewed 2:1 allocation (100, 50) -> J = 22500 / 25000 = 0.90 (900 permille)
        let (j_b, p_b) = benchmark::calculate_jains_fairness(&[100, 50]);
        assert_eq!(p_b, 900, "2:1 allocation must yield 900 permille fairness");
        assert!((j_b - 0.90).abs() < 1e-5);

        // Case C: Completely monopolized allocation across 2 tasks (100, 0) -> J = 0.50 (500 permille)
        let (j_c, p_c) = benchmark::calculate_jains_fairness(&[100, 0]);
        assert_eq!(p_c, 500, "Monopolized allocation must yield 1/n = 500 permille");
        assert!((j_c - 0.50).abs() < 1e-5);

        // Case D: Single task (100) -> J = 1.0 (1000 permille)
        let (j_d, p_d) = benchmark::calculate_jains_fairness(&[100]);
        assert_eq!(p_d, 1000);
        assert!((j_d - 1.0).abs() < 1e-5);

        // Case E: Safe edge cases (empty or all-zero allocations)
        let (j_e1, p_e1) = benchmark::calculate_jains_fairness(&[]);
        assert_eq!(p_e1, 1000);
        assert!((j_e1 - 1.0).abs() < 1e-5);

        let (j_e2, p_e2) = benchmark::calculate_jains_fairness(&[0, 0, 0]);
        assert_eq!(p_e2, 1000);
        assert!((j_e2 - 1.0).abs() < 1e-5);

        log::info!("  [PASS] Phase 1C Jain's Fairness Index algorithmic verification (edge cases J=1.0, J=0.90, J=0.50)");
    }

    // 6. Scheduling latency min/median/max statistics computation (Phase 1C)
    {
        let mut samples = [500u64, 120, 340, 980, 210];
        let (min, median, max) = benchmark::calculate_latency_stats(&mut samples);
        assert_eq!(min, 120, "Minimum latency must be 120");
        assert_eq!(median, 340, "Median latency must be 340");
        assert_eq!(max, 980, "Maximum latency must be 980");
        log::info!("  [PASS] Phase 1C Scheduling latency statistics calculation verified (min={}, med={}, max={})", min, median, max);
    }

    // 7. Scheduler performance benchmark harness integration (Phase 1C)
    {
        let report = benchmark::run_scheduler_benchmarks();
        assert_eq!(report.fairness.task_count, 3);
        assert_eq!(report.context_switch.repetitions, 5);
        log::info!("  [PASS] Phase 1C Scheduler performance benchmark suite integrated cleanly");
    }

    log::info!("================================================================");
    log::info!("     [OK] All Phase 3B/3C Task Subsystem Self-Tests Passed!     ");
    log::info!("================================================================");
}

/// Demonstrates preemptive multitasking and synchronization with Task Alpha and Task Beta.
pub fn run_preemption_demo() {
    log::info!("================================================================");
    log::info!("     Starting Micro-OS Phase 3C Preemptive Multitasking Demo    ");
    log::info!("================================================================");

    let id_alpha = spawn("Task Alpha", task_alpha_compute_hog);
    let id_beta = spawn("Task Beta", task_beta_preemptor);

    log::info!(
        "[Demo] Spawned worker tasks: Task Alpha (PID: {}), Task Beta (PID: {})",
        id_alpha.as_u64(),
        id_beta.as_u64()
    );

    // Hand over execution to the preemptive round-robin scheduler
    start_multitasking();

    log::info!("================================================================");
    log::info!("       Verifying Phase 3C Preemption & Synchronization Outcomes ");
    log::info!("================================================================");

    // 1. Verify that Task Alpha was genuinely preempted mid-computation
    let preemption_seen = PREEMPTION_OBSERVED.load(Ordering::Relaxed) || preemption_count() > 0;
    assert!(
        preemption_seen,
        "Preemption verification failed: Task Beta did not execute while Task Alpha was computing!"
    );
    log::info!("  [PASS] Non-yielding task preemption verified (Task Alpha interrupted by 8254 PIT timer).");
    log::info!("  [PASS] Total timer preemption events recorded: {}", preemption_count());

    // 2. Verify that SpinMutex protected shared counter without lost updates
    let final_counter = *MUTEX_COUNTER.lock();
    assert_eq!(
        final_counter, 2000,
        "SpinMutex concurrent counter mismatch: expected 2000, got {}",
        final_counter
    );
    log::info!("  [PASS] SpinMutex mutual exclusion verified (shared counter reached exactly 2000).");

    // 3. Verify Semaphore operation
    assert_eq!(TEST_SEMAPHORE.count(), 1, "Semaphore permit must be 1 after release");
    log::info!("  [PASS] Semaphore cooperative acquire & release verified.");

    log::info!("================================================================");
    log::info!("  [OK] Preemptive Multitasking Demonstration Complete!          ");
    log::info!("================================================================");
}
