//! Graph-based / Resource-Request Matrix Deadlock Detection Algorithm.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! This module implements the classical multi-resource deadlock detection algorithm
//! (equivalent to Wait-For Graph knot / cycle reduction) in `#![no_std]` Rust.
//!
//! ### Detection Algorithm Mechanism
//! Unlike Banker's Avoidance, which checks maximum declared claims before granting requests,
//! Deadlock Detection runs retroactively on active requests:
//! 1. Work vector initialized to Available: `Work = Available`.
//! 2. Any process holding **zero** resources (`Allocation[p] == 0`) cannot be causing a deadlock
//!    by holding items others want, and is immediately marked finished: `Finish[p] = true`.
//! 3. The algorithm iteratively finds an unmarked process `p` whose current outstanding request
//!    can be satisfied: `Request[p][r] <= Work[r]` for all resource types `r`.
//! 4. When such a process finishes, its allocated resources are returned to `Work`:
//!    `Work[r] <- Work[r] + Allocation[p][r]`, and `Finish[p] = true`.
//! 5. If no more processes can be reduced and unfinished processes remain, those processes
//!    are deadlocked in an unresolved cycle / knot.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use super::avoidance::{DeadlockError, MAX_PROCESSES, MAX_RESOURCES};

/// Output result of the deadlock detection evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionResult {
    /// Indices of processes identified as deadlocked (empty if no deadlock).
    pub deadlocked_processes: Vec<usize>,
    /// Whether any deadlock was detected in the evaluated state.
    pub is_deadlocked: bool,
    /// Number of element-wise resource comparisons performed during detection.
    pub resource_comparisons: usize,
    /// Number of reduction passes executed.
    pub reduction_passes: usize,
}

/// State representation for Resource-Request Deadlock Detection.
#[derive(Debug, Clone)]
pub struct DetectionState {
    /// Number of active processes in the system ($n$).
    pub num_processes: usize,
    /// Number of resource types in the system ($m$).
    pub num_resources: usize,
    /// Vector of currently available resource instances: Available[m].
    pub available: Vec<usize>,
    /// Matrix of currently allocated resources: Allocation[n][m].
    pub allocation: Vec<Vec<usize>>,
    /// Matrix of active outstanding requests: Request[n][m].
    pub request: Vec<Vec<usize>>,
}

impl DetectionState {
    /// Creates a new `DetectionState` with bounded validation.
    pub fn new(
        available: Vec<usize>,
        allocation: Vec<Vec<usize>>,
        request: Vec<Vec<usize>>,
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
        if request.len() != num_processes {
            return Err(DeadlockError::DimensionMismatch);
        }

        for p in 0..num_processes {
            if allocation[p].len() != num_resources || request[p].len() != num_resources {
                return Err(DeadlockError::DimensionMismatch);
            }
        }

        Ok(Self {
            num_processes,
            num_resources,
            available,
            allocation,
            request,
        })
    }

    /// Evaluates the system state for deadlocks using matrix reduction.
    ///
    /// Complexity: $O(m \cdot n^2)$ worst-case time, $O(m + n)$ space.
    pub fn detect_deadlock(&self) -> DetectionResult {
        let n = self.num_processes;
        let m = self.num_resources;

        let mut work = self.available.clone();

        // Step 1: Processes holding no resources are marked finished immediately.
        let mut finish = vec![false; n];
        for p in 0..n {
            let holds_nothing = self.allocation[p].iter().all(|&count| count == 0);
            if holds_nothing {
                finish[p] = true;
            }
        }

        let mut comparisons = 0usize;
        let mut passes = 0usize;

        // Step 2: Iterative graph reduction loop
        loop {
            passes += 1;
            let mut found_progress = false;

            for p in 0..n {
                if finish[p] {
                    continue;
                }

                // Check condition: Request[p][r] <= Work[r] for all r
                let mut can_finish = true;
                for r in 0..m {
                    comparisons += 1;
                    if self.request[p][r] > work[r] {
                        can_finish = false;
                        break;
                    }
                }

                if can_finish {
                    // Process p can complete and release held resources
                    for r in 0..m {
                        work[r] += self.allocation[p][r];
                    }
                    finish[p] = true;
                    found_progress = true;
                }
            }

            if !found_progress {
                break;
            }
        }

        // Step 3: Any process still unfinished is deadlocked
        let mut deadlocked = Vec::new();
        for p in 0..n {
            if !finish[p] {
                deadlocked.push(p);
            }
        }

        let is_deadlocked = !deadlocked.is_empty();
        DetectionResult {
            deadlocked_processes: deadlocked,
            is_deadlocked,
            resource_comparisons: comparisons,
            reduction_passes: passes,
        }
    }
}
