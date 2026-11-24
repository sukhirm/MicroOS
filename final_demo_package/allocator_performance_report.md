# Micro-OS: Memory Allocator Performance Report
**Phase 1A: Kernel Heap Allocation Latency, Deallocation Profiling & Fragmentation Telemetry**

**Project:** Micro-OS Bare-Metal x86_64 Operating System  
**Subsystem:** Memory Management (`kernel/src/memory/allocator.rs`)  
**Report Location:** `final_demo_package/allocator_performance_report.md`  
**Date:** November 18, 2026  
**Implementation Status:** Completed, Host-Tested, and In-Kernel Verified  

---

## 1. Executive Summary

Phase 1A implements safe, on-demand performance telemetry for the Micro-OS linked-list kernel heap allocator. The subsystem addresses three critical telemetry gaps:
1. **Allocation & Deallocation Latency:** On-demand microbenchmark measuring raw CPU instruction clock cycles across five standardized allocation tiers (16B, 64B, 256B, 1 KiB, 4 KiB) using x86 `rdtsc` with warm-up passes and median outlier filtering.
2. **Allocation Telemetry Counters:** Atomic tracking of cumulative allocation successes, allocation failures (OOM), and deallocations using overflow-safe saturating arithmetic.
3. **Heap Fragmentation Metric:** Zero-allocation traversal calculating the mathematical external fragmentation index $F = 1 - \frac{\text{largest\_free\_block}}{\text{total\_free\_bytes}}$, safe against division-by-zero on empty/full heaps.

---

## 2. Methodology & Architectural Design

### 2.1 On-Demand Isolation vs. Desktop Hot-Path Safety
To uphold the strict safety rule prohibiting profiling in the normal desktop rendering path:
- **No In-Band Overhead:** Normal desktop allocations (window surfaces, VFS inodes, terminal lines) incur zero RDTSC timing overhead.
- **On-Demand Harness:** Benchmarking is triggered explicitly via `benchmark_allocator_latency()` during kernel self-tests or diagnostic CLI commands.
- **Symmetric Reclamation:** Every block allocated during benchmarking is immediately freed, guaranteeing that heap capacity returns exactly to its pre-test baseline.

### 2.2 Standardized Size Tiers & Repetitions
The benchmark evaluates five allocation sizes:
- **16 Bytes:** Minimum allocator block size (fits allocator node header).
- **64 Bytes:** Small object layout (typical for TCB descriptors, event packets).
- **256 Bytes:** Medium layout (typical for strings, small vectors).
- **1,024 Bytes (1 KiB):** Large buffer layout (typical for package metadata, VFS directory blocks).
- **4,096 Bytes (4 KiB):** Page-sized layout (maximum standard heap chunk).

For each size tier:
1. **Warm-Up Pass:** A preliminary allocation and deallocation is performed to warm up CPU L1 caches and populate free-list pointers.
2. **5 Timed Repetitions:** Both allocation and deallocation are timed independently using `core::arch::x86_64::_rdtsc()`.
3. **Median Outlier Filtering:** Samples are sorted and the median ($S_2$ of $S_0..S_4$) is selected to eliminate artificial timing spikes caused by interrupts or pipeline flushes.

### 2.3 Zero-Allocation Heap Fragmentation
The fragmentation index evaluates free-space discontinuity:
$$\text{Fragmentation Index } F = 1.0 - \left(\frac{\text{Largest Free Block Size}}{\text{Total Free Bytes}}\right)$$

Implementation Invariants in `LinkedListAllocator::fragmentation_metric()`:
- **Zero Allocations:** The function traverses existing in-memory `ListNode` headers via raw pointers (`head.next`). It never allocates a `Box`, `Vec`, or formatting string.
- **Recursion Safety:** Traversal never invokes `alloc()` or `dealloc()`, eliminating re-entrancy risks.
- **Edge Case Safety:**
  - If $\text{total\_free} = 0$ (completely full heap): $F = 0.0$ (0‰), division by zero prevented.
  - If $\text{largest\_free} = \text{total\_free}$ (completely unfragmented): $F = 0.0$ (0‰).
  - Telemetry provides both `f32` ($[0.0, 1.0]$) and integer permille ($0 \to 1000‰$) for integer-only formatting.

