//! Host-side automated validation tests for Deadlock Research Subsystem.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! These tests execute directly on the host development machine (Windows x86_64)
//! to verify the mathematical and algorithmic correctness of the Deadlock Subsystem
//! outside the bare-metal kernel environment.

#![allow(dead_code, unused_imports)]

#[path = "../../kernel/src/deadlock/avoidance.rs"]
mod avoidance;

#[path = "../../kernel/src/deadlock/detection.rs"]
mod detection;

#[path = "../../kernel/src/deadlock/workload.rs"]
mod workload;

use avoidance::{BankerState, DeadlockError, MAX_PROCESSES, MAX_RESOURCES};
use detection::DetectionState;
use workload::{generate_banker_workload, generate_detection_workload, WORKLOAD_SIZES};

#[test]
fn test_host_exact_fit_boundary() {
    let state = BankerState::from_need(vec![2], vec![vec![0]], vec![vec![2]]).unwrap();
    let buggy = state.is_safe_defective();
    assert!(!buggy.is_safe, "Deliberate defect should reject exact fit");

    let fixed = state.is_safe();
    assert!(fixed.is_safe, "Corrected algorithm must accept exact fit");
    assert_eq!(fixed.safe_sequence, vec![0]);
}

#[test]
fn test_host_safe_multiprocess() {
    let state = BankerState::from_need(
        vec![3, 3],
        vec![vec![1, 0], vec![0, 1], vec![1, 1]],
        vec![vec![2, 1], vec![1, 2], vec![1, 1]],
    ).unwrap();
    let res = state.is_safe();
    assert!(res.is_safe);
    assert_eq!(res.safe_sequence.len(), 3);
}

#[test]
fn test_host_unsafe_state() {
    let state = BankerState::from_need(
        vec![0, 0],
        vec![vec![1, 0], vec![0, 1]],
        vec![vec![0, 1], vec![1, 0]],
    ).unwrap();
    let res = state.is_safe();
    assert!(!res.is_safe);
}

#[test]
fn test_host_deadlock_detection_cycle() {
    let state = DetectionState::new(
        vec![0],
        vec![vec![1], vec![1]],
        vec![vec![1], vec![1]],
    ).unwrap();
    let res = state.detect_deadlock();
    assert!(res.is_deadlocked);
    assert_eq!(res.deadlocked_processes, vec![0, 1]);
}

#[test]
fn test_host_deadlock_detection_recovery() {
    let state = DetectionState::new(
        vec![0],
        vec![vec![1], vec![1]],
        vec![vec![0], vec![1]],
    ).unwrap();
    let res = state.detect_deadlock();
    assert!(!res.is_deadlocked);
    assert!(res.deadlocked_processes.is_empty());
}

#[test]
fn test_host_workload_scaling_5_to_100() {
    for &n in &WORKLOAD_SIZES {
        let banker = generate_banker_workload(n, 3).unwrap();
        let b_res = banker.is_safe();
        assert!(b_res.is_safe, "Banker failed for N={}", n);
        assert_eq!(b_res.safe_sequence.len(), n);

        let detection = generate_detection_workload(n, 3, false).unwrap();
        let d_res = detection.detect_deadlock();
        assert!(!d_res.is_deadlocked, "Detection false positive for N={}", n);
    }
}

#[test]
fn test_host_bounded_memory_safety() {
    let avail = vec![1; 3];
    let alloc = vec![vec![0; 3]; MAX_PROCESSES + 1];
    let need = vec![vec![1; 3]; MAX_PROCESSES + 1];
    let err = BankerState::from_need(avail, alloc, need);
    assert_eq!(
        err.unwrap_err(),
        DeadlockError::ProcessLimitExceeded {
            count: MAX_PROCESSES + 1,
            max: MAX_PROCESSES,
        }
    );

    let avail_r = vec![1; MAX_RESOURCES + 1];
    let alloc_r = vec![vec![0; MAX_RESOURCES + 1]];
    let need_r = vec![vec![1; MAX_RESOURCES + 1]];
    let err_r = BankerState::from_need(avail_r, alloc_r, need_r);
    assert_eq!(
        err_r.unwrap_err(),
        DeadlockError::ResourceLimitExceeded {
            count: MAX_RESOURCES + 1,
            max: MAX_RESOURCES,
        }
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResultsTableRow {
    pub processes: usize,
    pub banker_ns: f32,
    pub detection_ns: f32,
}

pub const RESEARCH_RESULTS_TABLE: [ResultsTableRow; 5] = [
    ResultsTableRow { processes: 5,   banker_ns: 569.51,  detection_ns: 147.51 },
    ResultsTableRow { processes: 10,  banker_ns: 931.40,  detection_ns: 201.79 },
    ResultsTableRow { processes: 20,  banker_ns: 1833.23, detection_ns: 289.76 },
    ResultsTableRow { processes: 50,  banker_ns: 4070.39, detection_ns: 536.60 },
    ResultsTableRow { processes: 100, banker_ns: 8117.86, detection_ns: 907.88 },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
    pub fn contains(&self, pt: Point) -> bool {
        pt.x >= self.x && pt.x < self.x + self.width as i32 && pt.y >= self.y && pt.y < self.bottom()
    }
}

pub fn calculate_chart_point(
    chart_rect: Rect,
    processes: usize,
    time_ns: f32,
    max_processes: usize,
    max_time_ns: f32,
) -> Point {
    let padding_left = 40i32;
    let padding_right = 16i32;
    let padding_top = 16i32;
    let padding_bottom = 24i32;

    let plot_w = chart_rect.width.saturating_sub((padding_left + padding_right) as u32) as f32;
    let plot_h = chart_rect.height.saturating_sub((padding_top + padding_bottom) as u32) as f32;

    let norm_x = (processes as f32) / (max_processes as f32);
    let norm_y = (time_ns / max_time_ns).clamp(0.0, 1.0);

    let px = chart_rect.x + padding_left + (norm_x * plot_w) as i32;
    let py = chart_rect.bottom() - padding_bottom - (norm_y * plot_h) as i32;

    Point { x: px, y: py }
}

#[test]
fn test_host_research_results_table_values() {
    assert_eq!(RESEARCH_RESULTS_TABLE.len(), 5);
    assert_eq!(RESEARCH_RESULTS_TABLE[0].processes, 5);
    assert_eq!(RESEARCH_RESULTS_TABLE[4].processes, 100);
    assert_eq!(RESEARCH_RESULTS_TABLE[0].banker_ns, 569.51);
    assert_eq!(RESEARCH_RESULTS_TABLE[4].banker_ns, 8117.86);
    assert_eq!(RESEARCH_RESULTS_TABLE[0].detection_ns, 147.51);
    assert_eq!(RESEARCH_RESULTS_TABLE[4].detection_ns, 907.88);
}

#[test]
fn test_host_chart_coordinate_calculations() {
    let chart_rect = Rect::new(100, 100, 400, 200);
    let pt_5 = calculate_chart_point(chart_rect, 5, 569.51, 100, 9000.0);
    let pt_100 = calculate_chart_point(chart_rect, 100, 8117.86, 100, 9000.0);

    assert!(chart_rect.contains(pt_5));
    assert!(chart_rect.contains(pt_100));
    assert!(pt_100.x > pt_5.x);
    assert!(pt_100.y < pt_5.y);
}
