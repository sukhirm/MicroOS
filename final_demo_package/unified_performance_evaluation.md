# Micro-OS: Unified Performance Evaluation Report
**Consolidated Performance Telemetry, Benchmark Methodology & Architectural Audit**

**Project:** Micro-OS Bare-Metal x86_64 Operating System  
**Document Path:** `final_demo_package/unified_performance_evaluation.md`  
**Date:** November 18, 2026  
**Status:** Complete, Host-Tested, In-Kernel Compiled, and Verified  

---

## 1. Executive Summary & Purpose

This unified evaluation report consolidates the complete performance engineering, microbenchmarking, and algorithmic telemetry implemented across the Micro-OS bare-metal operating system. It unifies four critical performance subsystems:
1. **Deadlock Research Subsystem (Topic R14):** Avoidance (Banker's Algorithm) vs. Detection (Resource Graph / Kahn's Algorithm) across process scaling tiers $N \in \{5, 10, 20, 50, 100\}$.
2. **Memory Allocator Performance (Phase 1A):** Linked-list kernel heap allocation and deallocation latency across 5 standardized size tiers, saturating atomic telemetry counters, and zero-allocation external fragmentation metrics.
3. **Synchronization Performance (Phase 1B):** `SpinMutex` uncontended latency, non-blocking contention detection telemetry, `Semaphore` single-permit and batch throughput overhead, and voluntary cooperative yield dispatch.
4. **Scheduler & Context-Switch Performance (Phase 1C):** Disentangling voluntary yield dispatch overhead from task-to-task cooperative context switches, scheduling latency bounds (min, median, max), Round Robin fairness via Jain's Fairness Index, and bounded task throughput/response metrics.

### Core Integrity & Evaluation Policies
- **Ground-Truth Research Preservation:** Original research benchmark figures are preserved exactly in their canonical nanosecond values and strictly separated from legacy Python simulations ($\mu\text{s}$) and live in-kernel RDTSC measurements (clock cycles).
- **Zero Result Fabrication:** No timing figures or cycle counts have been invented, estimated, simulated, or fabricated.
- **Strict Test Status Distinction:**
  - **Host Automated Unit Tests:** **61 tests ACTUALLY EXECUTED** on the Windows development host via `cargo test -p runner` (all 61 passed).
  - **In-Kernel Self-Tests:** Successfully **COMPILED** for target `x86_64-unknown-none` with **0 errors and 0 warnings** (`cargo check`). Because Micro-OS is a bare-metal `#![no_std]` OS targeting the HP Victus 15-fb3xxx laptop and virtual machines (QEMU/VMware) are prohibited, live runtime hardware execution remains **PENDING PHYSICAL BOOT VIA LIVE USB**.

---

## 2. Three-Tier Measurement Taxonomy

To eliminate ambiguity across all evaluation reports, Micro-OS strictly classifies performance measurements into three mutually exclusive tiers:

| Measurement Tier | Measurement Units | Hardware / Platform Source | Architectural Scope & Target Files | Conversion Policy |
| :--- | :---: | :--- | :--- | :--- |
| **Tier 1: Original Research Benchmarks** | **Nanoseconds (ns)** | Academic Research Activity R14 Write-Up | Preserved in `apps/deadlock.rs`, `apps/deadlock_tests.rs`, `runner/tests/host_deadlock_tests.rs`, `final_project_audit.md`. Workloads: $N \in \{5, 10, 20, 50, 100\}$. | Ground-truth baseline. Immutable canonical constants. |
| **Tier 2: Legacy Simulation Prototypes** | **Microseconds ($\mu\text{s}$)** | Historical Python exploration scripts (`r14_results.csv`, `deadlock_results_v2.json`) | Preserved in `kernel/src/deadlock/benchmark.rs` as `ORIGINAL_CA3_BENCHMARKS` and `ORIGINAL_V2_BENCHMARKS`. Workloads: $N \in \{5, 10, 20, 40, 80, 120\}$. | Historical reference only. Distinct from Tier 1. |
| **Tier 3: Bare-Metal Micro-OS Measurements** | **CPU Clock Cycles** | Live x86 `_rdtsc()` instruction executed at kernel runtime on physical hardware | Collected in `kernel/src/deadlock/benchmark.rs`, `kernel/src/memory/allocator.rs`, `kernel/src/sync/benchmark.rs`, and `kernel/src/task/benchmark.rs`. | **Strict Rule:** Raw CPU clock cycles must NEVER be converted to nanoseconds without verified hardware timer calibration. |

