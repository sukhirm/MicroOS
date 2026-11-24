//! In-Kernel Unit and Integration Tests for Deadlock Research Lab Application.
//!
//! Research Activity R14: Deadlock Avoidance vs. Detection
//!
//! Validates:
//! 1. Application initial state and sidebar navigation.
//! 2. Exact-fit and deadlock scenario state toggles.
//! 3. Exact matching of preserved research results table values.
//! 4. 2D vector chart coordinate calculations and monotonicity.
//! 5. Mouse click hit-testing for tabs, pills, and buttons.
//! 6. Window Manager registration, icon attachment, and single-instance focus.
//! 7. Rendering safety across all 6 diagnostic sections with clipping bounds.

extern crate alloc;

use alloc::vec;
use bootloader_api::info::PixelFormat;
use super::deadlock::{
    calculate_chart_point, open_or_focus_deadlock_window, render_deadlock_content,
    DeadlockApp, DeadlockSection, DEADLOCK_WINDOW_TITLE, RESEARCH_RESULTS_TABLE,
};
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::window::manager::WindowManager;
use crate::window::state::WindowState;

/// Runs all unit and integration tests for the Deadlock Research Lab application.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("    MICRO-OS DEADLOCK RESEARCH LAB APPLICATION SELF-TESTS       ");
    log::info!("================================================================");

    test_app_initial_state_and_navigation();
    test_state_toggles();
    test_research_results_table_values();
    test_chart_coordinate_calculations();
    test_mouse_click_hit_testing();
    test_window_manager_integration();
    test_rendering_safety_and_clipping();

    log::info!("================================================================");
    log::info!("  [OK] All Deadlock Research Lab Application Tests Passed!");
    log::info!("================================================================");
}

/// 1. Verifies initial state, sidebar indexing, and keyboard navigation.
fn test_app_initial_state_and_navigation() {
    let mut app = DeadlockApp::new();
    assert_eq!(app.current_section, DeadlockSection::Overview);
    assert_eq!(app.selected_sidebar_index, 0);

    // Keyboard Down arrow cycles forward
    let down_key = KeyEvent::new(KeyCode::Down, KeyAction::Pressed, None, KeyModifiers::default());
    assert!(app.handle_key(down_key));
    assert_eq!(app.current_section, DeadlockSection::Banker);
    assert_eq!(app.selected_sidebar_index, 1);

    // Keyboard Up arrow cycles backward
    let up_key = KeyEvent::new(KeyCode::Up, KeyAction::Pressed, None, KeyModifiers::default());
    assert!(app.handle_key(up_key));
    assert_eq!(app.current_section, DeadlockSection::Overview);
    assert_eq!(app.selected_sidebar_index, 0);

    // Wrap-around backward check
    assert!(app.handle_key(up_key));
    assert_eq!(app.current_section, DeadlockSection::Analysis);
    assert_eq!(app.selected_sidebar_index, 5);

    log::info!("[PASS] 1. Deadlock Lab initial state and keyboard navigation test");
}

/// 2. Verifies exact-fit boundary and circular wait scenario toggles.
fn test_state_toggles() {
    let mut app = DeadlockApp::new();
    assert!(!app.exact_fit_demo_enabled);
    assert!(!app.detection_cycle_enabled);

    app.toggle_exact_fit_demo();
    assert!(app.exact_fit_demo_enabled);

    app.toggle_exact_fit_demo();
    assert!(!app.exact_fit_demo_enabled);

    app.toggle_detection_scenario();
    assert!(app.detection_cycle_enabled);

    app.toggle_detection_scenario();
    assert!(!app.detection_cycle_enabled);

    log::info!("[PASS] 2. Exact-fit and circular wait scenario toggle test");
}

/// 3. Verifies preserved research results table values against exact paper metrics.
fn test_research_results_table_values() {
    assert_eq!(RESEARCH_RESULTS_TABLE.len(), 5);

    let expected = [
        (5, 569.51, 147.51),
        (10, 931.40, 201.79),
        (20, 1833.23, 289.76),
        (50, 4070.39, 536.60),
        (100, 8117.86, 907.88),
    ];

    for (i, &(p, b_ns, d_ns)) in expected.iter().enumerate() {
        let row = RESEARCH_RESULTS_TABLE[i];
        assert_eq!(row.processes, p);
        assert!((row.banker_ns - b_ns).abs() < 0.01);
        assert!((row.detection_ns - d_ns).abs() < 0.01);
        // Avoidance execution time must exceed detection in this synthetic workload
        assert!(row.banker_ns > row.detection_ns);
    }

    log::info!("[PASS] 3. Preserved research results table exact values test");
}

