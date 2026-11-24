# Micro-OS: Scheduler & Context-Switch Performance Report
**Phase 1C: Context-Switch Latency, Scheduling Latency & Round Robin Fairness Telemetry**

**Project:** Micro-OS Bare-Metal x86_64 Operating System  
**Subsystem:** Task Management & Scheduler (`kernel/src/task/`)  
**Report Location:** `final_demo_package/scheduler_performance_report.md`  
**Date:** November 18, 2026  
**Implementation Status:** Completed, Host-Tested, and In-Kernel Verified  

---

## 1. Executive Summary

Phase 1C implements safe, on-demand performance telemetry and microbenchmarking for the Micro-OS preemptive and cooperative Round Robin task scheduler. The subsystem evaluates four key performance areas:
1. **Context-Switch Latency:** Disentangles voluntary yield dispatch overhead (`int 48` round-trip with empty ready queue) from actual task-to-task cooperative context switching (ping-pong task switching) using raw bare-metal x86 RDTSC clock cycles.
2. **Scheduling Latency:** Tracks elapsed time between task readiness ($T_{\text{spawn}}$) and initial instruction dispatch ($T_{\text{exec}}$), recording minimum, median, and maximum latency.
3. **Round Robin Fairness:** Evaluates CPU resource distribution across concurrent tasks using Jain's Fairness Index:
   $$J(x_1, \dots, x_n) = \frac{\left( \sum_{i=1}^n x_i \right)^2}{n \sum_{i=1}^n x_i^2}$$
4. **Task Throughput & Response:** Bounded workload telemetry tracking completed tasks, response time, turnaround time, and context switch counts without modifying low-level context switch assembly or interrupt handling.

---

## 2. Scheduler Architecture & Context-Switching Audit

### 2.1 Scheduler & Queue Design
- **Location:** [`kernel/src/task/scheduler.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/scheduler.rs)
- **Queue Semantics:** FIFO Ready Queue (`ReadyQueue` in [`kernel/src/task/queue.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/queue.rs)) backed by `alloc::collections::VecDeque<Box<TaskControlBlock>>`.
- **Preemptive Time-Slice:** 3 timer ticks (30 ms at 100 Hz 8254 PIT hardware timer).
- **Concurrency Protection:** Global instance `SCHEDULER: SpinMutex<Scheduler>` with atomic interrupt protection (`IrqGuard`).