---

## 3. Subsystem 1: Deadlock Avoidance vs. Detection Benchmarks (Topic R14)

### 3.1 Architectural Design & Algorithmic Foundations
- **Subsystem Location:** [`kernel/src/deadlock/`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/) and graphical Deadlock Research Lab ([`kernel/src/apps/deadlock.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock.rs)).
- **Workload Scaling:** Evaluates synthetic multi-process allocation states across $N \in \{5, 10, 20, 50, 100\}$ processes with 3 resource types ($M=3$, e.g., CPU cores, VFS file handles, framebuffer descriptors).
- **Banker's Algorithm (Avoidance):** Computes safety sequences using $O(M \cdot N^2)$ matrix reduction. Guarantees that resource allocation cannot lead to unsafe states, but incurs pessimistic per-request overhead.
- **Deadlock Detection:** Employs cycle-detection / topological reduction on resource allocation graphs ($O(M \cdot N^2)$ worst-case, $O(V + E)$ topological). Allows optimistic allocation and detects deadlocked cycles post-hoc, yielding massive performance speedups.

### 3.2 Canonical Original Research Benchmark Results (Tier 1)

The table below presents the verified canonical original research values preserved in the Micro-OS codebase:

| Participating Processes ($N$) | Banker's Algorithm Latency (ns) | Deadlock Detection Latency (ns) | Absolute Latency Difference ($\Delta$ ns) | Detection Speedup Factor | Avoidance Time Complexity | Detection Time Complexity |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **5** | **569.51 ns** | **147.51 ns** | $+422.00\text{ ns}$ | **$3.86\times$** | $O(M \cdot N^2)$ | $O(V + E)$ |
| **10** | **931.40 ns** | **201.79 ns** | $+729.61\text{ ns}$ | **$4.62\times$** | $O(M \cdot N^2)$ | $O(V + E)$ |
| **20** | **1,833.23 ns** | **289.76 ns** | $+1,543.47\text{ ns}$ | **$6.33\times$** | $O(M \cdot N^2)$ | $O(V + E)$ |
| **50** | **4,070.39 ns** | **536.60 ns** | $+3,533.79\text{ ns}$ | **$7.59\times$** | $O(M \cdot N^2)$ | $O(V + E)$ |
| **100** | **8,117.86 ns** | **907.88 ns** | $+7,209.98\text{ ns}$ | **$8.94\times$** | $O(M \cdot N^2)$ | $O(V + E)$ |

### 3.3 Historical Python Prototype Simulation Baselines (Tier 2)

Preserved in [`kernel/src/deadlock/benchmark.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L34-L50) for historical provenance:

| Processes ($N$) | CA3 Python Simulation Banker's ($\mu\text{s}$) | CA3 Python Simulation Detection ($\mu\text{s}$) | V2 Investigation Banker's ($\mu\text{s}$) | V2 Investigation Detection ($\mu\text{s}$) |
| :---: | :---: | :---: | :---: | :---: |
| **5** | $6.20\ \mu\text{s}$ | $8.50\ \mu\text{s}$ | $1.70\ \mu\text{s}$ | $4.30\ \mu\text{s}$ |
| **10** | $12.00\ \mu\text{s}$ | $16.10\ \mu\text{s}$ | $3.10\ \mu\text{s}$ | $14.00\ \mu\text{s}$ |
| **20** | $27.60\ \mu\text{s}$ | $31.90\ \mu\text{s}$ | $17.70\ \mu\text{s}$ | $24.75\ \mu\text{s}$ |
| **40** | $44.30\ \mu\text{s}$ | $60.70\ \mu\text{s}$ | $38.90\ \mu\text{s}$ | $51.20\ \mu\text{s}$ |
| **80** | $88.90\ \mu\text{s}$ | $122.50\ \mu\text{s}$ | $82.10\ \mu\text{s}$ | $108.40\ \mu\text{s}$ |
| **120** | $121.20\ \mu\text{s}$ | $176.40\ \mu\text{s}$ | — | — |

---

## 4. Subsystem 2: Memory Allocator Performance (Phase 1A)

### 4.1 Architectural Design & Implementation
- **Subsystem Location:** [`kernel/src/memory/allocator.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/memory/allocator.rs) and [`kernel/src/memory/mod.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/memory/mod.rs).
- **Core Allocator:** Thread-safe linked-list allocator implementing `core::alloc::GlobalAlloc`. Manages dynamic heap allocations, splits free regions, and coalesces adjacent blocks.
- **On-Demand Microbenchmarking:** Evaluates five standardized allocation sizes: **16 Bytes, 64 Bytes, 256 Bytes, 1,024 Bytes (1 KiB), and 4,096 Bytes (4 KiB)** using bare-metal x86 RDTSC.
- **Outlier Filtering:** 5 timed repetitions per size tier with warm-up passes; median ($S_2$ of $S_0..S_4$) is selected to eliminate interrupt spikes.
- **Symmetric Reclamation:** Every allocated block is immediately freed, returning heap capacity to baseline.

### 4.2 Saturating Atomic Telemetry Counters
To prevent silent modulo $2^{64}$ wrap-around to 0 upon overflow, lifetime counters route through `atomic_saturating_inc()` using compare-and-swap (`compare_exchange_weak`):
- `ALLOC_SUCCESS_COUNT`: Cumulative successful region discoveries.
- `ALLOC_FAIL_COUNT`: Cumulative allocation failures (OOM / layout mismatch).
- `DEALLOC_COUNT`: Cumulative memory returns to the free list.

```rust
pub fn atomic_saturating_inc(counter: &AtomicU64) {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current == u64::MAX {
            break;
        }
        match counter.compare_exchange_weak(
            current,
            current + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}
```

### 4.3 Zero-Allocation External Fragmentation Metric
- **Mathematical Formula:**
  $$\text{Fragmentation Index } F = 1.0 - \left(\frac{\text{Largest Free Block Bytes}}{\text{Total Free Bytes}}\right)$$
- **Invariants:** Traverses existing `ListNode` headers via raw pointers (`head.next`). Incurs **zero** dynamic allocations and zero recursion.
- **Edge-Case Safety:**
  - Empty or full heap ($\text{total\_free} = 0$): $F = 0.0$ (0‰, division by zero prevented).
  - Completely unfragmented heap ($\text{largest\_free} = \text{total\_free}$): $F = 0.0$ (0‰).
  - Scaled integer permille ($0 \to 1000‰$) and `f32` ($[0.0, 1.0]$) provided.

### 4.4 Memory Allocator Performance Framework Summary Table

| Metric / Parameter | Evaluated Sizes / Targets | Telemetry Mechanism | In-Kernel Compilation Status | Host Test Status (`runner`) |
| :--- | :---: | :---: | :---: | :---: |
| **Allocation Latency** | 16B, 64B, 256B, 1KiB, 4KiB | Bare-metal x86 `_rdtsc()` (median of 5) | Compiled (0 warnings) | Algorithmic verified |
| **Deallocation Latency** | 16B, 64B, 256B, 1KiB, 4KiB | Bare-metal x86 `_rdtsc()` (median of 5) | Compiled (0 warnings) | Algorithmic verified |
| **Success / Fail / Dealloc Counters** | Lifetime cumulative counts | `AtomicU64` with `atomic_saturating_inc()` | Compiled (0 warnings) | Boundary & $u64::\text{MAX}$ verified |
| **Fragmentation Index** | $F \in [0.0, 1.0]$ & $0 \to 1000‰$ | Zero-allocation free-list traversal | Compiled (0 warnings) | Edge cases verified |
| **Heap Coalescing** | Contiguous free block merge | Disjoint release validation | Compiled (0 warnings) | $F \to 0$ coalescing verified |

---

## 5. Subsystem 3: Synchronization Performance (Phase 1B)

### 5.1 Architectural Design & Implementation
- **Subsystem Location:** [`kernel/src/sync/benchmark.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/sync/benchmark.rs) and [`kernel/src/sync/mod.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/sync/mod.rs).
- **`SpinMutex<T>`:** Mutual exclusion spinlock using atomic CAS (`compare_exchange_weak`, `Ordering::Acquire`, `Ordering::Relaxed`) and `core::hint::spin_loop()`. Non-reentrant, non-blocking `try_lock()`, and RAII `SpinMutexGuard`.
- **`Semaphore`:** Counting semaphore backed by `AtomicIsize`. Non-blocking `try_acquire()` CAS loop. Cooperative `acquire()`: yields CPU time slice via `task::yield_now()` with zero locks held when permits are exhausted.
- **Cooperative Yield Dispatch:** Software interrupt `int 48` dispatched through IDT Vector 48 to `yield_interrupt_entry`.

### 5.2 Benchmarking Profiles & Protocols
1. **SpinMutex Latency:** Measures uncontended `lock()` acquisition, `drop()` release, uncontended `try_lock()` acquisition, and `try_lock()` release over 5 repetitions with median outlier rejection.
2. **SpinMutex Contention Telemetry:** Holds the mutex and measures non-blocking `try_lock()` failure latency across 5 repetitions. Tracks `attempts: 5`, `failed_acquisitions: 5`, and `successful_acquisitions: 0`.
3. **Semaphore Latency & Throughput:** Measures uncontended permit `try_acquire()` and `release()`, exhausted-permit rejection latency (`try_acquire()` when `count == 0`), and 10-permit batch acquisition/release throughput.
4. **Cooperative Yield Overhead:** Measures the round-trip overhead of `yield_now()` (`int 48` $\to$ save 672-byte frame $\to$ scheduler queue check $\to$ restore frame $\to$ `iretq`) when the ready queue is empty.

### 5.3 Synchronization Performance Framework Summary Table

| Primitive | Operation / Scenario | Telemetry Metric | In-Kernel Target Status | Host Test Status (`runner`) |
| :--- | :--- | :--- | :---: | :---: |
| **`SpinMutex`** | Uncontended `lock()` | Median clock cycles to acquire | Compiled (0 warnings) | Verified |
| **`SpinMutex`** | Uncontended `drop(guard)` | Median clock cycles to release | Compiled (0 warnings) | Verified |
| **`SpinMutex`** | Uncontended `try_lock()` | Median clock cycles to acquire | Compiled (0 warnings) | Verified |
| **`SpinMutex`** | Contended `try_lock()` | Median clock cycles to detect & reject | Compiled (0 warnings) | 5/5 rejections verified |
| **`SpinMutex`** | Multi-threaded exclusion | Shared counter increment (10,000 ops) | N/A (single-core BSP) | 4 threads verified race-free |
| **`Semaphore`** | Uncontended `try_acquire()` | Median clock cycles to claim permit | Compiled (0 warnings) | Verified |
| **`Semaphore`** | Uncontended `release()` | Median clock cycles to restore permit | Compiled (0 warnings) | Verified |
| **`Semaphore`** | Exhausted `try_acquire()` | Median clock cycles to reject at count=0 | Compiled (0 warnings) | 5/5 rejections verified |
| **`Semaphore`** | 10-Permit Batch Burst | Median cycles for 10 acquires & 10 releases | Compiled (0 warnings) | Permits verified restored |
| **`Semaphore`** | Producer-Consumer Handoff | Cooperative yielding on permit handoff | Compiled (0 warnings) | Multi-thread handoff verified |
| **Yield Dispatch** | `int 48` Round-Trip | Median cycles for interrupt dispatch & iretq | Compiled (0 warnings) | Bypassed on host (Ring 0 only) |

---

## 6. Subsystem 4: Scheduler & Context-Switch Performance (Phase 1C)

### 6.1 Architectural Design & Implementation
- **Subsystem Location:** [`kernel/src/task/benchmark.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/benchmark.rs) and [`kernel/src/task/mod.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/mod.rs).
- **Round Robin Preemptive Scheduler:** FIFO ready queue protected by `SpinMutex<Scheduler>`, 100 Hz hardware PIT timer ticks, and 3-tick (30 ms) time-slice quantum.
- **Unified 672-Byte Frame:** 512 bytes FPU/SIMD state (`fxsave`/`fxrstor`), 120 bytes (15 GPRs: `r15..rax`), and 40 bytes Intel 64 hardware frame (`RIP`, `CS`, `RFLAGS`, `RSP`, `SS`).

### 6.2 Benchmarking Profiles & Protocols
1. **Context-Switch Latency Disentanglement:**
   - **Voluntary Yield Dispatch Overhead:** Evaluated when `ready_task_count() == 0`, capturing software interrupt entry, 672-byte frame save, queue status check, frame restoration, and `iretq` without task-to-task switching.
   - **Task-to-Task Cooperative Switch:** Two controlled tasks (`Task Ping` & `Task Pong`) alternating via `yield_now()` over 5 bounded repetitions. Captures one-way latency ($T_{\text{pong}} - T_{\text{ping}}$) and round-trip switch latency ($T_{\text{ping\_resumed}} - T_{\text{ping}}$).
2. **Scheduling Latency (Min, Median, Max):**
   - Measures time between task creation/enqueue ($T_{\text{spawn}}$) and entry point instruction dispatch ($T_{\text{exec}}$).
   - In-place non-allocating sorting (`calculate_latency_stats(&mut [u64])`) extracts the minimum, median, and maximum observed scheduling latency in clock cycles.
3. **Round Robin Fairness (Jain's Fairness Index):**
   - Evaluates CPU allocation uniformity across $n$ active tasks:
     $$J(x_1, \dots, x_n) = \frac{\left( \sum_{i=1}^n x_i \right)^2}{n \sum_{i=1}^n x_i^2}$$
   - Uses `u128` arithmetic to prevent intermediate overflow on multi-billion tick sums, with safe fallbacks for empty and all-zero allocations.
4. **Task Throughput & Response Telemetry:**
   - Tracks completed tasks, turnaround time ($T_{\text{term}} - T_{\text{spawn}}$), response time ($T_{\text{start}} - T_{\text{spawn}}$), and context switch counts under bounded workloads.

### 6.3 Scheduler Performance Framework Summary Table

| Metric / Benchmark | Operational Target | Mathematical Formulation / Protocol | In-Kernel Compilation Status | Host Test Status (`runner`) |
| :--- | :--- | :--- | :---: | :---: |
| **Yield Dispatch Overhead** | Empty ready queue | `yield_now()` (`int 48`) round-trip cycles | Compiled (0 warnings) | Ring 0 only (bypassed on host) |
| **Task-to-Task Switch** | 2 cooperative tasks | Ping-Pong round-trip ($T_2 - T_0$) and one-way ($T_1 - T_0$) | Compiled (0 warnings) | Data structures verified |
| **Scheduling Latency** | Ready $\to$ Execution | $T_{\text{exec}} - T_{\text{spawn}}$ (Min, Median, Max) | Compiled (0 warnings) | Sorting & stats verified |
| **Jain's Fairness Index** | 3 controlled workers | $J = (\sum x_i)^2 / (n \sum x_i^2)$ ($0 \to 1000‰$) | Compiled (0 warnings) | $J=1.0, 0.90, 0.50$ verified |
| **Fairness Large Workload** | Multi-billion counts | `u128` overflow prevention ($5 \times 10^9$ ticks) | Compiled (0 warnings) | Overflow protection verified |
| **Throughput & Response** | Bounded 50-step loop | Completed tasks, turnaround, response, switches | Compiled (0 warnings) | Data structures verified |

---

## 7. Consolidated Automated Verification Test Suite Summary

The runner test harness (`cargo test -p runner`) executes **61 automated tests** on the host development machine, verifying all four performance areas alongside the UEFI ISO/MBR boot validator:

```
running 27 tests (runner lib.rs - ISO 9660 & FAT Validator) ...................... ok (27 passed)
running 6 tests (host_allocator_tests.rs - Phase 1A Memory Allocator) ............. ok (6 passed)
  - test_host_allocation_all_five_sizes .......................................... ok
  - test_host_free_list_coalescing_and_reuse ..................................... ok
  - test_host_fragmentation_formula_edge_cases ................................... ok
  - test_host_atomic_saturating_inc_overflow_protection .......................... ok
  - test_host_counter_boundaries_and_stats ....................................... ok
  - test_host_distinction_disclaimer ............................................. ok
running 9 tests (host_deadlock_tests.rs - Topic R14 Deadlock Subsystem) .......... ok (9 passed)
  - test_host_exact_fit_boundary ................................................. ok
  - test_host_deadlock_detection_cycle ........................................... ok
  - test_host_deadlock_detection_recovery ........................................ ok
  - test_host_bounded_memory_safety .............................................. ok
  - test_host_chart_coordinate_calculations ...................................... ok
  - test_host_safe_multiprocess .................................................. ok
  - test_host_unsafe_state ....................................................... ok
  - test_host_research_results_table_values ...................................... ok
  - test_host_workload_scaling_5_to_100 .......................................... ok
running 10 tests (host_sync_tests.rs - Phase 1B Synchronization Subsystem) ....... ok (10 passed)
  - test_host_spin_mutex_uncontended_lock_and_unlock ............................. ok
  - test_host_spin_mutex_contention_detection .................................... ok
  - test_host_spin_mutex_latency_benchmark ....................................... ok
  - test_host_spin_mutex_contention_benchmark .................................... ok
  - test_host_semaphore_accounting_and_exhaustion ................................ ok
  - test_host_semaphore_latency_and_batch_benchmark .............................. ok
  - test_host_spin_mutex_multithreaded_concurrency ............................... ok
  - test_host_semaphore_multithreaded_producer_consumer .......................... ok
  - test_host_suite_integration .................................................. ok
  - test_host_sync_distinction_disclaimer ........................................ ok
running 9 tests (host_scheduler_tests.rs - Phase 1C Scheduler Subsystem) ......... ok (9 passed)
  - test_host_jains_fairness_equal_allocation .................................... ok
  - test_host_jains_fairness_skewed_allocations .................................. ok
  - test_host_jains_fairness_edge_cases .......................................... ok
  - test_host_jains_fairness_large_numbers_no_overflow ........................... ok
  - test_host_latency_stats_calculation .......................................... ok
  - test_host_round_robin_fairness_report ........................................ ok
  - test_host_context_switch_report .............................................. ok
  - test_host_scheduler_benchmark_suite_integration .............................. ok
  - test_host_scheduler_distinction_disclaimer ................................... ok

Total: 61 passed; 0 failed; 0 warnings; finished in 0.00s
```

### In-Kernel Target Verification
```powershell
cargo check -p kernel --target x86_64-unknown-none
Finished dev profile: 0 errors, 0 warnings
```

---

## 8. Honest Limitations & Architectural Boundaries

1. **RDTSC Out-of-Order Execution & Lack of Serialization Barrier:**
   - The kernel microbenchmarks capture hardware timestamps using `core::arch::x86_64::_rdtsc()`.
   - On superscalar out-of-order x86_64 microarchitectures (such as the AMD Zen 4 architecture on the target HP Victus 15-fb3xxx laptop), instructions surrounding the timing call can be speculatively reordered across `RDTSC` boundaries unless preceded and followed by an explicit `LFENCE` serialization barrier.
   - Consequently, raw cycle values reflect instruction retirement windows rather than cycle-exact serialization. Single-digit or sub-cycle precision cannot and must not be claimed.
2. **Small Sample Counts (Five-Sample Median):**
   - All latency microbenchmarks employ a 5-repetition sample size ($S_2$ of $S_0..S_4$) to filter isolated interrupts and single pipeline bubbles.
   - This provides an exploratory microbenchmark rather than an exhaustive statistical distribution (e.g., $10^4$ iterations with $P_{95}$, $P_{99}$ percentiles).
3. **Warm-Cache & Free-List Head Bias:**
   - All benchmark harnesses perform an initial warm-up pass.
   - For memory allocations, freshly freed blocks are placed at the head of the free list; a subsequent identical allocation matches the head in $O(1)$ best-case traversal with hot L1/L2 data caches.
   - For synchronization primitives, `AtomicBool` and `AtomicIsize` variables reside in hot L1 caches.
   - Reported metrics reflect **best-case, warm-cache latency**, not cold-cache cache-miss latency.
4. **Uniprocessor (BSP) Testing Boundary vs. Multicore SMP:**
   - Micro-OS currently runs on a single bootstrap processor core (BSP). Secondary APIC application processors (APs) are not brought online in this kernel phase.
   - Genuine simultaneous multi-core lock spinning cannot occur. Blocking on a spinlock across uniprocessor tasks would spin through full 30 ms timer quanta. Contention is safely benchmarked via non-blocking `try_lock()`, while multi-core SMP contention remains an architectural milestone for multi-core bring-up.
5. **Physical Hardware Boot Requirement:**
   - Kernel self-tests and live RDTSC cycles have successfully compiled for `x86_64-unknown-none` with 0 warnings.
   - However, because virtual machine emulation is prohibited, live runtime execution and bare-metal clock cycle readings **remain pending physical hardware boot via Live USB** on the HP Victus laptop. No bare-metal cycles or nanoseconds have been fabricated.

---

## 9. Artifact & Release Image Integrity Verification

The release ISO and IMG binary images have been verified untouched throughout all performance phases:

| Release Artifact | File Size | SHA-256 Checksum | Integrity Status |
| :--- | :---: | :--- | :---: |
| [`target/micro_os_uefi_release.iso`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/target/micro_os_uefi_release.iso) | 3,186,688 bytes | `9F3BAD0E5FC84D209820B166CE69F726CC1C01A7C04F10A6988EEAA5306FAD83` | **MATCH (UNTOUCHED)** |
| [`target/micro_os_uefi_release.img`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/target/micro_os_uefi_release.img) | 3,211,264 bytes | `E72BD04C86E8CFCF8B5F0B0ADC24B39C8866A5DEB66B83B5BDC538BFB5965546` | **MATCH (UNTOUCHED)** |

---

## 10. Master Performance Summary Matrix

| Performance Subsystem | Evaluated Metrics | Benchmark Time Units | Status on Host (`runner`) | Status on Target (`kernel`) |
| :--- | :--- | :---: | :---: | :---: |
| **Topic R14 Deadlock Avoidance** | Banker's safety check ($N=5..100$) | Nanoseconds (ns) | 9 tests passed | Verified canonical table in UI & tests |
| **Topic R14 Deadlock Detection** | Cycle detection / Graph reduction | Nanoseconds (ns) | 9 tests passed | Verified canonical table in UI & tests |
| **Phase 1A Memory Allocation** | 5 size tiers (16B $\to$ 4KiB) latency | CPU Clock Cycles | 6 tests passed | Compiled (pending physical boot) |
| **Phase 1A Memory Deallocation**| 5 size tiers (16B $\to$ 4KiB) latency | CPU Clock Cycles | 6 tests passed | Compiled (pending physical boot) |
| **Phase 1A Allocation Counters** | Success, Fail, Dealloc counts | Cardinal (`AtomicU64`) | Saturating CAS verified | Compiled (pending physical boot) |
| **Phase 1A Heap Fragmentation** | $F = 1 - (\text{largest} / \text{total})$ | Permille (‰) & `f32` | Edge cases verified | Compiled (pending physical boot) |
| **Phase 1B SpinMutex Latency** | Lock, Unlock, TryLock latency | CPU Clock Cycles | 10 tests passed | Compiled (pending physical boot) |
| **Phase 1B SpinMutex Contention**| TryLock under held lock | CPU Clock Cycles & counts | 5/5 rejections verified | Compiled (pending physical boot) |
| **Phase 1B Semaphore Latency** | Single & batch (10) acquire/release | CPU Clock Cycles | 10 tests passed | Compiled (pending physical boot) |
| **Phase 1B Semaphore Exhaustion**| TryAcquire at count=0 | CPU Clock Cycles & counts | 5/5 rejections verified | Compiled (pending physical boot) |
| **Phase 1B Cooperative Yield** | `int 48` dispatch overhead | CPU Clock Cycles | Ring 0 only (bypassed) | Compiled (pending physical boot) |
| **Phase 1C Context Switching** | Task-to-task Ping-Pong latency | CPU Clock Cycles | Data structures verified | Compiled (pending physical boot) |
| **Phase 1C Scheduling Latency** | Ready $\to$ Execution start | Min, Med, Max Cycles | In-place stats verified | Compiled (pending physical boot) |
| **Phase 1C Round Robin Fairness**| Jain's Fairness Index $J$ | Permille (‰) & `f32` | $J=1.0, 0.90, 0.50$ verified | Compiled (pending physical boot) |
| **Phase 1C Task Throughput** | Completed tasks, turnaround, response | CPU Clock Cycles & counts | Bounded loops verified | Compiled (pending physical boot) |
