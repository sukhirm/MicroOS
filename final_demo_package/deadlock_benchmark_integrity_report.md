# Micro-OS: Deadlock Benchmark Integrity Audit Report
**Forensic Audit of Research Activity R14 Benchmark Telemetry & Data Provenance**

**Project:** Micro-OS Bare-Metal x86_64 Operating System  
**Audit Target:** Topic R14 — Deadlock Avoidance vs. Detection Benchmark Telemetry  
**File Location:** `final_demo_package/deadlock_benchmark_integrity_report.md`  
**Date:** November 17, 2026  
**Status:** Completed & Synchronized  

---

## 1. Executive Summary

A comprehensive forensic audit was conducted across all source code, tests, and documentation files in MicroOS to audit the benchmark figures for **Topic R14 (Deadlock Avoidance vs. Detection)**.

### Primary Audit Findings:
1. **The Graphical Deadlock Research Lab Displays the 100% CORRECT Values:**
   In [kernel/src/apps/deadlock.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock.rs#L53-L59), the array `RESEARCH_RESULTS_TABLE` contains the exact nanosecond figures recorded in the original research report ($569.51$ ns to $8117.86$ ns for Banker's; $147.51$ ns to $907.88$ ns for Detection). The UI renders these exact numbers in the Results table, plots the 2D vector line chart canvas directly from them, and analyzes them in the Analysis tab.
2. **Host and Kernel UI Tests Validate the CORRECT Values:**
   Both [kernel/src/apps/deadlock_tests.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock_tests.rs#L99-L103) and [runner/tests/host_deadlock_tests.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/runner/tests/host_deadlock_tests.rs#L131-L135) test and assert against the exact research figures ($569.51$, $931.40$, $1833.23$, $4070.39$, $8117.86$ ns for Banker's; $147.51$, $201.79$, $289.76$, $536.60$, $907.88$ ns for Detection).
3. **The Audit Report (`final_project_audit.md`) Contained Erroneous Figures (Now Corrected):**
   In [final_demo_package/final_project_audit.md](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/final_project_audit.md#L118-L119), lines 118 and 119 previously reported unverified numbers ($142\text{ ns} \to 7850\text{ ns}$ and $98\text{ ns} \to 8420\text{ ns}$). These figures have been replaced with the verified canonical research nanosecond figures.
4. **`kernel/src/deadlock/benchmark.rs` Preserves Historical Microsecond Data:**
   In [kernel/src/deadlock/benchmark.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L34-L50), `ORIGINAL_CA3_BENCHMARKS` and `ORIGINAL_V2_BENCHMARKS` store legacy microsecond ($\mu\text{s}$) figures sourced from earlier host Python simulation scripts across process counts $\{5, 10, 20, 40, 80, 120\}$.
5. **Original Research Values Were Never Overwritten:**
   The original research figures remain intact in `deadlock.rs`, `deadlock_tests.rs`, `host_deadlock_tests.rs`, and `step2_deadlock_research_lab_report.md`.

---

## 2. Three-Tier Measurement Taxonomy & Strict Integrity Distinction

To eliminate ambiguity across all evaluation reports, MicroOS strictly classifies performance measurements into three mutually exclusive tiers:

| Measurement Tier | Units | Data Origin | Scope & Target Files | Conversion Rule |
| :--- | :---: | :--- | :--- | :--- |
| **Tier 1: Original Research Benchmarks** | **Nanoseconds (ns)** | Academic Research Activity R14 Write-Up | Preserved in `apps/deadlock.rs`, `apps/deadlock_tests.rs`, `host_deadlock_tests.rs`, `final_project_audit.md`. Workloads: $N \in \{5, 10, 20, 50, 100\}$. | Ground-truth baseline. Immutable constants. |
| **Tier 2: Legacy Simulation Prototypes** | **Microseconds ($\mu\text{s}$)** | Historical Python exploration scripts (`r14_results.csv`, `deadlock_results_v2.json`) | Preserved in `kernel/src/deadlock/benchmark.rs` as `ORIGINAL_CA3_BENCHMARKS` and `ORIGINAL_V2_BENCHMARKS`. Workloads: $N \in \{5, 10, 20, 40, 80, 120\}$. | Historical reference only. Distinct from Tier 1. |
| **Tier 3: Bare-Metal MicroOS Measurements** | **CPU Clock Cycles** | Live x86 `rdtsc` instruction executed at kernel runtime on physical hardware | Collected in `kernel/src/deadlock/benchmark.rs` via `benchmark_live_workload()`. | **Strict Rule:** Raw CPU clock cycles must NEVER be converted to nanoseconds without a verified hardware timer calibration (e.g., PIT counter delta vs. TSC frequency). |

---

## 3. Definitive Discrepancy Matrix

The table below tracks every value recorded for each process count tier ($N \in \{5, 10, 20, 50, 100\}$) across all files in the project:

### 3.1 Banker's Algorithm Avoidance Discrepancy Comparison

| Processes ($N$) | Target Research Baseline | `apps/deadlock.rs` ([L53–59](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock.rs#L53-L59)) | `apps/deadlock_tests.rs` ([L99–103](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock_tests.rs#L99-L103)) | `runner/tests/host_deadlock_tests.rs` ([L131–135](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/runner/tests/host_deadlock_tests.rs#L131-L135)) | `step2_deadlock_report.md` ([L73–77](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/step2_deadlock_research_lab_report.md#L73-L77)) | `deadlock/benchmark.rs` ([L34–50](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L34-L50)) | `final_project_audit.md` (Corrected Status) |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **5** | **569.51 ns** | **569.51 ns** (`MATCH`) | **569.51 ns** (`MATCH`) | **569.51 ns** (`MATCH`) | **569.51 ns** (`MATCH`) | $6.20\ \mu\text{s}$ / $1.70\ \mu\text{s}$ (`Legacy CA3`) | **569.51 ns** (`MATCH`) |
| **10** | **931.40 ns** | **931.40 ns** (`MATCH`) | **931.40 ns** (`MATCH`) | **931.40 ns** (`MATCH`) | **931.40 ns** (`MATCH`) | $12.00\ \mu\text{s}$ / $3.10\ \mu\text{s}$ (`Legacy CA3`) | **931.40 ns** (`MATCH`) |
| **20** | **1833.23 ns** | **1833.23 ns** (`MATCH`) | **1833.23 ns** (`MATCH`) | **1833.23 ns** (`MATCH`) | **1833.23 ns** (`MATCH`) | $27.60\ \mu\text{s}$ / $17.70\ \mu\text{s}$ (`Legacy CA3`) | **1833.23 ns** (`MATCH`) |
| **50** | **4070.39 ns** | **4070.39 ns** (`MATCH`) | **4070.39 ns** (`MATCH`) | **4070.39 ns** (`MATCH`) | **4070.39 ns** (`MATCH`) | $44.30\ \mu\text{s}$ (at $N=40$) (`Legacy CA3`) | **4070.39 ns** (`MATCH`) |
| **100** | **8117.86 ns** | **8117.86 ns** (`MATCH`) | **8117.86 ns** (`MATCH`) | **8117.86 ns** (`MATCH`) | **8117.86 ns** (`MATCH`) | $88.90\ \mu\text{s}$ (at $N=80$) (`Legacy CA3`) | **8117.86 ns** (`MATCH`) |

---

### 3.2 Deadlock Detection Discrepancy Comparison

| Processes ($N$) | Target Research Baseline | `apps/deadlock.rs` ([L53–59](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock.rs#L53-L59)) | `apps/deadlock_tests.rs` ([L99–103](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock_tests.rs#L99-L103)) | `runner/tests/host_deadlock_tests.rs` ([L131–135](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/runner/tests/host_deadlock_tests.rs#L131-L135)) | `step2_deadlock_report.md` ([L73–77](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/step2_deadlock_research_lab_report.md#L73-L77)) | `deadlock/benchmark.rs` ([L34–50](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L34-L50)) | `final_project_audit.md` (Corrected Status) |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **5** | **147.51 ns** | **147.51 ns** (`MATCH`) | **147.51 ns** (`MATCH`) | **147.51 ns** (`MATCH`) | **147.51 ns** (`MATCH`) | $8.50\ \mu\text{s}$ / $4.30\ \mu\text{s}$ (`Legacy CA3`) | **147.51 ns** (`MATCH`) |
| **10** | **201.79 ns** | **201.79 ns** (`MATCH`) | **201.79 ns** (`MATCH`) | **201.79 ns** (`MATCH`) | **201.79 ns** (`MATCH`) | $16.10\ \mu\text{s}$ / $14.00\ \mu\text{s}$ (`Legacy CA3`) | **201.79 ns** (`MATCH`) |
| **20** | **289.76 ns** | **289.76 ns** (`MATCH`) | **289.76 ns** (`MATCH`) | **289.76 ns** (`MATCH`) | **289.76 ns** (`MATCH`) | $31.90\ \mu\text{s}$ / $24.75\ \mu\text{s}$ (`Legacy CA3`) | **289.76 ns** (`MATCH`) |
| **50** | **536.60 ns** | **536.60 ns** (`MATCH`) | **536.60 ns** (`MATCH`) | **536.60 ns** (`MATCH`) | **536.60 ns** (`MATCH`) | $60.70\ \mu\text{s}$ (at $N=40$) (`Legacy CA3`) | **536.60 ns** (`MATCH`) |
| **100** | **907.88 ns** | **907.88 ns** (`MATCH`) | **907.88 ns** (`MATCH`) | **907.88 ns** (`MATCH`) | **907.88 ns** (`MATCH`) | $122.50\ \mu\text{s}$ (at $N=80$) (`Legacy CA3`) | **907.88 ns** (`MATCH`) |

---

## 4. Forensic Answers to Specific Audit Questions

### Question 1: Which values came from the original research?
The canonical values from the research report are:
- **Banker's Algorithm Avoidance:**
  - 5 processes: **569.51 ns**
  - 10 processes: **931.40 ns**
  - 20 processes: **1833.23 ns**
  - 50 processes: **4070.39 ns**
  - 100 processes: **8117.86 ns**
- **Deadlock Detection:**
  - 5 processes: **147.51 ns**
  - 10 processes: **201.79 ns**
  - 20 processes: **289.76 ns**
  - 50 processes: **536.60 ns**
  - 100 processes: **907.88 ns**

These values are preserved in:
- [kernel/src/apps/deadlock.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock.rs#L53-L59) (`RESEARCH_RESULTS_TABLE`)
- [kernel/src/apps/deadlock_tests.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock_tests.rs#L99-L103)
- [runner/tests/host_deadlock_tests.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/runner/tests/host_deadlock_tests.rs#L131-L135)
- [final_demo_package/step2_deadlock_research_lab_report.md](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/step2_deadlock_research_lab_report.md#L73-L77)
- [final_demo_package/final_project_audit.md](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/final_project_audit.md#L118-L119) (Corrected)

---

### Question 2: Which values are from later experiments or measurements?
1. **Host Python Prototype Measurements (`ORIGINAL_CA3_BENCHMARKS`):**
   - Located in [kernel/src/deadlock/benchmark.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L34-L41).
   - Sourced from earlier exploratory Python scripts (`CA3_R14/r14_results.csv`).
   - Units: **Microseconds ($\mu\text{s}$)**. Process tiers: $5, 10, 20, 40, 80, 120$.
2. **Investigation v2 Measurements (`ORIGINAL_V2_BENCHMARKS`):**
   - Located in [kernel/src/deadlock/benchmark.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L44-L50).
   - Sourced from `deadlock_results_v2/results.csv`.
   - Units: **Microseconds ($\mu\text{s}$)**. Process tiers: $5, 10, 20, 40, 80$.
3. **Live MicroOS Kernel Measurements (`MicroOsBenchmarkMeasurement`):**
   - Located in [kernel/src/deadlock/benchmark.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/benchmark.rs#L77-L147).
   - Generated at runtime using bare-metal x86 `_rdtsc()` CPU clock cycle timing over multiple repetitions with outlier rejection.
   - Units: **CPU instruction clock cycles** (never nanoseconds).

---

### Question 3: Whether any original values were accidentally overwritten?
- **In `kernel/src/apps/deadlock.rs`:** **NO.** The values were never overwritten. The 5 exact nanosecond rows are stored in `RESEARCH_RESULTS_TABLE` and verified in both host and kernel tests.
- **In `final_demo_package/step2_deadlock_research_lab_report.md`:** **NO.** The values are properly documented.
- **In `kernel/src/deadlock/benchmark.rs`:** The canonical nanosecond table was not placed here during Step 1 because Step 1 preceded Prompt 9; instead, older Python microsecond baselines were placed here.
- **In `final_demo_package/final_project_audit.md`:** **CORRECTED.** The audit report previously contained erroneous numbers (142–7850 ns and 98–8420 ns). This has now been corrected with the canonical nanosecond values.

---

### Question 4: Whether the graphical Deadlock Research Lab displays the correct values?
- **YES, 100% CORRECT.**
- In [kernel/src/apps/deadlock.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/apps/deadlock.rs#L53-L59):
  - Section 5 (Results Table) reads directly from `RESEARCH_RESULTS_TABLE`:
    - Row 1 ($N=5$): Banker $569.51$ ns, Detection $147.51$ ns, Speedup $3.86\times$
    - Row 2 ($N=10$): Banker $931.40$ ns, Detection $201.79$ ns, Speedup $4.62\times$
    - Row 3 ($N=20$): Banker $1833.23$ ns, Detection $289.76$ ns, Speedup $6.33\times$
    - Row 4 ($N=50$): Banker $4070.39$ ns, Detection $536.60$ ns, Speedup $7.59\times$
    - Row 5 ($N=100$): Banker $8117.86$ ns, Detection $907.88$ ns, Speedup $8.94\times$
  - Section 5 (2D Vector Line Chart Canvas):
    - Uses `calculate_chart_point` to plot the Cyan curve ($569.51 \to 8117.86$ ns) and the Green curve ($147.51 \to 907.88$ ns).
  - Section 6 (Analysis):
    - Line 966 explicitly states: *"Across all workloads (N=5 to 100), Detection runs in 147 ns - 907 ns, while Banker's takes 569 ns - 8117 ns."*

---

### Question 5: Whether the audit report contains incorrect benchmark figures?
- **PREVIOUSLY YES, NOW RESOLVED.**
- In [final_demo_package/final_project_audit.md](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/final_project_audit.md#L118-L119), lines 118–119 have been edited to match the canonical 10 research values exactly.

---

## 5. Status of `kernel/src/deadlock/workload.rs`

Inspection of [kernel/src/deadlock/workload.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/deadlock/workload.rs):
- **Workload Sizes:** Line 18 defines `pub const WORKLOAD_SIZES: [usize; 5] = [5, 10, 20, 50, 100];`.
- **Resource Types:** Line 21 defines `pub const DEFAULT_RESOURCE_TYPES: usize = 3;`.
- **Benchmark Numbers:** `workload.rs` contains **zero hardcoded timing figures**. It strictly acts as a synthetic matrix generator for `BankerState` and `DetectionState`.
- **Status:** **CORRECT & CLEAN.**

---

## 6. Verification Summary of Modified Files

| File Path | Status | Details of Changes |
| :--- | :---: | :--- |
| [final_demo_package/final_project_audit.md](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/final_project_audit.md) | **Modified** | Replaced erroneous benchmark figures in Section 5 with canonical nanosecond values; added explicit three-tier measurement taxonomy in Section 6. |
| [final_demo_package/deadlock_benchmark_integrity_report.md](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/final_demo_package/deadlock_benchmark_integrity_report.md) | **Created** | Comprehensive permanent forensic audit report detailing data provenance across all files. |
| `kernel/*` | **UNMODIFIED** | Zero kernel source code files modified. |
| `target/micro_os_uefi_release.iso` | **UNMODIFIED** | Binary release artifact preserved (3,186,688 bytes). |
| `target/micro_os_uefi_release.img` | **UNMODIFIED** | Binary release artifact preserved (3,211,264 bytes). |
