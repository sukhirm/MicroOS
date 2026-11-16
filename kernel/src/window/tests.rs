//! Milestone 4C Window Manager Self-Tests.
//!
//! Validates window creation, unique IDs, window lifecycle states, active focus,
//! Z-order stacking, window movement, resizing, minimum dimension limits,
//! title bar and control button hit-testing, occlusion, and screen boundary clipping.

use alloc::vec;
use bootloader_api::info::PixelFormat;

use super::manager::WindowManager;
use super::renderer::render_window;
use super::state::{WindowControl, WindowId, WindowState};
use super::window::Window;
use crate::graphics::color::Color;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;

/// Executes the complete Milestone 4C Window Manager self-test suite.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("        Running Micro-OS Phase 4C Window Manager Self-Tests     ");
    log::info!("================================================================");

    test_window_creation();
    test_unique_window_ids();
    test_window_closing();
    test_minimize_and_restore();
    test_maximize_and_restore();
    test_active_inactive_states();
    test_z_order_changes();
    test_window_movement();
    test_window_resizing();
    test_minimum_dimensions_enforcement();
    test_hit_testing();
    test_title_bar_boundaries();
    test_control_button_boundaries();
    test_overlapping_windows_z_order();
    test_screen_clipping();
    test_minimized_window_not_rendered();
    test_closed_window_not_rendered();
    test_window_manager_composite_integration();

    log::info!("================================================================");
    log::info!("   [OK] All Phase 4C Window Manager Self-Tests Passed!          ");
    log::info!("================================================================");
}

/// 1. Verifies basic window creation, title, bounds, and initial state.
fn test_window_creation() {
    let win = Window::new(WindowId::new(1), "Test Window", 50, 60, 220, 140);

    assert_eq!(win.id(), WindowId::new(1));
    assert_eq!(win.title(), "Test Window");
    assert_eq!(win.bounds(), Rect::new(50, 60, 220, 140));
    assert_eq!(win.state(), WindowState::Normal);
    assert!(!win.is_active());
    assert!(win.is_visible());
    assert_eq!(win.min_width(), 180);
    assert_eq!(win.min_height(), 100);

    log::info!("  [PASS] 1. Window creation, bounds, and default state test");
}

/// 2. Verifies sequential and unique window ID allocation.
fn test_unique_window_ids() {
    let mut wm = WindowManager::new(1024, 768);
    let id1 = wm.create_window("Window 1", 10, 10, 200, 120);
    let id2 = wm.create_window("Window 2", 20, 20, 200, 120);
    let id3 = wm.create_window("Window 3", 30, 30, 200, 120);

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
    assert_eq!(id1.as_u32(), 1);
    assert_eq!(id2.as_u32(), 2);
    assert_eq!(id3.as_u32(), 3);

    log::info!("  [PASS] 2. Unique window ID generation test");
}

/// 3. Verifies window closing, state transition to Closed, and removal.
fn test_window_closing() {
    let mut wm = WindowManager::new(1024, 768);
    let id1 = wm.create_window("Window 1", 10, 10, 200, 120);
    let id2 = wm.create_window("Window 2", 20, 20, 200, 120);

    assert!(wm.close_window(id2));
    let win2 = wm.get_window(id2).expect("Window 2 must still exist in memory before purge");
    assert_eq!(win2.state(), WindowState::Closed);
    assert!(!win2.is_visible());
    assert!(!win2.is_active());
    assert!(!wm.z_order().contains(&id2));

    wm.remove_closed();
    assert!(wm.get_window(id2).is_none());
    assert!(wm.get_window(id1).is_some());

    log::info!("  [PASS] 3. Window closing, state transition, and collection purge test");
}

/// 4. Verifies minimizing a window, coordinate preservation, and restoration.
fn test_minimize_and_restore() {
    let mut win = Window::new(WindowId::new(1), "Minimizable", 45, 55, 240, 150);

    win.minimize();
    assert_eq!(win.state(), WindowState::Minimized);
    assert!(!win.is_active());
    assert!(!win.hit_test(Point::new(50, 60)));

    win.restore();
    assert_eq!(win.state(), WindowState::Normal);
    assert_eq!(win.bounds(), Rect::new(45, 55, 240, 150));
    assert!(win.hit_test(Point::new(50, 60)));

    log::info!("  [PASS] 4. Window minimize and restore test");
}

/// 5. Verifies maximizing a window across the workspace and restoring exact original dimensions.
fn test_maximize_and_restore() {
    let mut win = Window::new(WindowId::new(1), "Maximizable", 40, 50, 260, 160);

    win.maximize(1280, 800, 34, 78);
    assert_eq!(win.state(), WindowState::Maximized);
    assert_eq!(win.bounds(), Rect::new(0, 34, 1280, 800 - 34 - 78));

    win.restore();
    assert_eq!(win.state(), WindowState::Normal);
    assert_eq!(win.bounds(), Rect::new(40, 50, 260, 160));

    log::info!("  [PASS] 5. Window maximize and restore dimensions test");
}

