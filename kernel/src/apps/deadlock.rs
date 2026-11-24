//! Micro-OS Deadlock Research Lab Application ("Deadlock Research Lab — R14").
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! Provides an interactive desktop diagnostic suite with Cosmic Slate styling:
//! 1. OVERVIEW: Theoretical concepts, comparative mechanics, and research objectives.
//! 2. BANKER'S ALGORITHM: Live matrices (Available, Allocation, Need), safe sequence derivation,
//!    and the exact-fit boundary condition demonstration (`need > work` vs. `need >= work`).
//! 3. DEADLOCK DETECTION: Multi-resource request matrices, zero-allocation filtering,
//!    iterative reduction progress, and circular wait cycle detection ($P_0 \leftrightarrow P_1$).
//! 4. BENCHMARKS: Deterministic workloads for 5, 10, 20, 50, and 100 processes;
//!    strict separation of preserved original research nanoseconds vs. live MicroOS CPU cycles.
//! 5. RESULTS & LINE GRAPH: Preserved research results table and custom 2D vector chart
//!    plotting execution-time scaling curves.
//! 6. ANALYSIS: Algorithmic complexity ($O(m \cdot n^2)$), inline vs. periodic invocation frequency,
//!    and the fundamental policy trade-off between avoidance and detection.

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use spin::Mutex;

use crate::deadlock::benchmark::{run_microos_benchmarks, MicroOsBenchmarkMeasurement};
use crate::deadlock::workload::WORKLOAD_SIZES;
use crate::graphics::color::Color;
use crate::graphics::font::{draw_string, measure_string};
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::{
    draw_icon, Icon, ICON_ACTIVITY, ICON_CHECK, ICON_CLOSE, ICON_DEADLOCK, ICON_GEAR, ICON_STORE,
    ICON_TERMINAL,
};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Application window title.
pub const DEADLOCK_WINDOW_TITLE: &'static str = "Deadlock Research Lab — R14";

/// Global spinlock-protected Deadlock Research Lab application state.
pub static DEADLOCK_APP: Mutex<DeadlockApp> = Mutex::new(DeadlockApp::new());

/// Preserved original research results table required by Topic R14.
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

/// The six diagnostic views of the Deadlock Research Lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockSection {
    Overview,
    Banker,
    Detection,
    Benchmarks,
    Results,
    Analysis,
}

impl DeadlockSection {
    pub const ALL: [DeadlockSection; 6] = [
        DeadlockSection::Overview,
        DeadlockSection::Banker,
        DeadlockSection::Detection,
        DeadlockSection::Benchmarks,
        DeadlockSection::Results,
        DeadlockSection::Analysis,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            DeadlockSection::Overview => "Overview",
            DeadlockSection::Banker => "Banker's",
            DeadlockSection::Detection => "Detection",
            DeadlockSection::Benchmarks => "Benchmarks",
            DeadlockSection::Results => "Results Chart",
            DeadlockSection::Analysis => "Analysis",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            DeadlockSection::Overview => &ICON_ACTIVITY,
            DeadlockSection::Banker => &ICON_CHECK,
            DeadlockSection::Detection => &ICON_CLOSE,
            DeadlockSection::Benchmarks => &ICON_TERMINAL,
            DeadlockSection::Results => &ICON_STORE,
            DeadlockSection::Analysis => &ICON_GEAR,
        }
    }
}

/// Deadlock Research Lab Application state machine.
pub struct DeadlockApp {
    /// Currently active section tab.
    pub current_section: DeadlockSection,
    /// Sidebar active selection index.
    pub selected_sidebar_index: usize,
    /// Whether the Banker's tab is showing the CA-3 exact-fit boundary condition defect demo.
    pub exact_fit_demo_enabled: bool,
    /// Whether the Deadlock Detection tab is showing the circular wait scenario ($P_0 \leftrightarrow P_1$).
    pub detection_cycle_enabled: bool,
    /// Currently selected workload process count ($5, 10, 20, 50, 100$).
    pub selected_workload_processes: usize,
    /// Live in-kernel CPU cycle measurements captured via `rdtsc`.
    pub live_measurements: Option<Vec<MicroOsBenchmarkMeasurement>>,
    /// Status message displayed at the bottom of the window.
    pub status_message: &'static str,
}

impl DeadlockApp {
    /// Creates a new default DeadlockApp instance.
    pub const fn new() -> Self {
        Self {
            current_section: DeadlockSection::Overview,
            selected_sidebar_index: 0,
            exact_fit_demo_enabled: false,
            detection_cycle_enabled: false,
            selected_workload_processes: 5,
            live_measurements: None,
            status_message: "Ready. Select a tab to explore Topic R14.",
        }
    }

    /// Selects an explicit section tab.
    pub fn select_section(&mut self, section: DeadlockSection) {
        self.current_section = section;
        if let Some(idx) = DeadlockSection::ALL.iter().position(|&s| s == section) {
            self.selected_sidebar_index = idx;
        }
    }

    /// Toggles the exact-fit boundary demo on the Banker's tab.
    pub fn toggle_exact_fit_demo(&mut self) {
        self.exact_fit_demo_enabled = !self.exact_fit_demo_enabled;
        self.status_message = if self.exact_fit_demo_enabled {
            "Showing CA-3 Exact-Fit Boundary Demo (need == work)."
        } else {
            "Showing Standard 5-Process Safe State Snapshot."
        };
    }

    /// Toggles the circular wait scenario on the Detection tab.
    pub fn toggle_detection_scenario(&mut self) {
        self.detection_cycle_enabled = !self.detection_cycle_enabled;
        self.status_message = if self.detection_cycle_enabled {
            "Showing Circular Wait Deadlock Scenario (P0 <-> P1)."
        } else {
            "Showing Resolvable Progress Path (No Deadlock)."
        };
    }

