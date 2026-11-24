//! Host-side automated validation tests for Synchronization Performance Subsystem.
//!
//! Phase 1B: Synchronization Performance Benchmarking
//!
//! ARCHITECTURAL DISTINCTION:
//! These tests run on the host development machine (Windows x86_64) using the standard library
//! to verify algorithmic correctness, race-freedom, mutual exclusion, permit counting,
//! and benchmark harness structure.
//!
//! Bare-metal CPU instruction cycles are captured exclusively via bare-metal RDTSC in the
//! Micro-OS kernel on physical hardware. Host CPU cycles reflect host OS scheduling and are
//! explicitly NOT conflated with bare-metal hardware timings.

#![allow(dead_code, unused_imports)]

pub mod task {
    pub fn yield_now() {
        std::thread::yield_now();
    }
    pub fn ready_task_count() -> usize {
        0
    }
}

#[path = "../../kernel/src/sync/mutex.rs"]
pub mod mutex;

#[path = "../../kernel/src/sync/semaphore.rs"]
pub mod semaphore;

#[path = "../../kernel/src/sync/benchmark.rs"]
pub mod benchmark;

use std::sync::Arc;
use std::thread;

use benchmark::{
    benchmark_semaphore_latency, benchmark_spin_mutex_contention, benchmark_spin_mutex_latency,
    run_synchronization_benchmarks,
};
use mutex::SpinMutex;
use semaphore::Semaphore;

#[test]
fn test_host_spin_mutex_uncontended_lock_and_unlock() {
    let mutex = SpinMutex::new(42u64);
    assert!(!mutex.is_locked());

    {
        let mut guard = mutex.lock();
        assert!(mutex.is_locked());
        assert_eq!(*guard, 42);
        *guard = 99;
    }

    assert!(!mutex.is_locked());

    {
        let guard = mutex.lock();
        assert_eq!(*guard, 99);
    }

    assert!(!mutex.is_locked());

    // Uncontended try_lock
    let try_guard = mutex.try_lock();
    assert!(try_guard.is_some());
    drop(try_guard);
    assert!(!mutex.is_locked());
}

#[test]
fn test_host_spin_mutex_contention_detection() {
    let mutex = SpinMutex::new(100u64);
    assert!(!mutex.is_locked());

    let guard = mutex.lock();
    assert!(mutex.is_locked());

    // Multiple try_lock attempts while held must all return None
    for _ in 0..10 {
        assert!(
            mutex.try_lock().is_none(),
            "try_lock on held mutex must immediately return None"
        );
    }

    drop(guard);
    assert!(!mutex.is_locked());

    // Once released, try_lock succeeds
    let post_guard = mutex.try_lock();
    assert!(post_guard.is_some());
    drop(post_guard);
    assert!(!mutex.is_locked());
}

#[test]
fn test_host_spin_mutex_latency_benchmark() {
    let report = benchmark_spin_mutex_latency();
    assert!(report.success, "SpinMutex latency benchmark must succeed");
    assert_eq!(report.repetitions, 5);
}

#[test]
fn test_host_spin_mutex_contention_benchmark() {
    let report = benchmark_spin_mutex_contention();
    assert_eq!(report.attempts, 5);
    assert_eq!(report.failed_acquisitions, 5);
    assert_eq!(report.successful_acquisitions, 0);
    assert_eq!(report.repetitions, 5);
}

#[test]
fn test_host_semaphore_accounting_and_exhaustion() {
    let sem = Semaphore::new(2);
    assert_eq!(sem.count(), 2);

    assert!(sem.try_acquire());
    assert_eq!(sem.count(), 1);

    assert!(sem.try_acquire());
    assert_eq!(sem.count(), 0);

    // 3rd attempt on count == 0 must fail without blocking
    assert!(!sem.try_acquire());
    assert_eq!(sem.count(), 0);

    sem.release();
    assert_eq!(sem.count(), 1);

    sem.release();
    assert_eq!(sem.count(), 2);
}

#[test]
fn test_host_semaphore_latency_and_batch_benchmark() {
    let report = benchmark_semaphore_latency();
    assert!(
        report.permits_restored,
        "Semaphore initial permits must be restored"
    );
    assert_eq!(report.unavailable_attempts, 5);
    assert_eq!(report.unavailable_failures, 5);
    assert_eq!(report.repetitions, 5);
}

#[test]
fn test_host_spin_mutex_multithreaded_concurrency() {
    let counter = Arc::new(SpinMutex::new(0u64));
    let mut handles = Vec::new();

    let thread_count = 4;
    let increments_per_thread = 2_500;

    for _ in 0..thread_count {
        let counter_clone = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..increments_per_thread {
                let mut guard = counter_clone.lock();
                *guard += 1;
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Worker thread panicked");
    }

    let final_val = *counter.lock();
    assert_eq!(
        final_val,
        (thread_count * increments_per_thread) as u64,
        "SpinMutex failed to protect counter under multithreaded concurrency"
    );
}

#[test]
fn test_host_semaphore_multithreaded_producer_consumer() {
    let sem = Arc::new(Semaphore::new(0));
    let sem_producer = Arc::clone(&sem);

    let producer = thread::spawn(move || {
        // Yield briefly before producing permit
        thread::yield_now();
        sem_producer.release();
    });

    // Consumer acquires permit (waiting cooperatively if not yet available)
    sem.acquire();
    assert_eq!(sem.count(), 0);

    producer.join().expect("Producer thread panicked");
}

#[test]
fn test_host_suite_integration() {
    let suite = run_synchronization_benchmarks();
    assert!(suite.mutex_latency.success);
    assert_eq!(
        suite.mutex_contention.failed_acquisitions,
        suite.mutex_contention.attempts
    );
    assert!(suite.semaphore_latency.permits_restored);
}

#[test]
fn test_host_sync_distinction_disclaimer() {
    let host_cycles = unsafe { benchmark::read_cpu_cycles() };
    assert!(host_cycles > 0 || cfg!(not(target_arch = "x86_64")));
}
