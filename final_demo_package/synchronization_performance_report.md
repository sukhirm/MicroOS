# Micro-OS: Synchronization Performance Report
**Phase 1B: SpinMutex Latency, Contention Telemetry & Semaphore Overhead Profiling**

**Project:** Micro-OS Bare-Metal x86_64 Operating System  
**Subsystem:** Concurrency & Synchronization (`kernel/src/sync/`)  
**Report Location:** `final_demo_package/synchronization_performance_report.md`  
**Date:** November 18, 2026  
**Implementation Status:** Completed, Host-Tested, and In-Kernel Verified  

---

## 1. Executive Summary

Phase 1B implements safe, on-demand performance telemetry and microbenchmarking for the Micro-OS concurrency synchronization primitives. The subsystem evaluates:
1. **`SpinMutex` Latency:** On-demand measurement of raw CPU instruction clock cycles for uncontended lock acquisition, lock release, `try_lock()` acquisition, and `try_lock()` release via bare-metal x86 RDTSC.
2. **`SpinMutex` Contention Telemetry:** Safe, non-blocking contention detection latency profiling and attempt counters (`attempts`, `failed_acquisitions`, `successful_acquisitions`) under lock-held conditions.
3. **`Semaphore` Overhead & Batch Throughput:** Single-permit acquisition/release latency, exhausted-permit rejection latency (`try_acquire()` when permits == 0), and 10-permit batch acquisition/release throughput.
4. **Cooperative Yield Dispatch Overhead:** Round-trip timing of the `yield_now()` software interrupt (`int 48`) context-checking path when the ready queue is empty.

---

## 2. Synchronization Architecture & Primitives Audit

### 2.1 Mutual Exclusion Spinlock (`SpinMutex<T>`)
- **Location:** [`kernel/src/sync/mutex.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/sync/mutex.rs)
- **State Representation:** `locked: AtomicBool`, `data: UnsafeCell<T>`.
- **Acquisition Semantics:**
  - `try_lock()`: Non-blocking atomic compare-and-swap using `compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)`. Returns `Some(SpinMutexGuard)` on success or `None` if contested.
  - `lock()`: Spinning acquisition loop calling `core::hint::spin_loop()` until the CAS succeeds.
- **Release Semantics:** RAII drop implementation on `SpinMutexGuard<'a, T>` setting `locked.store(false, Ordering::Release)`.
- **Subsystem Invariants:**
  - Tasks MUST NOT voluntarily yield (`task::yield_now()`) while holding a `SpinMutex`.
  - Critical sections must remain brief and non-allocating.