/// 6. Verifies active/inactive focus switching between windows.
fn test_active_inactive_states() {
    let mut wm = WindowManager::new(1024, 768);
    let id1 = wm.create_window("Window 1", 10, 10, 200, 120);
    let id2 = wm.create_window("Window 2", 30, 30, 200, 120);

    wm.set_active(id1);
    assert!(wm.get_window(id1).unwrap().is_active());
    assert!(!wm.get_window(id2).unwrap().is_active());
    assert_eq!(wm.active_window_id(), Some(id1));

    wm.set_active(id2);
    assert!(!wm.get_window(id1).unwrap().is_active());
    assert!(wm.get_window(id2).unwrap().is_active());
    assert_eq!(wm.active_window_id(), Some(id2));

    log::info!("  [PASS] 6. Active and inactive window focus states test");
}

/// 7. Verifies Z-order stack management and bring-to-front behavior.
fn test_z_order_changes() {
    let mut wm = WindowManager::new(1024, 768);
    let id1 = wm.create_window("Window 1", 10, 10, 200, 120);
    let id2 = wm.create_window("Window 2", 20, 20, 200, 120);
    let id3 = wm.create_window("Window 3", 30, 30, 200, 120);

    assert_eq!(wm.z_order(), &[id1, id2, id3]);

    wm.bring_to_front(id1);
    assert_eq!(wm.z_order(), &[id2, id3, id1]);
    assert_eq!(wm.active_window_id(), Some(id1));

    log::info!("  [PASS] 7. Z-order stack reordering and bring-to-front test");
}

/// 8. Verifies window coordinate updates during movement.
fn test_window_movement() {
    let mut win = Window::new(WindowId::new(1), "Moving Window", 15, 25, 200, 120);

    win.set_position(120, 180);
    assert_eq!(win.bounds(), Rect::new(120, 180, 200, 120));
    assert_eq!(win.title_bar_rect().x, 120);
    assert_eq!(win.title_bar_rect().y, 180);
    assert_eq!(win.content_rect().x, 121);
    assert_eq!(win.content_rect().y, 180 + 28);

    log::info!("  [PASS] 8. Window movement and geometry recalculation test");
}

/// 9. Verifies valid window resizing.
fn test_window_resizing() {
    let mut win = Window::new(WindowId::new(1), "Resizable Window", 50, 50, 200, 120);

    win.set_size(350, 220);
    assert_eq!(win.bounds().width, 350);
    assert_eq!(win.bounds().height, 220);

    log::info!("  [PASS] 9. Window resizing test");
}

/// 10. Verifies minimum dimension enforcement when attempting to shrink windows too far.
fn test_minimum_dimensions_enforcement() {
    let mut win = Window::new(WindowId::new(1), "Constrained", 20, 20, 200, 120)
        .with_min_size(190, 110);

    // Attempt to shrink below minimum width and height
    win.set_size(50, 40);
    assert_eq!(win.bounds().width, 190, "Width must be clamped to min_width");
    assert_eq!(win.bounds().height, 110, "Height must be clamped to min_height");

    log::info!("  [PASS] 10. Minimum window dimensions enforcement test");
}

/// 11. Verifies hit-testing on the topmost window.
fn test_hit_testing() {
    let win = Window::new(WindowId::new(1), "Hit Target", 100, 100, 200, 150);

    assert!(win.hit_test(Point::new(150, 150)));
    assert!(!win.hit_test(Point::new(50, 50)));
    assert!(!win.hit_test(Point::new(350, 200)));

    log::info!("  [PASS] 11. Window coordinate hit-testing test");
}

/// 12. Verifies title bar boundaries and exclusion of control buttons during title drag testing.
fn test_title_bar_boundaries() {
    let win = Window::new(WindowId::new(1), "Title Test", 100, 100, 220, 140);
    let title_rect = win.title_bar_rect();
    assert_eq!(title_rect, Rect::new(100, 100, 220, 28));

    // Point in title bar text area
    assert!(win.hit_test_title_bar(Point::new(120, 114)));

    // Point in content body below title bar
    assert!(!win.hit_test_title_bar(Point::new(120, 150)));

    // Point inside Close button: must return FALSE for title bar drag
    let close_rect = win.control_button_rect(WindowControl::Close);
    let close_center = Point::new(close_rect.x + 4, close_rect.y + 4);
    assert!(!win.hit_test_title_bar(close_center));

    log::info!("  [PASS] 12. Title bar boundaries and button exclusion test");
}