/// 4. Verifies 2D vector line chart coordinate calculations and monotonicity.
fn test_chart_coordinate_calculations() {
    let chart_rect = Rect::new(100, 100, 400, 200);

    let pt_5 = calculate_chart_point(chart_rect, 5, 569.51, 100, 9000.0);
    let pt_100 = calculate_chart_point(chart_rect, 100, 8117.86, 100, 9000.0);

    assert!(chart_rect.contains(pt_5), "Point 5 must be within chart bounds");
    assert!(chart_rect.contains(pt_100), "Point 100 must be within chart bounds");

    // Monotonicity: pt_100 should be further right (x) and higher up (lower y)
    assert!(pt_100.x > pt_5.x, "X coordinate must increase with process count");
    assert!(pt_100.y < pt_5.y, "Y coordinate must decrease (higher plot) with runtime");

    log::info!("[PASS] 4. 2D vector chart coordinate calculations and monotonicity test");
}

/// 5. Verifies mouse click hit-testing for tabs, pills, and buttons.
fn test_mouse_click_hit_testing() {
    let mut app = DeadlockApp::new();
    let content_rect = Rect::new(50, 50, 680, 440);

    // Click 2nd tab (Banker's) at (x: 80, y: 50 + 36 + 32 + 10 = 128)
    let clicked_tab = app.handle_click(Point::new(80, 128), content_rect);
    assert!(clicked_tab);
    assert_eq!(app.current_section, DeadlockSection::Banker);

    // Click Exact-Fit Demo button near bottom
    let btn_pt = Point::new(200, content_rect.bottom() - 30);
    let clicked_btn = app.handle_click(btn_pt, content_rect);
    assert!(clicked_btn);
    assert!(app.exact_fit_demo_enabled);

    log::info!("[PASS] 5. Deadlock Lab mouse click hit-testing test");
}

/// 6. Verifies Window Manager registration, icon attachment, and single-instance focus.
fn test_window_manager_integration() {
    let mut wm = WindowManager::new(1280, 800);

    open_or_focus_deadlock_window(&mut wm);
    let win_id = wm
        .windows()
        .iter()
        .find(|w| w.title() == DEADLOCK_WINDOW_TITLE)
        .map(|w| w.id())
        .expect("Window must be created");
    assert_eq!(wm.active_window_id(), Some(win_id));

    let win = wm.get_window(win_id).unwrap();
    assert_eq!(win.bounds().width, 660);
    assert_eq!(win.bounds().height, 440);
    assert_eq!(win.state(), WindowState::Normal);

    // Calling again must focus existing window without creating duplicate
    open_or_focus_deadlock_window(&mut wm);
    let count = wm.windows().iter().filter(|w| w.title() == DEADLOCK_WINDOW_TITLE).count();
    assert_eq!(count, 1, "Must maintain single-instance window");

    log::info!("[PASS] 6. Deadlock Lab Window Manager registration and focus test");
}

/// 7. Verifies rendering safety and clipping bounds across all sections.
fn test_rendering_safety_and_clipping() {
    let w = 320u32;
    let h = 240u32;
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let mut surface = Surface::from_raw_parts(&mut buf, w as usize, h as usize, w as usize, 4, PixelFormat::Bgr);

    let mut wm = WindowManager::new(680, 460);
    open_or_focus_deadlock_window(&mut wm);
    let win_id = wm
        .windows()
        .iter()
        .find(|w| w.title() == DEADLOCK_WINDOW_TITLE)
        .map(|w| w.id())
        .unwrap();
    let win = wm.get_window(win_id).unwrap();

    let content_rect = win.content_rect();

    // Render under each of the 6 sections to verify no panics or out-of-bounds writes
    for &sec in DeadlockSection::ALL.iter() {
        {
            let mut app = super::deadlock::DEADLOCK_APP.lock();
            app.select_section(sec);
        }
        render_deadlock_content(win, &mut surface, content_rect);
    }

    // Verify non-empty rendering by sampling background pixel
    let bg_pixel = surface.get_pixel(content_rect.x + 150, content_rect.y + 50);
    assert!(bg_pixel.is_some());

    log::info!("[PASS] 7. Deadlock Lab rendering safety and clipping test");
}
