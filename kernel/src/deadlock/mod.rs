//! Micro-OS Deadlock Research Subsystem.
//!
//! Topic R14: Deadlock Avoidance vs. Detection
//!
//! Provides bare-metal implementations of:
//! - Banker's Safety Algorithm for Deadlock Avoidance (corrected + deliberate defect reproduction)
//! - Graph-based / Resource-Request Deadlock Detection (Wait-For Graph reduction)
//! - Deterministic synthetic workload generator for 5, 10, 20, 50, and 100 processes
//! - In-kernel `rdtsc` cycle benchmark runner
//! - Research report baseline data preservation (`CA3_R14` and `deadlock_results_v2`)
//! - In-kernel self-test validation suite (Tests 1–7)

#![allow(dead_code, unused_imports)]

pub mod avoidance;
pub mod detection;
pub mod workload;
pub mod benchmark;
pub mod tests;

pub use avoidance::{BankerResult, BankerState, DeadlockError, MAX_PROCESSES, MAX_RESOURCES};
pub use detection::{DetectionResult, DetectionState};
pub use workload::{
    generate_banker_workload, generate_detection_workload, DEFAULT_RESOURCE_TYPES, WORKLOAD_SIZES,
};
pub use benchmark::{
    run_microos_benchmarks, MicroOsBenchmarkMeasurement, OriginalBenchmarkEntry,
    ORIGINAL_CA3_BENCHMARKS, ORIGINAL_V2_BENCHMARKS,
};

/// Runs the complete in-kernel self-test suite for the Deadlock Research Subsystem.
pub fn run_self_tests() {
    tests::run_all();
}