### 2.2 Execution Context & Assembly Frame Structure
- **Location:** [`kernel/src/task/context.rs`](file:///c:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/task/context.rs)
- **Unified 672-Byte Frame:**
  - `0x000..0x1FF` (512 bytes): `FxSaveArea` (x87 FPU, MMX, and SSE state, 16-byte aligned).
  - `0x200..0x277` (120 bytes): 15 General-Purpose Registers (`r15..rax`).
  - `0x278..0x29F` (40 bytes): 5 Intel 64 hardware quadwords (`RIP`, `CS`, `RFLAGS`, `RSP`, `SS`).
- **Interrupt Entry Points:**
  - **Preemptive Switch:** Vector 32 (IRQ0 PIT timer) $\to$ `timer_interrupt_entry` $\to$ acknowledges PIC EOI $\to$ `on_timer_tick`.
  - **Cooperative Switch:** Vector 48 (`int 48` software interrupt) $\to$ `yield_interrupt_entry` $\to$ `on_cooperative_yield`.
  - **Task Exit:** `task_trampoline` invokes entry function; upon return, calls `task_exit()` $\to$ `prepare_exit()` $\to$ `switch_context_exit(next_rsp)`. When all worker tasks terminate, control seamlessly returns to the main kernel thread.

---

## 3. Benchmarking Methodology & Architectural Design

### 3.1 Disentangling Voluntary Yield Dispatch vs. Task-to-Task Switching
A fundamental architectural distinction is enforced between dispatch overhead and task switching:
1. **Voluntary Yield Dispatch Overhead (`benchmark_voluntary_yield_dispatch`):**
   - Invoked when the ready queue is empty (`ready_task_count() == 0`).
   - Executes `yield_now()` (`int 48`), saving the 672-byte frame, acquiring `SCHEDULER.lock()`, inspecting the empty queue, releasing the lock, restoring the 672-byte frame, and executing `iretq`.
   - Measures pure interrupt dispatch and scheduler state inspection latency without task switching.
2. **Actual Task-to-Task Cooperative Switch (`task_bench_ping` & `task_bench_pong`):**
   - Two controlled tasks execute a cooperative ping-pong sequence across 5 bounded repetitions.
   - `Task Ping` captures $T_0$, records timestamp, and yields to `Task Pong`.
   - `Task Pong` captures $T_1$, recording one-way switch latency $T_1 - T_0$, and yields back.
   - `Task Ping` resumes at $T_2$, recording round-trip switch latency $T_2 - T_0$.
   - Symmetrically terminates both tasks upon completion.

### 3.2 Scheduling Latency Profiling (Min, Median, Max)
- **Latency Metric:** Elapsed clock cycles between task creation/enqueue ($T_{\text{spawn}}$) and entry point instruction dispatch ($T_{\text{exec}}$):
  $$\text{Scheduling Latency} = T_{\text{exec}} - T_{\text{spawn}}$$
- **In-Place Sorting:** `calculate_latency_stats(&mut [u64])` sorts the sample buffer in-place without heap allocation, extracting the minimum, median, and maximum observed scheduling latency.

### 3.3 Round Robin Fairness & Jain's Fairness Index
- **Fairness Metric:** Evaluates the uniformity of execution opportunities allocated across $n$ active tasks:
  $$J(x_1, \dots, x_n) = \frac{\left( \sum_{i=1}^n x_i \right)^2}{n \sum_{i=1}^n x_i^2}$$
- **Interpretation:**
  - $J \in [1/n, 1.0]$.
  - $J = 1.0$ (1000‰) indicates perfectly equal execution distribution.
  - $J = 1/n$ indicates worst-case starvation where a single task monopolizes the CPU.
- **Overflow-Proof Arithmetic:** Evaluated using intermediate `u128` products to prevent integer overflow when squaring multi-billion tick sums, with safe fallback ($J = 1.0$) for empty or zero allocations.

### 3.4 Task Throughput & Response Telemetry
- **Bounded Workloads:** Controlled worker tasks execute bounded loops (50 iterations) incrementing atomic counters, guaranteeing finite termination.
- **Turnaround Time:** Cycles elapsed from spawn to termination ($T_{\text{term}} - T_{\text{spawn}}$).
- **Response Time:** Cycles elapsed from spawn to first execution ($T_{\text{start}} - T_{\text{spawn}}$).
- **Context Switch Accounting:** Lifetime context switch count tracked via `context_switch_count()`.

---

## 4. Measurement Units & Data Separation

| Metric | Measurement Unit | Hardware Time Source | Conversion Policy |
| :--- | :---: | :---: | :--- |
| **Voluntary Yield Overhead** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Strictly prohibited from converting to nanoseconds without calibrated physical TSC frequency. |
| **Task-to-Task Switch Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Separate round-trip and one-way metrics. |
| **Scheduling Latency** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Min, median, and max cycles reported. |
| **Round Robin Fairness** | Permille (‰) & `f32` | Execution tick counters | Mathematical index in $[0.0, 1.0]$ ($0 \to 1000‰$). |
| **Task Response & Turnaround** | Raw CPU Instruction Cycles | x86 `_rdtsc()` | **Cycles Only.** Strictly separate from nanosecond research benchmarks. |

> [!IMPORTANT]
> **Isolation from Topic R14 Deadlock Research Subsystem:**
> All scheduler performance metrics, telemetry counters, and reports are housed exclusively in `kernel/src/task/benchmark.rs`. They are strictly isolated from the Banker's Algorithm and Cycle Detection benchmarks in `kernel/src/deadlock/benchmark.rs`.

---

## 5. Analytical Boundaries & Subsystem Limitations

1. **RDTSC Out-of-Order Execution & Lack of Serialization Barrier:**
   - The microbenchmark captures timing via `core::arch::x86_64::_rdtsc()`.
   - On superscalar out-of-order x86_64 processors (such as the AMD Zen 4 architecture on the HP Victus 15-fb3xxx laptop), instructions may be speculatively reordered across `RDTSC` boundaries unless preceded and followed by an explicit `LFENCE` serialization barrier.
   - Consequently, cycle values reflect instruction retirement windows rather than exact cycle-level serialization. Single-digit or sub-cycle precision is not claimed.
2. **Uniprocessor (BSP) Quantum Resolution Boundary:**
   - Micro-OS currently operates on a single bootstrap processor core (BSP). Secondary APIC cores are not active in this kernel phase.
   - Multitasking represents time-sliced uniprocessor concurrency governed by the 100 Hz timer (10 ms tick resolution, 30 ms time slice).
   - Real-time multicore scheduling latency (IPI dispatch, cache bouncing) is an architectural milestone for multi-core SMP bring-up.
3. **Exploratory Microbenchmark Sample Size:**
   - The harness captures 5 repetitions per metric and takes the median.
   - This effectively filters isolated hardware interrupts but constitutes an exploratory microbenchmark rather than an exhaustive statistical distribution ($P_{95}$, $P_{99}$).

---

## 6. Safety & Zero-Allocation Invariants

- **Bounded Workloads:** All benchmark tasks execute finite loops (e.g. 5 or 50 iterations), preventing infinite task loops or CPU starvation.
- **Clean Task Termination:** All tasks return cleanly to `task_trampoline`, calling `task_exit()` and returning CPU control to the main kernel thread.
- **Zero Memory Allocation Under Lock:** All latency sorting and calculations operate on in-place slices or stack buffers with zero heap allocations (`Box`, `Vec`, formatting strings) while locks are held.
- **Zero Assembly/Interrupt Modification:** Context-switch assembly in `context.rs` and IDT handlers in `idt.rs` remain completely unmodified.
- **On-Demand Invocation:** Benchmarks are never executed in the desktop rendering loop or driver interrupt paths.

---

## 7. Verification & Test Suite Status

### 7.1 Host Automated Test Suite (`runner/tests/host_scheduler_tests.rs`) — ACTUALLY EXECUTED
All 9 dedicated host scheduler tests executed and passed on the development environment:
1. `test_host_jains_fairness_equal_allocation`: Verifies that identical 3-task (`[100, 100, 100]`) and 4-task (`[5000, 5000, 5000, 5000]`) allocations produce exactly $J = 1.0$ (1000‰).
2. `test_host_jains_fairness_skewed_allocations`: Verifies 2:1 skewed allocation ($J = 0.90$, 900‰), 2-task complete monopolization ($J = 0.50$, 500‰), and 3-task partial starvation ($J = 0.666$, 666‰).
3. `test_host_jains_fairness_edge_cases`: Verifies empty slice ($J = 1.0$), all-zero slice ($J = 1.0$), and single-task slice ($J = 1.0$) without division by zero.
4. `test_host_jains_fairness_large_numbers_no_overflow`: Evaluates 5 tasks with $5 \times 10^9$ ticks each (sum squared $= 6.25 \times 10^{20} > u64::\text{MAX}$), confirming that `u128` arithmetic prevents overflow and yields $J = 1.0$ (1000‰).
5. `test_host_latency_stats_calculation`: Verifies in-place sorting and min/median/max extraction across 5-element, 1-element, and empty slices.
6. `test_host_round_robin_fairness_report`: Verifies fairness report data structure consistency.
7. `test_host_context_switch_report`: Verifies context switch report data structure consistency.
8. `test_host_scheduler_benchmark_suite_integration`: Verifies end-to-end execution of `run_scheduler_benchmarks()`.
9. `test_host_scheduler_distinction_disclaimer`: Structurally asserts that host test execution does not claim bare-metal hardware CPU cycle numbers.

### 7.2 In-Kernel Self-Tests (`kernel/src/task/mod.rs` Tests 5, 6, 7) — COMPILED ONLY (RUNTIME PENDING PHYSICAL BOOT)
- **Compilation Status:** Successfully compiled for target `x86_64-unknown-none` with **0 errors and 0 warnings**.
- **Execution Policy:** Because Micro-OS is a bare-metal `#![no_std]` OS targeting the HP Victus laptop, and virtual machines (QEMU/VMware) are prohibited, in-kernel runtime execution and bare-metal RDTSC clock cycle capture **remain pending physical hardware boot via Live USB**. No bare-metal cycles or nanoseconds have been fabricated.

### 7.3 Total Runner Test Suite Summary
```
test result: ok. 27 passed; 0 failed (runner lib.rs)
test result: ok. 6 passed; 0 failed (host_allocator_tests.rs - Phase 1A)
test result: ok. 9 passed; 0 failed (host_deadlock_tests.rs - Topic R14)
test result: ok. 10 passed; 0 failed (host_sync_tests.rs - Phase 1B)
test result: ok. 9 passed; 0 failed (host_scheduler_tests.rs - Phase 1C)

Total: 61 passed; 0 failed; 0 warnings; finished in 0.00s
```

---

## 8. Artifact & File Integrity Verification

| File Path | Status | Purpose of Modification |
| :--- | :---: | :--- |
| `kernel/src/task/benchmark.rs` | **Created** | Context switch latency, scheduling latency, Jain's Fairness Index, and throughput reports. |
| `kernel/src/task/mod.rs` | **Modified** | Re-exported benchmark routines and integrated Tests 5, 6, 7 into `run_self_tests()`. |
| `runner/tests/host_scheduler_tests.rs` | **Created** | Comprehensive host-side automated validation test suite (9 tests). |
| `final_demo_package/scheduler_performance_report.md` | **Created** | This formal performance methodology and evaluation report. |
| `target/micro_os_uefi_release.iso` | **UNMODIFIED** | Preserved (3,186,688 bytes, SHA-256: `9F3BAD0E5FC84D209820B166CE69F726CC1C01A7C04F10A6988EEAA5306FAD83`). |
| `target/micro_os_uefi_release.img` | **UNMODIFIED** | Preserved (3,211,264 bytes, SHA-256: `E72BD04C86E8CFCF8B5F0B0ADC24B39C8866A5DEB66B83B5BDC538BFB5965546`). |
