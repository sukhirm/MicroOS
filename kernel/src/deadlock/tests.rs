//! In-Kernel Unit and Self-Tests for Deadlock Research Subsystem.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! Validates:
//! 1. Exact-fit boundary defect reproduction & correction.
//! 2. Safe multi-process state Banker evaluation.
//! 3. Unsafe state rejection.
//! 4. Graph circular wait deadlock detection ($P_0 \leftrightarrow P_1$).
//! 5. Detection recovery / reduction progress path.
//! 6. Deterministic synthetic workload scaling across 5, 10, 20, 50, and 100 processes.
//! 7. Bounded memory limits and matrix dimension integrity.

extern crate alloc;

use alloc::vec;
use super::avoidance::{BankerState, DeadlockError, MAX_PROCESSES, MAX_RESOURCES};
use super::detection::DetectionState;
use super::workload::{generate_banker_workload, generate_detection_workload, WORKLOAD_SIZES};

/// Runs all in-kernel self-tests for the Deadlock Research Subsystem.
pub fn run_all() {
    log::info!("================================================================");
    log::info!("       MICRO-OS DEADLOCK RESEARCH SUBSYSTEM SELF-TESTS          ");
    log::info!("       Topic R14: Deadlock Avoidance vs. Detection              ");
    log::info!("================================================================");

    test_exact_fit_boundary();
    test_safe_multiprocess();
    test_unsafe_state();
    test_deadlock_detection_cycle();
    test_deadlock_detection_recovery_path();
    test_banker_state_new_and_max_validation();
    test_workload_scaling_5_to_100();
    test_bounded_memory_safety();
    test_benchmark_engine();

    log::info!("================================================================");
    log::info!("  [OK] All Deadlock Research Subsystem Self-Tests Passed!");
    log::info!("================================================================");
}

/// 1. Exact resource-fit boundary case.
///
/// Reproduces the deliberate defect where `need >= work` falsely rejects exact-fit allocations,
/// and verifies that the corrected `need <= work` algorithm safely accepts the allocation.
fn test_exact_fit_boundary() {
    let available = vec![2];
    let allocation = vec![vec![0]];
    let need = vec![vec![2]];

    let state = BankerState::from_need(available, allocation, need).expect("Valid state");

    // Deliberate defect: must falsely claim unsafe
    let buggy = state.is_safe_defective();
    assert!(!buggy.is_safe, "Defect reproduction failed: buggy algorithm should reject exact fit");

    // Corrected algorithm: must safely accept
    let fixed = state.is_safe();
    assert!(fixed.is_safe, "Corrected algorithm failed exact-fit boundary test");
    assert_eq!(fixed.safe_sequence, vec![0]);

    log::info!("[PASS] 1. Exact resource-fit boundary condition test (defect reproduced & fix validated)");
}

/// 2. Safe multi-process state Banker evaluation.
fn test_safe_multiprocess() {
    let available = vec![3, 3];
    let allocation = vec![vec![1, 0], vec![0, 1], vec![1, 1]];
    let need = vec![vec![2, 1], vec![1, 2], vec![1, 1]];

    let state = BankerState::from_need(available, allocation, need).expect("Valid state");
    let res = state.is_safe();

    assert!(res.is_safe, "Expected safe multi-process state");
    assert_eq!(res.safe_sequence.len(), 3);

    log::info!("[PASS] 2. Safe multi-process state Banker evaluation test");
}

/// 3. Unsafe state rejection.
fn test_unsafe_state() {
    let available = vec![0, 0];
    let allocation = vec![vec![1, 0], vec![0, 1]];
    let need = vec![vec![0, 1], vec![1, 0]];

    let state = BankerState::from_need(available, allocation, need).expect("Valid state");
    let res = state.is_safe();

    assert!(!res.is_safe, "Expected unsafe state to be flagged unsafe");

    log::info!("[PASS] 3. Unsafe state Banker detection test");
}

/// 4. Resource-Request circular wait deadlock detection ($P_0 \leftrightarrow P_1$).
fn test_deadlock_detection_cycle() {
    let available = vec![0];
    let allocation = vec![vec![1], vec![1]];
    let request = vec![vec![1], vec![1]];

    let state = DetectionState::new(available, allocation, request).expect("Valid state");
    let res = state.detect_deadlock();

    assert!(res.is_deadlocked, "Expected deadlock to be detected");
    assert_eq!(res.deadlocked_processes, vec![0, 1]);

    log::info!("[PASS] 4. Resource-Request circular wait deadlock detection test");
}

