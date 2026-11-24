//! Banker's Safety Algorithm for Deadlock Avoidance.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! This module implements Dijkstra's Banker's Algorithm safety check in `#![no_std]` Rust.
//! It includes both:
//! 1. `is_safe()`: The corrected, production-grade safety algorithm with exact-fit support.
//! 2. `is_safe_defective()`: The deliberately defective implementation reproducing the
//!    exact-fit boundary defect investigated in Research Report CA-3 / R14.
//!
//! ### Boundary Condition Defect Explanation
//! In classical Banker's safety verification, a process `p` may proceed to completion if its
//! outstanding resource demand satisfies:
//!     Need[p][r] <= Work[r]  for all resource types r.
//!
//! A common implementation defect occurs when the comparison strictly disallows equality:
//!     if Need[p][r] >= Work[r] { cannot_proceed(); } // DEFECT
//!
//! In this defective form, when Need[p][r] == Work[r], available resources exactly match the
//! process's remaining demand. The process should be allowed to run, complete its execution,
//! and release all its currently held resources back into the available pool:
//!     Work[r] <- Work[r] + Allocation[p][r].
//!
//! By rejecting `Need == Work`, the defective algorithm falsely flags safe states as unsafe,
//! artificially starving processes and causing spurious allocation rejections.
//! The corrected implementation tests `Need[p][r] > Work[r]` before rejecting, ensuring that
//! exact resource fits are safely accepted.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

/// Maximum number of processes supported by the bounded kernel deadlock subsystem.
pub const MAX_PROCESSES: usize = 128;

/// Maximum number of distinct resource types supported by the bounded kernel deadlock subsystem.
pub const MAX_RESOURCES: usize = 16;

/// Errors returned during deadlock matrix validation or state construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockError {
    /// Process count exceeds the kernel bounded limit (`MAX_PROCESSES`).
    ProcessLimitExceeded { count: usize, max: usize },
    /// Resource count exceeds the kernel bounded limit (`MAX_RESOURCES`).
    ResourceLimitExceeded { count: usize, max: usize },
    /// Matrix dimensions do not match between available, allocation, and need/max.
    DimensionMismatch,
    /// An allocated resource count is inconsistent or negative in signed contexts.
    InvalidAllocation,
    /// A process's allocation exceeds its declared maximum claim.
    AllocationExceedsMaximum,
}

/// Output result of the Banker's safety evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankerResult {
    /// Whether the system state is safe (a complete execution sequence exists).
    pub is_safe: bool,
    /// Safe execution sequence of process indices if safe, or partial sequence if unsafe.
    pub safe_sequence: Vec<usize>,
    /// Number of element-wise resource comparisons performed during evaluation.
    pub resource_comparisons: usize,
    /// Number of scheduling passes / loop iterations executed.
    pub iteration_passes: usize,
}

/// State representation for Banker's Deadlock Avoidance.
#[derive(Debug, Clone)]
pub struct BankerState {
    /// Number of active processes in the system ($n$).
    pub num_processes: usize,
    /// Number of resource types in the system ($m$).
    pub num_resources: usize,
    /// Vector of available instances per resource type: Available[m].
    pub available: Vec<usize>,
    /// Matrix of currently allocated resources: Allocation[n][m].
    pub allocation: Vec<Vec<usize>>,
    /// Matrix of maximum resource claims: Max[n][m].
    pub max: Vec<Vec<usize>>,
    /// Matrix of remaining resource needs: Need[n][m] = Max[n][m] - Allocation[n][m].
    pub need: Vec<Vec<usize>>,
}

impl BankerState {
    /// Creates a new `BankerState` with bounded validation and automatically computes `Need`.
    pub fn new(
        available: Vec<usize>,
        allocation: Vec<Vec<usize>>,
        max: Vec<Vec<usize>>,
    ) -> Result<Self, DeadlockError> {
        let num_resources = available.len();
        let num_processes = allocation.len();

        if num_processes > MAX_PROCESSES {
            return Err(DeadlockError::ProcessLimitExceeded {
                count: num_processes,
                max: MAX_PROCESSES,
            });
        }
        if num_resources > MAX_RESOURCES {
            return Err(DeadlockError::ResourceLimitExceeded {
                count: num_resources,
                max: MAX_RESOURCES,
            });
        }
        if max.len() != num_processes {
            return Err(DeadlockError::DimensionMismatch);
        }

        // Validate row dimensions and compute Need matrix
        let mut need = Vec::with_capacity(num_processes);
        for p in 0..num_processes {
            if allocation[p].len() != num_resources || max[p].len() != num_resources {
                return Err(DeadlockError::DimensionMismatch);
            }
            let mut need_row = Vec::with_capacity(num_resources);
            for r in 0..num_resources {
                if allocation[p][r] > max[p][r] {
                    return Err(DeadlockError::AllocationExceedsMaximum);
                }
                need_row.push(max[p][r] - allocation[p][r]);
            }
            need.push(need_row);
        }

        Ok(Self {
            num_processes,
            num_resources,
            available,
            allocation,
            max,
            need,
        })
    }

