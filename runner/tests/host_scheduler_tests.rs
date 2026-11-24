//! Host-side automated validation tests for Scheduler and Context-Switch Performance Subsystem.
//!
//! Phase 1C: Scheduler and Context-Switch Performance Benchmarking
//!
//! ARCHITECTURAL DISTINCTION:
//! These tests execute on the host development machine (Windows x86_64) using the standard library
//! to mathematically verify Jain's Fairness Index formula, overflow resistance, statistical sorting,
//! latency bounds, and benchmark harness structures.
//!
//! Bare-metal CPU instruction cycles are captured exclusively via bare-metal RDTSC in the
//! Micro-OS kernel during physical hardware execution. Host CPU cycles reflect host OS scheduling
//! and are explicitly NOT conflated with bare-metal hardware timings.

#![allow(dead_code, unused_imports)]

#[path = "../../kernel/src/task/benchmark.rs"]
pub mod benchmark;

use benchmark::{
    calculate_jains_fairness, calculate_latency_stats, context_switch_report,
    round_robin_fairness_report, run_scheduler_benchmarks,
};

#[test]
fn test_host_jains_fairness_equal_allocation() {
    // 3 tasks with identical resource allocation -> J = 1.0 (1000 permille)
    let (j_3, p_3) = calculate_jains_fairness(&[100, 100, 100]);
    assert_eq!(p_3, 1000, "Equal 3-task allocation must be 1000 permille");
    assert!((j_3 - 1.0).abs() < 1e-5);

    // 4 tasks with identical resource allocation -> J = 1.0 (1000 permille)
    let (j_4, p_4) = calculate_jains_fairness(&[5000, 5000, 5000, 5000]);
    assert_eq!(p_4, 1000, "Equal 4-task allocation must be 1000 permille");
    assert!((j_4 - 1.0).abs() < 1e-5);
}

#[test]
fn test_host_jains_fairness_skewed_allocations() {
    // 2:1 skewed allocation: x = [100, 50]
    // sum = 150, sum^2 = 22,500
    // sum_sq = 10,000 + 2,500 = 12,500; 2 * sum_sq = 25,000
    // J = 22,500 / 25,000 = 0.90 (900 permille)
    let (j_skew, p_skew) = calculate_jains_fairness(&[100, 50]);
    assert_eq!(p_skew, 900, "2:1 allocation must yield 900 permille");
    assert!((j_skew - 0.90).abs() < 1e-5);

    // Completely monopolized allocation across 2 tasks: x = [100, 0]
    // sum = 100, sum^2 = 10,000
    // sum_sq = 10,000; 2 * sum_sq = 20,000
    // J = 10,000 / 20,000 = 0.50 (500 permille, 1/n)
    let (j_mono, p_mono) = calculate_jains_fairness(&[100, 0]);
    assert_eq!(p_mono, 500, "Monopolized 2-task allocation must be 1/2 = 500 permille");
    assert!((j_mono - 0.50).abs() < 1e-5);

    // 2 tasks active, 1 starvation: x = [100, 100, 0]
    // sum = 200, sum^2 = 40,000
    // sum_sq = 20,000; 3 * sum_sq = 60,000
    // J = 40,000 / 60,000 = 0.6666... (666 permille)
    let (j_starve, p_starve) = calculate_jains_fairness(&[100, 100, 0]);
    assert_eq!(p_starve, 666);
    assert!((j_starve - 0.6666667).abs() < 1e-4);
}

#[test]
fn test_host_jains_fairness_edge_cases() {
    // Edge Case 1: Empty slice
    let (j_empty, p_empty) = calculate_jains_fairness(&[]);
    assert_eq!(p_empty, 1000);
    assert_eq!(j_empty, 1.0);

    // Edge Case 2: All zeros
    let (j_zeros, p_zeros) = calculate_jains_fairness(&[0, 0, 0]);
    assert_eq!(p_zeros, 1000);
    assert_eq!(j_zeros, 1.0);

    // Edge Case 3: Single task
    let (j_single, p_single) = calculate_jains_fairness(&[42]);
    assert_eq!(p_single, 1000);
    assert_eq!(j_single, 1.0);
}

#[test]
fn test_host_jains_fairness_large_numbers_no_overflow() {
    // Allocation counts exceeding 2^32 where sum^2 would overflow u64
    // 5 tasks with 5,000,000,000 ticks each: sum = 25 * 10^9 -> sum^2 = 6.25 * 10^20 (> u64::MAX = 1.84 * 10^19)
    let large_val = 5_000_000_000u64;
    let allocations = [large_val, large_val, large_val, large_val, large_val];
    let (j_large, p_large) = calculate_jains_fairness(&allocations);
    assert_eq!(
        p_large, 1000,
        "u128 arithmetic must prevent product overflow on multi-billion counts"
    );
    assert!((j_large - 1.0).abs() < 1e-5);
}

#[test]
fn test_host_latency_stats_calculation() {
    let mut samples = [500u64, 120, 340, 980, 210];
    let (min, median, max) = calculate_latency_stats(&mut samples);
    assert_eq!(min, 120);
    assert_eq!(median, 340);
    assert_eq!(max, 980);

    let mut single = [77u64];
    let (min1, med1, max1) = calculate_latency_stats(&mut single);
    assert_eq!(min1, 77);
    assert_eq!(med1, 77);
    assert_eq!(max1, 77);

    let mut empty: [u64; 0] = [];
    let (min0, med0, max0) = calculate_latency_stats(&mut empty);
    assert_eq!(min0, 0);
    assert_eq!(med0, 0);
    assert_eq!(max0, 0);
}

#[test]
fn test_host_round_robin_fairness_report() {
    let report = round_robin_fairness_report();
    assert_eq!(report.task_count, 3);
}

#[test]
fn test_host_context_switch_report() {
    let report = context_switch_report();
    assert_eq!(report.repetitions, 5);
}

#[test]
fn test_host_scheduler_benchmark_suite_integration() {
    let suite = run_scheduler_benchmarks();
    assert_eq!(suite.context_switch.repetitions, 5);
    assert_eq!(suite.fairness.task_count, 3);
    assert_eq!(suite.scheduling_latency.samples_count, 0);
}

#[test]
fn test_host_scheduler_distinction_disclaimer() {
    let host_cycles = unsafe { benchmark::read_cpu_cycles() };
    assert!(host_cycles > 0 || cfg!(not(target_arch = "x86_64")));
}
