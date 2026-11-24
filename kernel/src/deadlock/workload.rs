//! Synthetic Deterministic Workload Generator for Deadlock Benchmarking.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! Generates deterministic workload matrices for benchmarking Banker's Avoidance
//! and Graph Deadlock Detection across process counts:
//!     n in {5, 10, 20, 50, 100}
//! with bounded memory constraints (`MAX_PROCESSES`, `MAX_RESOURCES`).

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use super::avoidance::{BankerState, DeadlockError, MAX_PROCESSES};
use super::detection::DetectionState;

/// The canonical workload process counts required by the research write-up.
pub const WORKLOAD_SIZES: [usize; 5] = [5, 10, 20, 50, 100];

/// Default number of resource classes used across research benchmarks.
pub const DEFAULT_RESOURCE_TYPES: usize = 3;

/// Generates a deterministic Banker's Algorithm test workload for `n` processes and `m` resources.
///
/// Designed to represent a safe multi-process system where processes can sequentially complete.
pub fn generate_banker_workload(
    processes: usize,
    resources: usize,
) -> Result<BankerState, DeadlockError> {
    if processes > MAX_PROCESSES {
        return Err(DeadlockError::ProcessLimitExceeded {
            count: processes,
            max: MAX_PROCESSES,
        });
    }

    // Available resources scaled to ensure safe progression
    let base_avail = (processes / 2) + 4;
    let available = vec![base_avail; resources];

    let mut allocation = Vec::with_capacity(processes);
    let mut need = Vec::with_capacity(processes);

    for p in 0..processes {
        let mut alloc_row = Vec::with_capacity(resources);
        let mut need_row = Vec::with_capacity(resources);

        for r in 0..resources {
            // Deterministic periodic allocation pattern
            let a = (p + r) % 3;
            // Need pattern bounded by 4
            let n_val = ((p * 2 + r) % 4) + 1;

            alloc_row.push(a);
            need_row.push(n_val);
        }

        allocation.push(alloc_row);
        need.push(need_row);
    }

    BankerState::from_need(available, allocation, need)
}

/// Generates a deterministic Deadlock Detection test workload for `n` processes and `m` resources.
///
/// If `induce_deadlock` is true, injects a circular wait condition between the first two processes
/// ($P_0 \leftrightarrow P_1$) while leaving remaining processes satisfiable.
pub fn generate_detection_workload(
    processes: usize,
    resources: usize,
    induce_deadlock: bool,
) -> Result<DetectionState, DeadlockError> {
    if processes > MAX_PROCESSES {
        return Err(DeadlockError::ProcessLimitExceeded {
            count: processes,
            max: MAX_PROCESSES,
        });
    }

    let base_avail = if induce_deadlock { 0 } else { (processes / 2) + 4 };
    let available = vec![base_avail; resources];

    let mut allocation = Vec::with_capacity(processes);
    let mut request = Vec::with_capacity(processes);

    for p in 0..processes {
        let mut alloc_row = Vec::with_capacity(resources);
        let mut req_row = Vec::with_capacity(resources);

        for r in 0..resources {
            alloc_row.push((p + r) % 3);
            req_row.push(((p * 2 + r) % 4) + 1);
        }

        allocation.push(alloc_row);
        request.push(req_row);
    }

    if induce_deadlock && processes >= 2 && resources >= 2 {
        // Force circular wait between P0 and P1:
        // P0 holds R0=1, requests R1=1
        // P1 holds R1=1, requests R0=1
        // Available = [0, 0, ...]
        allocation[0] = vec![1, 0, 0];
        request[0] = vec![0, 1, 0];

        allocation[1] = vec![0, 1, 0];
        request[1] = vec![1, 0, 0];
    }

    DetectionState::new(available, allocation, request)
}