    /// Creates a `BankerState` directly from an existing `Need` matrix (e.g. for synthetic workloads).
    pub fn from_need(
        available: Vec<usize>,
        allocation: Vec<Vec<usize>>,
        need: Vec<Vec<usize>>,
    ) -> Result<Self, DeadlockError> {
        let num_resources = available.len();
        let num_processes = allocation.len();

        if num_processes > MAX_PROCESSES {
            return Err(DeadlockError::ProcessLimitExceeded {
                count: num_processes,
                max: MAX_PROCESSES,
            });
        }
        if num_resources > MAX_RESOURCES {
            return Err(DeadlockError::ResourceLimitExceeded {
                count: num_resources,
                max: MAX_RESOURCES,
            });
        }
        if need.len() != num_processes {
            return Err(DeadlockError::DimensionMismatch);
        }

        let mut max = Vec::with_capacity(num_processes);
        for p in 0..num_processes {
            if allocation[p].len() != num_resources || need[p].len() != num_resources {
                return Err(DeadlockError::DimensionMismatch);
            }
            let mut max_row = Vec::with_capacity(num_resources);
            for r in 0..num_resources {
                max_row.push(allocation[p][r] + need[p][r]);
            }
            max.push(max_row);
        }

        Ok(Self {
            num_processes,
            num_resources,
            available,
            allocation,
            max,
            need,
        })
    }

    /// Executes the **CORRECTED** Banker's Safety Algorithm.
    ///
    /// Evaluates whether the system is in a safe state by simulating process completion.
    /// Supports exact-fit allocations (`need[p][r] <= work[r]`).
    ///
    /// Complexity: $O(m \cdot n^2)$ worst-case time, $O(m + n)$ space.
    pub fn is_safe(&self) -> BankerResult {
        let n = self.num_processes;
        let m = self.num_resources;

        let mut work = self.available.clone();
        let mut finish = vec![false; n];
        let mut sequence = Vec::with_capacity(n);
        let mut comparisons = 0usize;
        let mut passes = 0usize;

        loop {
            passes += 1;
            let mut found_progress = false;

            for p in 0..n {
                if finish[p] {
                    continue;
                }

                // Check condition: Need[p][r] <= Work[r] for all r.
                // If any Need[p][r] > Work[r], process p cannot finish now.
                let mut can_finish = true;
                for r in 0..m {
                    comparisons += 1;
                    if self.need[p][r] > work[r] {
                        can_finish = false;
                        break;
                    }
                }

                if can_finish {
                    // Process p finishes and returns all allocated resources to work pool
                    for r in 0..m {
                        work[r] += self.allocation[p][r];
                    }
                    finish[p] = true;
                    sequence.push(p);
                    found_progress = true;
                }
            }

            if !found_progress {
                break;
            }
        }

        let is_safe = finish.iter().all(|&done| done);
        BankerResult {
            is_safe,
            safe_sequence: sequence,
            resource_comparisons: comparisons,
            iteration_passes: passes,
        }
    }

    /// Executes the **DELIBERATELY DEFECTIVE** Banker's Safety Algorithm.
    ///
    /// Reproduces the exact-fit defect from Research Report CA-3 / R14:
    /// Uses `need[p][r] >= work[r]` instead of `need[p][r] > work[r]`,
    /// causing the algorithm to reject processes whose demand matches available supply exactly.
    pub fn is_safe_defective(&self) -> BankerResult {
        let n = self.num_processes;
        let m = self.num_resources;

        let mut work = self.available.clone();
        let mut finish = vec![false; n];
        let mut sequence = Vec::with_capacity(n);
        let mut comparisons = 0usize;
        let mut passes = 0usize;

        loop {
            passes += 1;
            let mut found_progress = false;

            for p in 0..n {
                if finish[p] {
                    continue;
                }

                let mut can_finish = true;
                for r in 0..m {
                    comparisons += 1;
                    // DELIBERATE DEFECT: >= rejects exact-fit allocations where need == work!
                    if self.need[p][r] >= work[r] {
                        can_finish = false;
                        break;
                    }
                }

                if can_finish {
                    for r in 0..m {
                        work[r] += self.allocation[p][r];
                    }
                    finish[p] = true;
                    sequence.push(p);
                    found_progress = true;
                }
            }

            if !found_progress {
                break;
            }
        }

        let is_safe = finish.iter().all(|&done| done);
        BankerResult {
            is_safe,
            safe_sequence: sequence,
            resource_comparisons: comparisons,
            iteration_passes: passes,
        }
    }
}