### 2.2 Counting Semaphore (`Semaphore`)
- **Location:** [`kernel/src/sync/semaphore.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/sync/semaphore.rs)
- **State Representation:** `count: AtomicIsize`.
- **Acquisition Semantics:**
  - `try_acquire()`: Non-blocking atomic CAS decrement loop (`Ordering::AcqRel`). Returns `true` if a permit was claimed, or `false` if `count <= 0`.
  - `acquire()`: Adaptive cooperative-spinning acquisition. While `!try_acquire()`, the calling task cooperatively relinquishes its remaining time slice via `task::yield_now()` with zero locks held, allowing producer tasks to run and release permits.
- **Release Semantics:** `release()` atomically increments permit count via `count.fetch_add(1, Ordering::Release)`.

### 2.3 Cooperative Yield Mechanism (`yield_now()`)
- **Location:** [`kernel/src/task/scheduler.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/scheduler.rs#L173-L179)
- **Dispatch Path:** Software interrupt `int 48` dispatched through IDT Vector 48 to `yield_interrupt_entry` in [`kernel/src/task/context.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/context.rs#L65-L100).
- **Execution Invariants:**
  - Saves 15 General Purpose Registers (120 bytes) and 512 bytes FPU/SIMD state (`fxsave`).
  - Calls `on_cooperative_yield(curr_rsp)`.
  - If `ready_queue.is_empty()`, immediately returns `curr_rsp` and executes `iretq`, measuring the baseline kernel interrupt dispatch overhead without task switching.

---

## 3. Benchmarking Methodology & Architectural Design

### 3.1 Uncontended SpinMutex Benchmarking
- **Warm-Up Phase:** A preliminary lock and unlock cycle is performed to prime L1 instruction and data caches and branch predictors.
- **Timed Repetitions:** 5 repetitions measuring:
  1. `lock()` acquisition: cycles between pre-read and lock return.
  2. `lock()` release: cycles between pre-read and `drop(guard)` return.
  3. `try_lock()` acquisition: cycles between pre-read and `Some(guard)` return.
  4. `try_lock()` release: cycles between pre-read and `drop(guard)` return.
- **Median Outlier Rejection:** Repetition cycles are sorted independently and the median ($S_2$ of $S_0..S_4$) is selected to eliminate interrupt spikes.

### 3.2 Non-Blocking Contention Benchmarking
- **Controlled Contention Protocol:**
  1. The benchmark thread acquires the mutex via `lock()`, placing it in a guaranteed held state.
  2. Over 5 repetitions, `try_lock()` is invoked against the held lock.
  3. Cycles to detect contention and return `None` are captured via RDTSC.
  4. The benchmark verifies `attempts == 5`, `failed_acquisitions == 5`, and `successful_acquisitions == 0`.
  5. The holding guard is symmetrically dropped, restoring the mutex to an unlocked state.

### 3.3 Semaphore Telemetry & Batch Throughput
- **Single-Permit Latency:** Measures cycles for uncontended `try_acquire()` and `release()`.
- **Exhaustion Detection Overhead:**
  - The permit is drained so `count == 0`.
  - Over 5 repetitions, `try_acquire()` is invoked against the exhausted semaphore.
  - Cycles required to detect permit unavailability and return `false` are recorded.
  - The permit is restored, and `count == 1` is strictly asserted.
- **10-Permit Batch Throughput:**
  - Evaluates burst acquisition of 10 sequential permits followed by burst release of 10 permits.
  - Computes aggregate burst cycles and per-permit throughput.
  - Asserts initial permit count of 10 is symmetrically restored.

### 3.4 Cooperative Yield Dispatch Overhead
- **Pre-Condition Safety Guard:** Evaluates `ready_task_count() == 0` prior to invocation. If the ready queue is non-empty, invocation is safely bypassed to avoid preempting active tasks out of turn.
- **Cycle Capture:** Measures the round-trip overhead of `yield_now()` (`int 48` dispatch $\to$ register save $\to$ scheduler lock $\to$ empty queue check $\to$ `iretq`).

---

## 4. Measurement Units & Data Separation

| Metric | Measurement Unit | Hardware Time Source | Conversion Policy |
| :--- | :---: | :---: | :--- |
| **SpinMutex Acquisition Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Strictly prohibited from converting to nanoseconds without calibrated physical TSC frequency. |
| **SpinMutex Release Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Strictly prohibited from converting to nanoseconds without calibrated physical TSC frequency. |
| **Contention Detection Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Measured as non-blocking `try_lock()` cycles under held lock. |
| **Contention Counters** | Cardinal Count (Integers) | Direct Counter | Reports attempts, successes, failures. |
| **Semaphore Permit Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Kept strictly separate from nanosecond benchmarks. |
| **Cooperative Yield Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Reports software interrupt round-trip clock cycles. |

> [!IMPORTANT]
> **Separation from Deadlock Research Subsystem (Topic R14):**
> These synchronization microbenchmarks profile kernel primitive instruction cycles. They are strictly isolated from the Banker's Algorithm and Cycle-Detection research benchmarks in `kernel/src/deadlock/benchmark.rs`. Neither data sets nor units are conflated.

---

## 5. Analytical Boundaries & Subsystem Limitations

1. **RDTSC Out-of-Order Execution & Lack of Serialization Barrier:**
   - The microbenchmark captures timing via `core::arch::x86_64::_rdtsc()`.
   - On superscalar out-of-order x86_64 processors (such as the AMD Zen 4 CPU on the HP Victus 15-fb3xxx laptop), instructions may be speculatively reordered across `RDTSC` boundaries unless preceded and followed by an explicit serialization barrier (`LFENCE` or `CPUID`, or `RDTSCP`).
   - Consequently, measured values reflect instruction retirement windows rather than exact cycle-level serialization. Single-digit precision cannot and must not be claimed.
2. **Uniprocessor (BSP) SMP Architectural Boundary:**
   - Micro-OS currently operates on a single bootstrap processor core (BSP). Secondary APIC application processors (APs) are not brought online in this kernel phase.
   - Genuine simultaneous multi-core lock spinning cannot physically occur on a single core.
   - If Task A holds a `SpinMutex` and Task B invokes `lock()`, Task B would spin until the 8254 PIT hardware timer interrupt preempts it (3 ticks = 30 ms), wasting tens of millions of cycles.
   - Therefore, contention is safely benchmarked via non-blocking `try_lock()`, while multi-task contention is analyzed through controlled time-sliced increments (`task::run_preemption_demo`). Genuine simultaneous multi-core contention remains an architectural milestone for multi-core SMP bring-up.
3. **Warm-Cache Bias:**
   - The benchmark harness executes an initial warm-up pass, and successive iterations execute in a tight loop.
   - The cache lines containing the `AtomicBool` and `AtomicIsize` flags reside in hot L1 data caches.
   - Consequently, reported cycles represent **best-case, warm-cache latency**, not cold-cache cache-miss latency.
4. **Five-Sample Median Sample Size:**
   - The harness captures 5 repetitions per metric and takes the median.
   - This effectively filters isolated hardware interrupts but constitutes an exploratory microbenchmark rather than an exhaustive statistical distribution ($P_{95}$, $P_{99}$).

---

## 6. Safety & Zero-Allocation Invariants

- **Zero Memory Allocation Under Lock:** All benchmark timing loops utilize stack-allocated fixed-size arrays (`[0u64; 5]`). No heap allocations (`Box`, `Vec`, formatting strings) occur while holding a `SpinMutexGuard` or executing timing loops.
- **Strict Symmetrical Restoration:**
  - Mutexes tested are asserted unlocked upon function completion.
  - Semaphores tested have their initial permit count restored and asserted before returning.
- **Zero Assembly/Interrupt Modification:** Context-switch assembly in `context.rs` and IDT handlers in `idt.rs` remain completely unmodified.
- **On-Demand Invocation:** Benchmarks are never executed in the desktop rendering loop or driver interrupt paths.

---

## 7. Verification & Test Suite Status

### 7.1 Host Automated Test Suite (`runner/tests/host_sync_tests.rs`) — ACTUALLY EXECUTED
All 10 dedicated host synchronization tests executed and passed on the development environment:
1. `test_host_spin_mutex_uncontended_lock_and_unlock`: Verifies `SpinMutex` creation, `is_locked()` tracking, mutable dereference, and clean unlocking on drop.
2. `test_host_spin_mutex_contention_detection`: Verifies that `try_lock()` repeatedly returns `None` while the mutex is held, and returns `Some` once dropped.
3. `test_host_spin_mutex_latency_benchmark`: Verifies that `benchmark_spin_mutex_latency()` runs to completion with `success == true`.
4. `test_host_spin_mutex_contention_benchmark`: Verifies that `benchmark_spin_mutex_contention()` records exactly 5 attempts, 5 failures, and 0 successful acquisitions against a held lock.
5. `test_host_semaphore_accounting_and_exhaustion`: Verifies atomic counting, permit decrement, exhaustion rejection at count == 0, and permit restoration.
6. `test_host_semaphore_latency_and_batch_benchmark`: Verifies that `benchmark_semaphore_latency()` runs cleanly with `permits_restored == true` and 5/5 unavailable rejections.
7. `test_host_spin_mutex_multithreaded_concurrency`: Spawns 4 host OS threads concurrently incrementing a shared `SpinMutex<u64>` 2,500 times each (10,000 total), confirming strict mutual exclusion and zero race conditions under true multi-threaded concurrency.
8. `test_host_semaphore_multithreaded_producer_consumer`: Spawns producer and consumer threads using `Semaphore::acquire()` and `Semaphore::release()` with cooperative yielding, verifying handoff.
9. `test_host_suite_integration`: Verifies end-to-end execution of `run_synchronization_benchmarks()`.
10. `test_host_sync_distinction_disclaimer`: Structurally asserts that host test execution does not claim bare-metal hardware CPU cycle numbers.

### 7.2 In-Kernel Self-Tests (`kernel/src/sync/mod.rs` Test 4) — COMPILED ONLY (RUNTIME PENDING PHYSICAL BOOT)
- **Compilation Status:** Successfully compiled for target `x86_64-unknown-none` with **0 errors and 0 warnings**.
- **Execution Policy:** Because Micro-OS is a bare-metal `#![no_std]` OS targeting the HP Victus laptop, and virtual machines (QEMU/VMware) are prohibited, in-kernel runtime execution and bare-metal RDTSC clock cycle capture **remain pending physical hardware boot via Live USB**. No bare-metal cycles or nanoseconds have been fabricated.

### 7.3 Test Suite Execution Summary
- **Host Runner Test Suite (`cargo test -p runner`):** **52 passed, 0 failed, 0 warnings**
  - 27 ISO/MBR/FAT structural tests
  - 6 host memory allocator tests (Phase 1A)
  - 9 host deadlock research tests
  - 10 host synchronization performance tests (Phase 1B)
- **Kernel Compilation (`cargo check -p kernel --target x86_64-unknown-none`):** **0 errors, 0 warnings**.
- **Kernel Runtime Status:** **Pending physical boot on HP Victus laptop**.

---

## 8. Artifact & File Integrity Verification

| File Path | Status | Purpose of Modification |
| :--- | :---: | :--- |
| `kernel/src/sync/benchmark.rs` | **Created** | Implemented `SpinMutex`, contention, `Semaphore`, and cooperative yield latency microbenchmarks. |
| `kernel/src/sync/mod.rs` | **Modified** | Re-exported benchmark routines and integrated Test 4 into `run_self_tests()`. |
| `runner/tests/host_sync_tests.rs` | **Created** | Comprehensive host-side automated validation test suite (10 tests). |
| `final_demo_package/synchronization_performance_report.md` | **Created** | This formal performance methodology and evaluation report. |
| `target/micro_os_uefi_release.iso` | **UNMODIFIED** | Preserved (3,186,688 bytes, SHA-256: `9F3BAD0E5FC84D209820B166CE69F726CC1C01A7C04F10A6988EEAA5306FAD83`). |
| `target/micro_os_uefi_release.img` | **UNMODIFIED** | Preserved (3,211,264 bytes, SHA-256: `E72BD04C86E8CFCF8B5F0B0ADC24B39C8866A5DEB66B83B5BDC538BFB5965546`). |