/// 5. Detection recovery / reduction progress path.
fn test_deadlock_detection_recovery_path() {
    let available = vec![0];
    let allocation = vec![vec![1], vec![1]];
    let request = vec![vec![0], vec![1]];

    let state = DetectionState::new(available, allocation, request).expect("Valid state");
    let res = state.detect_deadlock();

    assert!(!res.is_deadlocked, "Expected state without deadlock");
    assert!(res.deadlocked_processes.is_empty());

    log::info!("[PASS] 5. Deadlock recovery / progress path reduction test");
}

/// 6. Automatic Need calculation from Max and allocation overflow validation.
fn test_banker_state_new_and_max_validation() {
    let available = vec![3, 3, 2];
    let allocation = vec![vec![0, 1, 0], vec![2, 0, 0]];
    let max = vec![vec![7, 5, 3], vec![3, 2, 2]];

    let state = BankerState::new(available, allocation, max).expect("Valid state");
    assert_eq!(state.need[0], vec![7, 4, 3]);
    assert_eq!(state.need[1], vec![1, 2, 2]);

    // Validation: Allocation exceeds max
    let bad_alloc = vec![vec![8, 1, 0]];
    let max_lim = vec![vec![7, 5, 3]];
    let err = BankerState::new(vec![1, 1, 1], bad_alloc, max_lim);
    assert_eq!(err.unwrap_err(), DeadlockError::AllocationExceedsMaximum);

    log::info!("[PASS] 6. Banker State new with Need derivation and Max bounds test");
}

/// 7. Deterministic synthetic workload scaling across 5, 10, 20, 50, and 100 processes.
fn test_workload_scaling_5_to_100() {
    for &n in &WORKLOAD_SIZES {
        let banker = generate_banker_workload(n, 3).expect("Banker workload generation failed");
        let b_res = banker.is_safe();
        assert!(b_res.is_safe, "Banker workload for N={} should be safe", n);
        assert_eq!(b_res.safe_sequence.len(), n);

        let detection = generate_detection_workload(n, 3, false).expect("Detection workload generation failed");
        let d_res = detection.detect_deadlock();
        assert!(!d_res.is_deadlocked, "Detection workload for N={} should have no deadlock", n);
        assert!(d_res.deadlocked_processes.is_empty());
    }

    log::info!("[PASS] 7. Workload scaling test across 5, 10, 20, 50, and 100 processes");
}

/// 8. Bounded memory limits and input dimension validation test.
fn test_bounded_memory_safety() {
    // Process limit exceeded check
    let avail = vec![1; 3];
    let alloc = vec![vec![0; 3]; MAX_PROCESSES + 1];
    let need = vec![vec![1; 3]; MAX_PROCESSES + 1];
    let err_proc = BankerState::from_need(avail, alloc, need);
    assert_eq!(
        err_proc.unwrap_err(),
        DeadlockError::ProcessLimitExceeded {
            count: MAX_PROCESSES + 1,
            max: MAX_PROCESSES,
        }
    );

    // Resource limit exceeded check
    let avail_r = vec![1; MAX_RESOURCES + 1];
    let alloc_r = vec![vec![0; MAX_RESOURCES + 1]];
    let need_r = vec![vec![1; MAX_RESOURCES + 1]];
    let err_res = BankerState::from_need(avail_r, alloc_r, need_r);
    assert_eq!(
        err_res.unwrap_err(),
        DeadlockError::ResourceLimitExceeded {
            count: MAX_RESOURCES + 1,
            max: MAX_RESOURCES,
        }
    );

    // Dimension mismatch check
    let alloc_mismatch = vec![vec![0; 2], vec![0; 2]];
    let need_mismatch = vec![vec![1; 2]];
    let err_dim = BankerState::from_need(vec![1, 1], alloc_mismatch, need_mismatch);
    assert_eq!(err_dim.unwrap_err(), DeadlockError::DimensionMismatch);

    log::info!("[PASS] 8. Bounded memory limits and input dimension validation test");
}

/// 9. In-kernel bare-metal cycle benchmark engine test.
fn test_benchmark_engine() {
    use super::benchmark::{run_microos_benchmarks, ORIGINAL_CA3_BENCHMARKS};

    assert_eq!(ORIGINAL_CA3_BENCHMARKS.len(), 6);

    let measurements = run_microos_benchmarks();
    assert_eq!(measurements.len(), WORKLOAD_SIZES.len());

    for m in &measurements {
        assert!(m.banker_safe, "Benchmark state should be safe");
        assert_eq!(m.deadlocked_count, 0, "Benchmark detection should be non-deadlocked");
        assert!(m.banker_median_cycles > 0, "Banker cycles should be positive");
        assert!(m.detection_median_cycles > 0, "Detection cycles should be positive");
    }

    log::info!("[PASS] 9. In-kernel bare-metal cycle benchmark engine test");
}