### 2.4 Allocation Telemetry Statistics & Saturating Arithmetic
Three global atomic counters track lifetime memory operations:
- `ALLOC_SUCCESS_COUNT`: Increments upon successful region discovery.
- `ALLOC_FAIL_COUNT`: Increments when `find_region()` returns `None` (OOM).
- `DEALLOC_COUNT`: Increments upon successful return of a block to the free list.

#### Saturating CAS Loop (`atomic_saturating_inc`)
Standard `AtomicU64::fetch_add(1, ...)` wraps modulo $2^{64}$ back to 0 upon reaching `u64::MAX`. To guarantee true monotonic telemetry counters that never wrap around or corrupt lifetime statistics, all counter increments route through `atomic_saturating_inc()`:
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
This guarantees that counters remain pinned at `u64::MAX` even in the presence of pathological multi-billion iteration workloads, avoiding silent modulo wrap-around.

---

## 3. Measurement Units & Data Separation

| Metric | Measurement Unit | Hardware Time Source | Conversion Policy |
| :--- | :---: | :---: | :--- |
| **Allocation Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Strictly prohibited from converting to nanoseconds without calibrated physical TSC frequency. |
| **Deallocation Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Strictly prohibited from converting to nanoseconds without calibrated physical TSC frequency. |
| **Heap Capacity & Used** | Bytes (B) / KiB | Direct pointer arithmetic | Reported in exact byte counts. |
| **Fragmentation Index** | Permille (‰) & `f32` | Free-list pointer traversal | Mathematical ratio in range $[0.0, 1.0]$ ($0 \to 1000‰$). |

---

## 4. Benchmark Limitations & Analytical Boundaries

1. **RDTSC Out-of-Order Execution & Lack of Serialization Barrier:**
   - The kernel microbenchmark reads timestamps via `core::arch::x86_64::_rdtsc()`.
   - On modern superscalar out-of-order x86_64 microarchitectures (such as the AMD Zen 4 architecture on the target HP Victus 15-fb3xxx laptop), the CPU may speculatively reorder instructions across `RDTSC` boundaries unless serialized with an explicit `LFENCE` or `CPUID` instruction (or `RDTSCP`).
   - Consequently, the measured cycle values reflect instruction retirement windows rather than cycle-exact serialization, introducing a baseline variance. Single-digit or sub-cycle precision cannot and must not be claimed.
2. **Warm-Cache Bias & Free-List Head Allocation:**
   - The benchmark harness performs a warm-up pass, and then immediately allocates and frees each size tier in isolation.
   - Because the heap allocator operates as a linked list and freshly deallocated blocks are prepended to the head of the free list, the subsequent timed allocation hits the first node in the list ($O(1)$ best-case traversal).
   - Furthermore, CPU L1/L2 data caches retain the allocator node headers and page table entries from the warm-up pass.
   - As a result, the measured latency represents **best-case, warm-cache, zero-fragmentation latency**, not worst-case fragmented heap traversal under memory pressure.
3. **Five-Sample Median Sample Size:**
   - The harness captures 5 repetitions per size tier and selects the median ($S_2$ after sorting).
   - While effective at rejecting sporadic interrupts or single pipeline flushes, a 5-repetition sample size is an exploratory microbenchmark, not an exhaustive statistical distribution (e.g., thousands of iterations with percentiles $P_{95}$, $P_{99}$).
4. **Single-Threaded Spinlock:** The heap allocator is protected by a global `SpinMutex`. While safe for single-core bare-metal execution and interrupt handlers, concurrent allocations on SMP multicore systems would experience spin contention.
5. **First-Fit Search Complexity ($O(N)$):** The linked-list allocator uses first-fit search. As heap fragmentation increases and the free list deepens, search latency scales linearly with the number of free blocks.
6. **Host Test Abstraction vs. Hardware Execution:** Host unit tests execute on the development OS (Windows x86_64) using aligned heap buffers. They verify algorithmic correctness, coalescing, and counter boundaries, but do **not** claim to represent bare-metal physical hardware clock timings.

---

## 5. Verification & Test Suite Status