/// 13. Verifies control button hit-test precision (Minimize, Maximize, Close).
fn test_control_button_boundaries() {
    let win = Window::new(WindowId::new(1), "Buttons Test", 100, 100, 220, 140);

    let close_rect = win.control_button_rect(WindowControl::Close);
    let max_rect = win.control_button_rect(WindowControl::Maximize);
    let min_rect = win.control_button_rect(WindowControl::Minimize);

    assert_eq!(win.hit_test_controls(Point::new(close_rect.x + 3, close_rect.y + 3)), Some(WindowControl::Close));
    assert_eq!(win.hit_test_controls(Point::new(max_rect.x + 3, max_rect.y + 3)), Some(WindowControl::Maximize));
    assert_eq!(win.hit_test_controls(Point::new(min_rect.x + 3, min_rect.y + 3)), Some(WindowControl::Minimize));

    // Outside all buttons
    assert_eq!(win.hit_test_controls(Point::new(105, 110)), None);

    log::info!("  [PASS] 13. Control button hit-testing (Min, Max, Close) test");
}

/// 14. Verifies overlapping windows and proper Z-order hit testing priority.
fn test_overlapping_windows_z_order() {
    let mut wm = WindowManager::new(1024, 768);
    // Window 1: (50, 50, 180, 120)
    let id1 = wm.create_window("Window 1", 50, 50, 180, 120);
    // Window 2: (100, 80, 180, 120) - Overlaps Window 1 at (110, 90)
    let id2 = wm.create_window("Window 2", 100, 80, 180, 120);

    // Overlapping coordinate
    let overlap_pt = Point::new(120, 100);

    // Window 2 was created second, so it is top-most
    assert_eq!(wm.hit_test(overlap_pt), Some(id2));

    // Bring Window 1 to front
    wm.bring_to_front(id1);
    assert_eq!(wm.hit_test(overlap_pt), Some(id1));

    log::info!("  [PASS] 14. Overlapping window hit-test and Z-order priority test");
}

/// 15. Verifies safe screen clipping when windows are positioned partially off-screen.
fn test_screen_clipping() {
    let w = 120;
    let h = 120;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);
    surface.clear(Color::BLACK);

    // Window partially off-screen at top-left: (-60, -40, 180, 100)
    let win_tl = Window::new(WindowId::new(1), "Top-Left Offscreen", -60, -40, 180, 100);
    render_window(&win_tl, &mut surface);

    // Window partially off-screen at bottom-right: (80, 80, 180, 100)
    let win_br = Window::new(WindowId::new(2), "Bottom-Right Offscreen", 80, 80, 180, 100);
    render_window(&win_br, &mut surface);

    // Surface bounds check: corner pixel (0, 0) must have been rendered safely
    assert!(surface.get_pixel(0, 0).is_some());
    assert!(surface.get_pixel(w as i32 - 1, h as i32 - 1).is_some());

    log::info!("  [PASS] 15. Screen boundary clipping and buffer safety test");
}

/// 16. Verifies that minimized windows are omitted from the drawing pass.
fn test_minimized_window_not_rendered() {
    let w = 150;
    let h = 100;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);
    surface.clear(Color::BLACK);

    let mut win = Window::new(WindowId::new(1), "Minimized Window", 10, 10, 180, 100);
    win.minimize();

    render_window(&win, &mut surface);

    // Pixel at (20, 20) must remain untouched (BLACK)
    assert_eq!(surface.get_pixel(20, 20), Some(Color::BLACK));

    log::info!("  [PASS] 16. Minimized window omission from render pass test");
}

/// 17. Verifies that closed windows are omitted from the drawing pass.
fn test_closed_window_not_rendered() {
    let w = 150;
    let h = 100;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);
    surface.clear(Color::BLACK);

    let mut win = Window::new(WindowId::new(1), "Closed Window", 10, 10, 180, 100);
    win.close();

    render_window(&win, &mut surface);

    // Pixel at (20, 20) must remain untouched (BLACK)
    assert_eq!(surface.get_pixel(20, 20), Some(Color::BLACK));

    log::info!("  [PASS] 17. Closed window omission from render pass test");
}

/// 18. Verifies complete multi-window composite rendering via WindowManager.
fn test_window_manager_composite_integration() {
    let w = 260;
    let h = 180;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);
    surface.clear(Color::OBSIDIAN_NAVY);

    let mut wm = WindowManager::new(w as u32, h as u32);
    let id1 = wm.create_window("Back Window", 10, 10, 180, 110);
    let id2 = wm.create_window("Front Window", 50, 40, 180, 110);

    // id2 is front and active
    assert_eq!(wm.active_window_id(), Some(id2));
    wm.render(&mut surface);

    // Active window border check: (50, 40) + r corner arc or edge
    // Pixel inside front window title bar (60, 50) must be active title background
    let title_bg = wm.get_window(id2).unwrap().theme().title_active_bg;
    assert_eq!(surface.get_pixel(65, 50), Some(title_bg));

    // Switch focus to id1
    wm.set_active(id1);
    wm.render(&mut surface);
    let title_bg_1 = wm.get_window(id1).unwrap().theme().title_active_bg;
    assert_eq!(surface.get_pixel(25, 20), Some(title_bg_1));

    log::info!("  [PASS] 18. Multi-window composite rendering and focus integration test");
}