    /// Executes live in-kernel CPU cycle benchmarks via `rdtsc`.
    pub fn run_in_kernel_benchmarks(&mut self) {
        self.status_message = "Executing in-kernel rdtsc benchmark across 5-100 processes...";
        let results = run_microos_benchmarks();
        self.live_measurements = Some(results);
        self.status_message = "Bare-metal benchmark complete. Exact CPU cycles recorded.";
    }

    /// Handles mouse clicks within the client area.
    pub fn handle_click(&mut self, pt: Point, content_rect: Rect) -> bool {
        let sidebar_w = 130i32;

        // 1. Sidebar tab clicks
        if pt.x >= content_rect.x && pt.x < content_rect.x + sidebar_w {
            let start_y = content_rect.y + 36;
            let item_h = 32i32;

            for (i, &sec) in DeadlockSection::ALL.iter().enumerate() {
                let cur_y = start_y + (i as i32 * item_h);
                if pt.y >= cur_y && pt.y < cur_y + item_h {
                    self.select_section(sec);
                    return true;
                }
            }
            return false;
        }

        // 2. Main content interaction
        if pt.x >= content_rect.x + sidebar_w {
            let main_x = content_rect.x + sidebar_w + 12;
            let _main_w = content_rect.width.saturating_sub(sidebar_w as u32 + 24);

            match self.current_section {
                DeadlockSection::Banker => {
                    // Toggle button for Exact-Fit Demo
                    let btn_rect = Rect::new(main_x, content_rect.bottom() - 44, 250, 28);
                    if btn_rect.contains(pt) {
                        self.toggle_exact_fit_demo();
                        return true;
                    }
                }
                DeadlockSection::Detection => {
                    // Toggle button for Circular Wait vs. Resolvable
                    let btn_rect = Rect::new(main_x, content_rect.bottom() - 44, 260, 28);
                    if btn_rect.contains(pt) {
                        self.toggle_detection_scenario();
                        return true;
                    }
                }
                DeadlockSection::Benchmarks => {
                    // Workload selector pills: 5, 10, 20, 50, 100
                    let pill_y = content_rect.y + 44;
                    let pill_w = 64i32;
                    let pill_h = 24i32;

                    for (i, &size) in WORKLOAD_SIZES.iter().enumerate() {
                        let px = main_x + (i as i32 * (pill_w + 8));
                        let pill_rect = Rect::new(px, pill_y, pill_w as u32, pill_h as u32);
                        if pill_rect.contains(pt) {
                            self.selected_workload_processes = size;
                            self.status_message = "Selected workload size. Click Run to measure cycles.";
                            return true;
                        }
                    }

                    // Run In-Kernel Benchmark button
                    let run_btn_rect = Rect::new(main_x, content_rect.bottom() - 44, 240, 28);
                    if run_btn_rect.contains(pt) {
                        self.run_in_kernel_benchmarks();
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Handles keyboard events for the application.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.action != KeyAction::Pressed {
            return false;
        }

        match key.code {
            KeyCode::Down => {
                let next_idx = (self.selected_sidebar_index + 1) % DeadlockSection::ALL.len();
                self.selected_sidebar_index = next_idx;
                self.current_section = DeadlockSection::ALL[next_idx];
                true
            }
            KeyCode::Up => {
                let prev_idx = if self.selected_sidebar_index == 0 {
                    DeadlockSection::ALL.len() - 1
                } else {
                    self.selected_sidebar_index - 1
                };
                self.selected_sidebar_index = prev_idx;
                self.current_section = DeadlockSection::ALL[prev_idx];
                true
            }
            KeyCode::T => {
                if self.current_section == DeadlockSection::Banker {
                    self.toggle_exact_fit_demo();
                    return true;
                } else if self.current_section == DeadlockSection::Detection {
                    self.toggle_detection_scenario();
                    return true;
                }
                false
            }
            KeyCode::R => {
                if self.current_section == DeadlockSection::Benchmarks {
                    self.run_in_kernel_benchmarks();
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// Global dispatcher for mouse clicks on the Deadlock window client area.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    DEADLOCK_APP.lock().handle_click(pt, content_rect)
}

/// Global dispatcher for keyboard events targeted at the Deadlock window.
pub fn handle_key(key: KeyEvent) -> bool {
    DEADLOCK_APP.lock().handle_key(key)
}

/// Helper function to calculate a 2D line graph point within a given chart canvas rectangle.
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

    Point::new(px, py)
}

/// Opens or focuses the Deadlock Research Lab window on the desktop.
pub fn open_or_focus_deadlock_window(wm: &mut WindowManager) {
    let existing_id = wm
        .windows()
        .iter()
        .find(|w| w.title() == DEADLOCK_WINDOW_TITLE)
        .map(|w| w.id());

    if let Some(id) = existing_id {
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[Deadlock] Activated existing Deadlock Research Lab window (ID: {:?})", id);
    } else {
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 660u32.min(screen_w.saturating_sub(40));
        let win_h = 440u32.min(screen_h.saturating_sub(80));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 90i32;

        let deadlock_win = Window::new(
            WindowId::new(8),
            DEADLOCK_WINDOW_TITLE,
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_DEADLOCK)
        .with_min_size(520, 360)
        .with_content_renderer(render_deadlock_content);

        wm.add_window(deadlock_win);
        wm.set_active(WindowId::new(8));
        log::info!("[Deadlock] Spawned new Deadlock Research Lab window (ID: 8)");
    }
}

/// Top-level client content renderer for the Deadlock Research Lab window.
pub fn render_deadlock_content(_window: &Window, surface: &mut Surface, content_rect: Rect) {
    let app = DEADLOCK_APP.lock();

    let saved_clip = surface.clip_rect();
    let client_clip = match saved_clip.intersect(&content_rect) {
        Some(c) => c,
        None => return,
    };
    surface.set_clip_rect(client_clip);

    // 1. Fill base client background (Obsidian Navy)
    surface.fill_rect(content_rect, Color::new(0x0E, 0x14, 0x24));

    // 2. Render Left Navigation Sidebar (Width: 130 px)
    let sidebar_w = 130u32;
    let sidebar_rect = Rect::new(content_rect.x, content_rect.y, sidebar_w, content_rect.height);
    surface.fill_rect(sidebar_rect, Color::new(0x0B, 0x10, 0x1C));
    surface.draw_vline(
        content_rect.x + sidebar_w as i32,
        content_rect.y,
        content_rect.height,
        Color::BORDER_SLATE,
    );

    // Sidebar Title
    draw_string(surface, content_rect.x + 12, content_rect.y + 12, "RESEARCH R14", Color::ACCENT_CYAN, None, 1);

    // Sidebar Items
    let item_start_y = content_rect.y + 36;
    let item_h = 32i32;

    for (i, &sec) in DeadlockSection::ALL.iter().enumerate() {
        let cur_y = item_start_y + (i as i32 * item_h);
        if cur_y + item_h > content_rect.bottom() {
            break;
        }

        let is_selected = app.current_section == sec;
        if is_selected {
            let active_rect = Rect::new(content_rect.x + 6, cur_y + 2, sidebar_w - 12, 28);
            surface.fill_rounded_rect(active_rect, 5, Color::new(0x1B, 0x25, 0x38));
            surface.draw_vline(content_rect.x + 8, cur_y + 6, 20, Color::ACCENT_CYAN);
        }

        let label_color = if is_selected { Color::ACCENT_CYAN } else { Color::TEXT_LIGHT };
        let icon_tint = if is_selected { Some(Color::ACCENT_CYAN) } else { Some(Color::TEXT_MUTED) };

        draw_icon(surface, content_rect.x + 14, cur_y + 8, sec.icon(), icon_tint);
        draw_string(surface, content_rect.x + 36, cur_y + 10, sec.title(), label_color, None, 1);
    }

    // 3. Render Status Bar at bottom
    let status_bar_h = 24i32;
    let status_rect = Rect::new(
        content_rect.x + sidebar_w as i32 + 1,
        content_rect.bottom() - status_bar_h,
        content_rect.width.saturating_sub(sidebar_w + 1),
        status_bar_h as u32,
    );
    surface.fill_rect(status_rect, Color::new(0x09, 0x0D, 0x17));
    surface.draw_hline(status_rect.x, status_rect.y, status_rect.width, Color::BORDER_SLATE);
    draw_string(surface, status_rect.x + 10, status_rect.y + 7, app.status_message, Color::TEXT_MUTED, None, 1);

    // 4. Render Main Section Content Area
    let main_x = content_rect.x + sidebar_w as i32 + 1;
    let main_w = content_rect.width.saturating_sub(sidebar_w + 1);
    let main_h = content_rect.height.saturating_sub(status_bar_h as u32);
    let main_rect = Rect::new(main_x, content_rect.y, main_w, main_h);

    match app.current_section {
        DeadlockSection::Overview => render_overview_section(surface, main_rect),
        DeadlockSection::Banker => render_banker_section(surface, main_rect, &app),
        DeadlockSection::Detection => render_detection_section(surface, main_rect, &app),
        DeadlockSection::Benchmarks => render_benchmarks_section(surface, main_rect, &app),
        DeadlockSection::Results => render_results_section(surface, main_rect),
        DeadlockSection::Analysis => render_analysis_section(surface, main_rect),
    }

    surface.set_clip_rect(saved_clip);
}

/// SECTION 1: OVERVIEW
fn render_overview_section(surface: &mut Surface, rect: Rect) {
    let padding = 16i32;
    let text_x = rect.x + padding;
    let mut cur_y = rect.y + 14;

    draw_string(surface, text_x, cur_y, "Deadlock Avoidance vs. Detection", Color::TEXT_LIGHT, None, 1);
    cur_y += 14;
    draw_string(
        surface,
        text_x,
        cur_y,
        "Operating Systems Research Activity R14 | Sukhi R.M.",
        Color::ACCENT_AZURE,
        None,
        1,
    );
    cur_y += 18;

    surface.draw_hline(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), Color::BORDER_SLATE);
    cur_y += 14;

    // Card 1: Deadlock Avoidance
    let card_w = (rect.width.saturating_sub((padding * 2) as u32 + 12)) / 2;
    let card1_rect = Rect::new(text_x, cur_y, card_w, 130);
    surface.fill_rounded_rect(card1_rect, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card1_rect, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card1_rect.x + 12, card1_rect.y + 10, "1. DEADLOCK AVOIDANCE", Color::ACCENT_CYAN, None, 1);
    draw_string(surface, card1_rect.x + 12, card1_rect.y + 26, "Algorithm: Dijkstra's Banker's", Color::TEXT_LIGHT, None, 1);
    draw_string(surface, card1_rect.x + 12, card1_rect.y + 44, "Policy: Proactive / Pessimistic", Color::TEXT_MUTED, None, 1);
    draw_string(surface, card1_rect.x + 12, card1_rect.y + 60, "Mechanism: Inspects maximum claims (Max)", Color::TEXT_MUTED, None, 1);
    draw_string(surface, card1_rect.x + 12, card1_rect.y + 76, "Guarantee: System never enters unsafe state", Color::SUCCESS_GREEN, None, 1);
    draw_string(surface, card1_rect.x + 12, card1_rect.y + 92, "Overhead: O(m*n^2) on every request", Color::WARNING_AMBER, None, 1);

    // Card 2: Deadlock Detection
    let card2_rect = Rect::new(text_x + card_w as i32 + 12, cur_y, card_w, 130);
    surface.fill_rounded_rect(card2_rect, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card2_rect, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card2_rect.x + 12, card2_rect.y + 10, "2. DEADLOCK DETECTION", Color::ACCENT_AZURE, None, 1);
    draw_string(surface, card2_rect.x + 12, card2_rect.y + 26, "Algorithm: Matrix Reduction / WFG", Color::TEXT_LIGHT, None, 1);
    draw_string(surface, card2_rect.x + 12, card2_rect.y + 44, "Policy: Reactive / Optimistic", Color::TEXT_MUTED, None, 1);
    draw_string(surface, card2_rect.x + 12, card2_rect.y + 60, "Mechanism: Periodically inspects Requests", Color::TEXT_MUTED, None, 1);
    draw_string(surface, card2_rect.x + 12, card2_rect.y + 76, "Guarantee: Identifies existing circular waits", Color::SUCCESS_GREEN, None, 1);
    draw_string(surface, card2_rect.x + 12, card2_rect.y + 92, "Overhead: Amortized; recovery required", Color::WARNING_AMBER, None, 1);

    cur_y += 144;

    // Card 3: Research Objective & Scope
    let card3_rect = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 110);
    surface.fill_rounded_rect(card3_rect, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card3_rect, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card3_rect.x + 12, card3_rect.y + 10, "RESEARCH OBJECTIVE & METHODOLOGY", Color::WHITE, None, 1);
    draw_string(
        surface,
        card3_rect.x + 12,
        card3_rect.y + 28,
        "Compare empirical execution times across increasing concurrency tiers (5 to 100 processes).",
        Color::TEXT_LIGHT,
        None,
        1,
    );
    draw_string(
        surface,
        card3_rect.x + 12,
        card3_rect.y + 46,
        "Investigate the critical exact-fit boundary condition: Need == Work must be safely accepted.",
        Color::TEXT_MUTED,
        None,
        1,
    );
    draw_string(
        surface,
        card3_rect.x + 12,
        card3_rect.y + 64,
        "Assess trade-offs: Avoidance prevents deadlocks but burdens every grant; Detection allows fast grants",
        Color::TEXT_MUTED,
        None,
        1,
    );
    draw_string(
        surface,
        card3_rect.x + 12,
        card3_rect.y + 80,
        "but demands expensive process termination or rollback when circular wait cycles materialize.",
        Color::TEXT_MUTED,
        None,
        1,
    );
}

/// SECTION 2: BANKER'S ALGORITHM
fn render_banker_section(surface: &mut Surface, rect: Rect, app: &DeadlockApp) {
    let padding = 16i32;
    let text_x = rect.x + padding;
    let mut cur_y = rect.y + 14;

    draw_string(surface, text_x, cur_y, "Dijkstra's Banker's Algorithm", Color::TEXT_LIGHT, None, 1);
    cur_y += 14;
    draw_string(
        surface,
        text_x,
        cur_y,
        "Avoidance Safety Check: Need = Max - Allocation <= Work",
        Color::ACCENT_CYAN,
        None,
        1,
    );
    cur_y += 18;

    surface.draw_hline(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), Color::BORDER_SLATE);
    cur_y += 12;

    if app.exact_fit_demo_enabled {
        // EXACT FIT DEMONSTRATION
        let demo_rect = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 220);
        surface.fill_rounded_rect(demo_rect, 8, Color::CARD_SLATE);
        surface.stroke_rounded_rect(demo_rect, 8, 1, Color::ACCENT_CYAN);

        draw_string(surface, demo_rect.x + 12, demo_rect.y + 10, "CA-3 EXACT-FIT BOUNDARY CONDITION DEMONSTRATION", Color::ACCENT_CYAN, None, 1);
        draw_string(surface, demo_rect.x + 12, demo_rect.y + 28, "System Snapshot: Available = [2], Allocation = [[0]], Need = [[2]]", Color::TEXT_LIGHT, None, 1);

        // Defective Box
        let box1 = Rect::new(demo_rect.x + 12, demo_rect.y + 50, (demo_rect.width - 36) / 2, 70);
        surface.fill_rounded_rect(box1, 6, Color::new(0x28, 0x14, 0x14));
        surface.stroke_rounded_rect(box1, 6, 1, Color::DANGER_RED);
        draw_string(surface, box1.x + 10, box1.y + 8, "DEFECTIVE: if need[p][r] >= work[r]", Color::DANGER_RED, None, 1);
        draw_string(surface, box1.x + 10, box1.y + 26, "Evaluation: UNSAFE (Defect Reproduced)", Color::WHITE, None, 1);
        draw_string(surface, box1.x + 10, box1.y + 44, "Rejects exact-fit; falsely claims unsafe!", Color::TEXT_MUTED, None, 1);

        // Corrected Box
        let box2 = Rect::new(box1.right() + 12, demo_rect.y + 50, box1.width, 70);
        surface.fill_rounded_rect(box2, 6, Color::new(0x14, 0x28, 0x1C));
        surface.stroke_rounded_rect(box2, 6, 1, Color::SUCCESS_GREEN);
        draw_string(surface, box2.x + 10, box2.y + 8, "CORRECTED: if need[p][r] > work[r]", Color::SUCCESS_GREEN, None, 1);
        draw_string(surface, box2.x + 10, box2.y + 26, "Evaluation: SAFE (Sequence: P0)", Color::WHITE, None, 1);
        draw_string(surface, box2.x + 10, box2.y + 44, "Accepts exact-fit; process completes safely.", Color::TEXT_MUTED, None, 1);

        draw_string(
            surface,
            demo_rect.x + 12,
            demo_rect.y + 132,
            "Mathematical Proof: When Need == Work, idle units exactly satisfy remaining claim.",
            Color::TEXT_LIGHT,
            None,
            1,
        );
        draw_string(
            surface,
            demo_rect.x + 12,
            demo_rect.y + 150,
            "P0 finishes and returns its allocation to Work: Work <- Work + Allocation.",
            Color::TEXT_MUTED,
            None,
            1,
        );
        draw_string(
            surface,
            demo_rect.x + 12,
            demo_rect.y + 168,
            "Status: In-kernel safety check verified with zero allocation rejection.",
            Color::SUCCESS_GREEN,
            None,
            1,
        );
    } else {
        // STANDARD 5-PROCESS SAFE SNAPSHOT
        let matrix_card = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 230);
        surface.fill_rounded_rect(matrix_card, 8, Color::CARD_SLATE);
        surface.stroke_rounded_rect(matrix_card, 8, 1, Color::BORDER_SLATE);

        draw_string(surface, matrix_card.x + 12, matrix_card.y + 10, "5-PROCESS ALLOCATION & NEED MATRIX SNAPSHOT", Color::WHITE, None, 1);
        draw_string(surface, matrix_card.x + 12, matrix_card.y + 26, "Available Resources: [R0: 3, R1: 3, R2: 2]", Color::ACCENT_AZURE, None, 1);

        // Header
        let row_y = matrix_card.y + 46;
        draw_string(surface, matrix_card.x + 14, row_y, "Process | Allocation (R0 R1 R2) | Max Claim | Need Matrix", Color::TEXT_MUTED, None, 1);
        surface.draw_hline(matrix_card.x + 12, row_y + 12, matrix_card.width - 24, Color::BORDER_SLATE);

        let data = [
            ("P0", "0  1  0", "7  5  3", "7  4  3"),
            ("P1", "2  0  0", "3  2  2", "1  2  2"),
            ("P2", "3  0  2", "9  0  2", "6  0  0"),
            ("P3", "2  1  1", "2  2  2", "0  1  1"),
            ("P4", "0  0  2", "4  3  3", "4  3  1"),
        ];

        for (i, (p, alloc, max, need)) in data.iter().enumerate() {
            let py = row_y + 18 + (i as i32 * 18);
            let line = format!("  {}    |    {}    |   {}   |   {}", p, alloc, max, need);
            draw_string(surface, matrix_card.x + 14, py, &line, Color::TEXT_LIGHT, None, 1);
        }

        // Safe Sequence Card
        let seq_y = matrix_card.y + 154;
        surface.fill_rounded_rect(Rect::new(matrix_card.x + 12, seq_y, matrix_card.width - 24, 34), 6, Color::new(0x13, 0x22, 0x24));
        draw_string(surface, matrix_card.x + 20, seq_y + 6, "STATUS: SAFE STATE", Color::SUCCESS_GREEN, None, 1);
        draw_string(surface, matrix_card.x + 20, seq_y + 18, "Safe Execution Sequence: P1 -> P3 -> P4 -> P0 -> P2", Color::ACCENT_CYAN, None, 1);

        draw_string(surface, matrix_card.x + 12, matrix_card.y + 200, "All processes successfully reduce without deadlock risk.", Color::TEXT_MUTED, None, 1);
    }

    // Toggle Button
    let btn_rect = Rect::new(text_x, rect.bottom() - 44, 250, 28);
    surface.fill_rounded_rect(btn_rect, 6, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(btn_rect, 6, 1, Color::ACCENT_CYAN);
    let btn_text = if app.exact_fit_demo_enabled {
        "<- View 5-Process Safe State"
    } else {
        "View CA-3 Exact-Fit Defect Demo ->"
    };
    draw_string(surface, btn_rect.x + 12, btn_rect.y + 8, btn_text, Color::ACCENT_CYAN, None, 1);
}

/// SECTION 3: DEADLOCK DETECTION
fn render_detection_section(surface: &mut Surface, rect: Rect, app: &DeadlockApp) {
    let padding = 16i32;
    let text_x = rect.x + padding;
    let mut cur_y = rect.y + 14;

    draw_string(surface, text_x, cur_y, "Resource-Request Deadlock Detection", Color::TEXT_LIGHT, None, 1);
    cur_y += 14;
    draw_string(
        surface,
        text_x,
        cur_y,
        "Matrix Reduction Algorithm: Equivalent to Wait-For Graph knot analysis",
        Color::ACCENT_AZURE,
        None,
        1,
    );
    cur_y += 18;

    surface.draw_hline(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), Color::BORDER_SLATE);
    cur_y += 12;

    let det_card = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 220);
    surface.fill_rounded_rect(det_card, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(det_card, 8, 1, Color::BORDER_SLATE);

    if app.detection_cycle_enabled {
        // DEADLOCK CYCLE SCENARIO (P0 <-> P1)
        draw_string(surface, det_card.x + 12, det_card.y + 10, "SCENARIO: CIRCULAR WAIT CYCLE (P0 <-> P1)", Color::DANGER_RED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 28, "Available: [0]  |  Allocation: P0=[1], P1=[1]  |  Request: P0=[1], P1=[1]", Color::TEXT_LIGHT, None, 1);

        let badge_rect = Rect::new(det_card.x + 12, det_card.y + 54, det_card.width - 24, 46);
        surface.fill_rounded_rect(badge_rect, 6, Color::new(0x28, 0x14, 0x14));
        surface.stroke_rounded_rect(badge_rect, 6, 1, Color::DANGER_RED);
        draw_string(surface, badge_rect.x + 12, badge_rect.y + 8, "DETECTION RESULT: DEADLOCK CONFIRMED", Color::DANGER_RED, None, 1);
        draw_string(surface, badge_rect.x + 12, badge_rect.y + 24, "Deadlocked Process Set: { P0, P1 }", Color::WHITE, None, 1);

        draw_string(surface, det_card.x + 12, det_card.y + 116, "Graph Topology Analysis:", Color::WHITE, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 134, "  1. P0 holds R0, blocked requesting R1 held by P1.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 150, "  2. P1 holds R1, blocked requesting R0 held by P0.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 166, "  3. No process holds zero resources; no request <= Available.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 184, "Remediation: Operating system must abort P0 or P1 to break cycle.", Color::WARNING_AMBER, None, 1);
    } else {
        // RESOLVABLE SYSTEM (NO DEADLOCK)
        draw_string(surface, det_card.x + 12, det_card.y + 10, "SCENARIO: RESOLVABLE PROGRESS PATH (NO DEADLOCK)", Color::SUCCESS_GREEN, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 28, "Available: [0]  |  Allocation: P0=[1], P1=[1]  |  Request: P0=[0], P1=[1]", Color::TEXT_LIGHT, None, 1);

        let badge_rect = Rect::new(det_card.x + 12, det_card.y + 54, det_card.width - 24, 46);
        surface.fill_rounded_rect(badge_rect, 6, Color::new(0x14, 0x28, 0x1C));
        surface.stroke_rounded_rect(badge_rect, 6, 1, Color::SUCCESS_GREEN);
        draw_string(surface, badge_rect.x + 12, badge_rect.y + 8, "DETECTION RESULT: NO DEADLOCK DETECTED", Color::SUCCESS_GREEN, None, 1);
        draw_string(surface, badge_rect.x + 12, badge_rect.y + 24, "Deadlocked Process Set: { } (Empty)", Color::WHITE, None, 1);

        draw_string(surface, det_card.x + 12, det_card.y + 116, "Graph Reduction Progression:", Color::WHITE, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 134, "  1. P0 has 0 outstanding requests (Request[0] <= Work). P0 finishes.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 150, "  2. P0 releases its allocation: Work <- Work + Allocation[0] = [1].", Color::TEXT_MUTED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 166, "  3. Now Request[1] <= Work ([1] <= [1]); P1 finishes safely.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, det_card.x + 12, det_card.y + 184, "Conclusion: Graph completely reduced; zero victim aborts required.", Color::SUCCESS_GREEN, None, 1);
    }

    // Toggle Button
    let btn_rect = Rect::new(text_x, rect.bottom() - 44, 260, 28);
    surface.fill_rounded_rect(btn_rect, 6, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(btn_rect, 6, 1, Color::ACCENT_AZURE);
    let btn_text = if app.detection_cycle_enabled {
        "<- View Resolvable Path"
    } else {
        "View Circular Wait (P0 <-> P1) ->"
    };
    draw_string(surface, btn_rect.x + 12, btn_rect.y + 8, btn_text, Color::ACCENT_AZURE, None, 1);
}

/// SECTION 4: BENCHMARKS
fn render_benchmarks_section(surface: &mut Surface, rect: Rect, app: &DeadlockApp) {
    let padding = 16i32;
    let text_x = rect.x + padding;
    let mut cur_y = rect.y + 14;

    draw_string(surface, text_x, cur_y, "Deterministic Workload Benchmarks", Color::TEXT_LIGHT, None, 1);
    cur_y += 14;
    draw_string(
        surface,
        text_x,
        cur_y,
        "Separation of Preserved Host Nanoseconds vs. Live MicroOS CPU Cycles",
        Color::ACCENT_CYAN,
        None,
        1,
    );
    cur_y += 18;

    surface.draw_hline(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), Color::BORDER_SLATE);
    cur_y += 10;

    // Workload Size Selector Pills
    draw_string(surface, text_x, cur_y, "Select Concurrency Tier:", Color::TEXT_MUTED, None, 1);
    let pill_y = cur_y + 14;
    let pill_w = 64i32;
    let pill_h = 24i32;

    for (i, &size) in WORKLOAD_SIZES.iter().enumerate() {
        let px = text_x + (i as i32 * (pill_w + 8));
        let is_selected = app.selected_workload_processes == size;
        let p_rect = Rect::new(px, pill_y, pill_w as u32, pill_h as u32);

        let bg_col = if is_selected { Color::new(0x1B, 0x34, 0x48) } else { Color::SURFACE_SLATE };
        let border_col = if is_selected { Color::ACCENT_CYAN } else { Color::BORDER_SLATE };
        surface.fill_rounded_rect(p_rect, 5, bg_col);
        surface.stroke_rounded_rect(p_rect, 5, 1, border_col);

        let label = format!("N={}", size);
        let (lw, _) = measure_string(&label, 1);
        let lx = px + (pill_w - lw as i32) / 2;
        draw_string(surface, lx, pill_y + 6, &label, if is_selected { Color::ACCENT_CYAN } else { Color::TEXT_LIGHT }, None, 1);
    }

    cur_y += 48;

    // Card A: Original Research Baseline
    let card_w = (rect.width.saturating_sub((padding * 2) as u32 + 12)) / 2;
    let card_a = Rect::new(text_x, cur_y, card_w, 140);
    surface.fill_rounded_rect(card_a, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card_a, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card_a.x + 10, card_a.y + 8, "ORIGINAL RESEARCH REPORT (Host)", Color::ACCENT_AZURE, None, 1);
    draw_string(surface, card_a.x + 10, card_a.y + 22, "Unit: Nanoseconds (Wall-clock)", Color::TEXT_MUTED, None, 1);

    if let Some(row) = RESEARCH_RESULTS_TABLE.iter().find(|r| r.processes == app.selected_workload_processes) {
        let l1 = format!("Processes:    {}", row.processes);
        let l2 = format!("Banker:       {:.2} ns", row.banker_ns);
        let l3 = format!("Detection:    {:.2} ns", row.detection_ns);
        let l4 = format!("Ratio (B/D):  {:.2}x", row.banker_ns / row.detection_ns);
        draw_string(surface, card_a.x + 10, card_a.y + 46, &l1, Color::WHITE, None, 1);
        draw_string(surface, card_a.x + 10, card_a.y + 66, &l2, Color::ACCENT_CYAN, None, 1);
        draw_string(surface, card_a.x + 10, card_a.y + 86, &l3, Color::ACCENT_AZURE, None, 1);
        draw_string(surface, card_a.x + 10, card_a.y + 106, &l4, Color::SUCCESS_GREEN, None, 1);
    }

    // Card B: Live MicroOS CPU Cycles
    let card_b = Rect::new(text_x + card_w as i32 + 12, cur_y, card_w, 140);
    surface.fill_rounded_rect(card_b, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card_b, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card_b.x + 10, card_b.y + 8, "MICRO-OS BARE-METAL TELEMETRY", Color::ACCENT_CYAN, None, 1);
    draw_string(surface, card_b.x + 10, card_b.y + 22, "Unit: CPU Clock Cycles (_rdtsc)", Color::TEXT_MUTED, None, 1);

    if let Some(meas) = &app.live_measurements {
        if let Some(m) = meas.iter().find(|r| r.processes == app.selected_workload_processes) {
            let l1 = format!("Processes:    {}", m.processes);
            let l2 = format!("Banker:       {} cycles", m.banker_median_cycles);
            let l3 = format!("Detection:    {} cycles", m.detection_median_cycles);
            let l4 = format!("Comparisons:  B={}, D={}", m.banker_comparisons, m.detection_comparisons);
            draw_string(surface, card_b.x + 10, card_b.y + 46, &l1, Color::WHITE, None, 1);
            draw_string(surface, card_b.x + 10, card_b.y + 66, &l2, Color::ACCENT_CYAN, None, 1);
            draw_string(surface, card_b.x + 10, card_b.y + 86, &l3, Color::ACCENT_AZURE, None, 1);
            draw_string(surface, card_b.x + 10, card_b.y + 106, &l4, Color::SUCCESS_GREEN, None, 1);
        }
    } else {
        draw_string(surface, card_b.x + 10, card_b.y + 56, "Live rdtsc not captured yet.", Color::TEXT_MUTED, None, 1);
        draw_string(surface, card_b.x + 10, card_b.y + 76, "Click 'Run In-Kernel Benchmark'", Color::TEXT_LIGHT, None, 1);
        draw_string(surface, card_b.x + 10, card_b.y + 94, "to measure bare-metal cycles.", Color::ACCENT_CYAN, None, 1);
    }

    // Run Button
    let btn_rect = Rect::new(text_x, rect.bottom() - 44, 240, 28);
    surface.fill_rounded_rect(btn_rect, 6, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(btn_rect, 6, 1, Color::ACCENT_CYAN);
    draw_string(surface, btn_rect.x + 12, btn_rect.y + 8, "> Run In-Kernel Benchmark (rdtsc)", Color::ACCENT_CYAN, None, 1);
}

/// SECTION 5: RESULTS & LINE GRAPH
fn render_results_section(surface: &mut Surface, rect: Rect) {
    let padding = 16i32;
    let text_x = rect.x + padding;
    let mut cur_y = rect.y + 12;

    draw_string(surface, text_x, cur_y, "Research Results & Scaling Curves", Color::TEXT_LIGHT, None, 1);
    cur_y += 14;
    draw_string(surface, text_x, cur_y, "Original Research Report Measurements (nanoseconds)", Color::ACCENT_CYAN, None, 1);
    cur_y += 16;

    surface.draw_hline(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), Color::BORDER_SLATE);
    cur_y += 10;

    // 1. Table Card (Left: 220px)
    let table_w = 210u32;
    let card_h = 220u32;
    let table_card = Rect::new(text_x, cur_y, table_w, card_h);
    surface.fill_rounded_rect(table_card, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(table_card, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, table_card.x + 8, table_card.y + 8, "Processes | Banker | Detection", Color::ACCENT_CYAN, None, 1);
    surface.draw_hline(table_card.x + 6, table_card.y + 22, table_card.width - 12, Color::BORDER_SLATE);

    for (i, row) in RESEARCH_RESULTS_TABLE.iter().enumerate() {
        let ry = table_card.y + 30 + (i as i32 * 24);
        let line = format!("{:3} | {:7.1} ns | {:6.1} ns", row.processes, row.banker_ns, row.detection_ns);
        draw_string(surface, table_card.x + 8, ry, &line, Color::TEXT_LIGHT, None, 1);
    }

    draw_string(surface, table_card.x + 8, table_card.y + 158, "Scaling at N=100:", Color::WHITE, None, 1);
    draw_string(surface, table_card.x + 8, table_card.y + 174, "Banker:    8117.86 ns", Color::ACCENT_CYAN, None, 1);
    draw_string(surface, table_card.x + 8, table_card.y + 190, "Detection:  907.88 ns", Color::SUCCESS_GREEN, None, 1);

    // 2. Graphical Line Chart Canvas (Right)
    let chart_x = text_x + table_w as i32 + 12;
    let chart_w = rect.width.saturating_sub(table_w + (padding as u32 * 2) + 12);
    let chart_card = Rect::new(chart_x, cur_y, chart_w, card_h);
    surface.fill_rounded_rect(chart_card, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(chart_card, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, chart_card.x + 10, chart_card.y + 8, "EXECUTION TIME SCALING (ns)", Color::WHITE, None, 1);

    // Chart Legend
    let leg_x = chart_card.right() - 150;
    surface.fill_rect(Rect::new(leg_x, chart_card.y + 8, 8, 8), Color::ACCENT_CYAN);
    draw_string(surface, leg_x + 12, chart_card.y + 8, "Banker", Color::TEXT_LIGHT, None, 1);
    surface.fill_rect(Rect::new(leg_x + 70, chart_card.y + 8, 8, 8), Color::SUCCESS_GREEN);
    draw_string(surface, leg_x + 82, chart_card.y + 8, "Detection", Color::TEXT_LIGHT, None, 1);

    // Inner plot bounds
    let plot_x = chart_card.x + 36;
    let plot_y = chart_card.y + 26;
    let plot_w = chart_card.width.saturating_sub(48);
    let plot_h = chart_card.height.saturating_sub(52);
    let plot_rect = Rect::new(plot_x, plot_y, plot_w, plot_h);

    // Gridlines and Y-axis ticks
    surface.stroke_rect(plot_rect, 1, Color::BORDER_SLATE);
    for tick in 1..4 {
        let ty = plot_rect.bottom() - (tick * (plot_h as i32 / 4));
        surface.draw_hline(plot_rect.x, ty, plot_rect.width, Color::new(0x1B, 0x25, 0x36));
        let y_val = tick * 2250;
        let y_lbl = format!("{}k", y_val / 1000);
        draw_string(surface, chart_card.x + 6, ty - 4, &y_lbl, Color::TEXT_MUTED, None, 1);
    }
    draw_string(surface, chart_card.x + 12, plot_rect.bottom() - 4, "0", Color::TEXT_MUTED, None, 1);

    // X-axis process labels
    for &n in &WORKLOAD_SIZES {
        let pt = calculate_chart_point(plot_rect, n, 0.0, 100, 9000.0);
        let lbl = format!("{}", n);
        let (lw, _) = measure_string(&lbl, 1);
        draw_string(surface, pt.x - (lw as i32 / 2), plot_rect.bottom() + 4, &lbl, Color::TEXT_MUTED, None, 1);
    }

    // Plot Banker's Algorithm Curve (Cyan)
    let max_t = 9000.0f32;
    let mut prev_pt: Option<Point> = None;
    for row in &RESEARCH_RESULTS_TABLE {
        let pt = calculate_chart_point(plot_rect, row.processes, row.banker_ns, 100, max_t);
        if let Some(prev) = prev_pt {
            surface.draw_line(prev, pt, Color::ACCENT_CYAN);
        }
        // Marker dot
        surface.fill_rect(Rect::new(pt.x - 2, pt.y - 2, 5, 5), Color::ACCENT_CYAN);
        prev_pt = Some(pt);
    }

    // Plot Deadlock Detection Curve (Success Green)
    let mut prev_det_pt: Option<Point> = None;
    for row in &RESEARCH_RESULTS_TABLE {
        let pt = calculate_chart_point(plot_rect, row.processes, row.detection_ns, 100, max_t);
        if let Some(prev) = prev_det_pt {
            surface.draw_line(prev, pt, Color::SUCCESS_GREEN);
        }
        // Marker dot
        surface.fill_rect(Rect::new(pt.x - 2, pt.y - 2, 5, 5), Color::SUCCESS_GREEN);
        prev_det_pt = Some(pt);
    }
}

/// SECTION 6: ANALYSIS
fn render_analysis_section(surface: &mut Surface, rect: Rect) {
    let padding = 16i32;
    let text_x = rect.x + padding;
    let mut cur_y = rect.y + 14;

    draw_string(surface, text_x, cur_y, "Analysis & Operating System Trade-offs", Color::TEXT_LIGHT, None, 1);
    cur_y += 14;
    draw_string(
        surface,
        text_x,
        cur_y,
        "Why Deadlock Detection Scales Faster Per Check While Avoidance Eliminates Recovery",
        Color::ACCENT_AZURE,
        None,
        1,
    );
    cur_y += 18;

    surface.draw_hline(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), Color::BORDER_SLATE);
    cur_y += 12;

    let card_w = rect.width.saturating_sub((padding * 2) as u32);
    let card1 = Rect::new(text_x, cur_y, card_w, 100);
    surface.fill_rounded_rect(card1, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card1, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card1.x + 12, card1.y + 10, "1. THE OBSERVED PERFORMANCE DIFFERENCE", Color::ACCENT_CYAN, None, 1);
    draw_string(
        surface,
        card1.x + 12,
        card1.y + 28,
        "Across all workloads (N=5 to 100), Detection runs in 147 ns - 907 ns, while Banker's takes 569 ns - 8117 ns.",
        Color::TEXT_LIGHT,
        None,
        1,
    );
    draw_string(
        surface,
        card1.x + 12,
        card1.y + 46,
        "Detection is ~8.9x faster per snapshot check because processes with zero allocation are immediately pruned,",
        Color::TEXT_MUTED,
        None,
        1,
    );
    draw_string(
        surface,
        card1.x + 12,
        card1.y + 64,
        "and outstanding requests are smaller and more readily satisfied than theoretical worst-case maximum claims.",
        Color::TEXT_MUTED,
        None,
        1,
    );

    cur_y += 112;

    let card2 = Rect::new(text_x, cur_y, card_w, 130);
    surface.fill_rounded_rect(card2, 8, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card2, 8, 1, Color::BORDER_SLATE);

    draw_string(surface, card2.x + 12, card2.y + 10, "2. THE ARCHITECTURAL TRADE-OFF MATRIX", Color::WHITE, None, 1);

    let rows = [
        ("Criterion", "Banker's Avoidance", "Deadlock Detection"),
        ("Invocation Frequency", "Every resource grant (Inline)", "Periodic / Sleep / Threshold"),
        ("Advance Knowledge", "Requires Max claim upfront", "No advance knowledge required"),
        ("Deadlock Prevention", "100% Guaranteed (No deadlocks)", "Permits deadlocks to occur"),
        ("Recovery Overhead", "Zero (No rollback/aborts)", "Expensive (Victim kill / rollback)"),
    ];

    for (i, (c1, c2, c3)) in rows.iter().enumerate() {
        let ry = card2.y + 28 + (i as i32 * 18);
        let color = if i == 0 { Color::ACCENT_AZURE } else { Color::TEXT_LIGHT };
        let line = format!("  {:<22} | {:<25} | {:<25}", c1, c2, c3);
        draw_string(surface, card2.x + 10, ry, &line, color, None, 1);
    }
}