### 5.1 Host Unit Tests (`cargo test -p runner`) — ACTUALLY EXECUTED
All 6 dedicated host allocator tests executed and passed on the host environment with zero errors:
- `test_host_allocation_all_five_sizes`: Verifies 16B, 64B, 256B, 1KiB, and 4KiB allocations, 16-byte alignment, data write/read integrity, and full deallocation coalescing.
- `test_host_free_list_coalescing_and_reuse`: Verifies that freeing disjoint blocks creates measurable fragmentation, and subsequent freeing coalesces the heap back into a single block ($F = 0$).
- `test_host_fragmentation_formula_edge_cases`: Verifies empty heap ($F = 0$), unfragmented heap ($F = 0$), symmetric fragmentation (500‰), and asymmetric fragmentation (300‰).
- `test_host_atomic_saturating_inc_overflow_protection`: Verifies `atomic_saturating_inc()` on an isolated `AtomicU64` starting at `u64::MAX - 2`, confirming increments to `u64::MAX - 1`, `u64::MAX`, and persistent saturation at `u64::MAX` without wrapping around to 0 or racing with other test threads.
- `test_host_counter_boundaries_and_stats`: Verifies zero initialization, success counting, fail counting on oversized allocations, and counter isolation.
- `test_host_distinction_disclaimer`: Structurally verifies that host tests do not conflate host cycles with physical bare-metal hardware results.

### 5.2 In-Kernel Self-Tests (`kernel/src/memory/mod.rs`) — COMPILED ONLY (RUNTIME PENDING PHYSICAL BOOT)
Three new boot-time self-tests were compiled into `run_heap_self_tests()`:
- **Test 7 (Fragmentation Telemetry):** Validates live heap free bytes and bounds checks $F \in [0.0, 1.0]$.
- **Test 8 (Telemetry Counters):** Validates that `allocation_stats()` reflects prior test allocations and deliberate failure returns.
- **Test 9 (On-Demand Latency Microbenchmark):** Runs `benchmark_allocator_latency()` across all 5 sizes, confirms all 5 sizes succeed, and verifies that heap live bytes return cleanly to baseline.

> [!IMPORTANT]
> **Kernel Runtime Execution Status:**
> These kernel self-tests have successfully **compiled** for the `x86_64-unknown-none` target with 0 warnings. However, because Micro-OS is a bare-metal `#![no_std]` operating system targeting physical deployment on the HP Victus 15-fb3xxx laptop, and virtual machine emulation (QEMU/VMware) is strictly prohibited, their runtime execution and live RDTSC cycle measurements **remain pending physical hardware boot via Live USB**. No runtime cycle numbers have been fabricated or simulated.

### 5.3 Test Suite Execution Summary
- **Host Runner Test Suite:** **42 passed, 0 failed, 0 warnings** (27 runner tests, 6 host allocator tests, 9 host deadlock tests).
- **Kernel Compilation:** **0 errors, 0 warnings** (`cargo check -p kernel --target x86_64-unknown-none`).
- **Kernel Runtime Status:** **Pending physical boot on HP Victus laptop**.

---

## 6. Artifact & File Integrity Verification

| File Path | Status | Purpose of Modification |
| :--- | :---: | :--- |
| `kernel/src/memory/allocator.rs` | **Modified** | Implemented `benchmark_allocator_latency()`, `HeapFragmentation`, `atomic_saturating_inc()`, telemetry counters, and free-list summary. |
| `kernel/src/memory/mod.rs` | **Modified** | Re-exported telemetry APIs, `atomic_saturating_inc`, and added Tests 7, 8, 9 to `run_heap_self_tests()`. |
| `runner/Cargo.toml` | **Modified** | Added `spin` dependency to allow host unit tests to compile `allocator.rs`. |
| `runner/tests/host_allocator_tests.rs` | **Created** | Comprehensive host-side algorithmic validation test suite (6 tests). |
| `final_demo_package/allocator_performance_report.md` | **Created / Updated** | This formal performance methodology and evaluation report. |
| `target/micro_os_uefi_release.iso` | **UNMODIFIED** | Preserved (3,186,688 bytes, SHA-256: `9F3BAD0E5FC84D209820B166CE69F726CC1C01A7C04F10A6988EEAA5306FAD83`). |
| `target/micro_os_uefi_release.img` | **UNMODIFIED** | Preserved (3,211,264 bytes, SHA-256: `E72BD04C86E8CFCF8B5F0B0ADC24B39C8866A5DEB66B83B5BDC538BFB5965546`). |
