//! In-Kernel Unit and Self-Tests for the File Manager Application ("Files").
//!
//! Verifies initial directory listing, folder navigation, history tracking (Back/Forward/Up),
//! sidebar navigation shortcuts, keyboard navigation, click hit-testing, and empty directory presentation.

use super::files::{FileManagerApp, SIDEBAR_SHORTCUTS};
use crate::fs::vfs::{FsNodeKind, VFS};
use crate::graphics::color::Color;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::window::manager::WindowManager;
use crate::window::state::WindowState;
use crate::window::window::Window;
use alloc::vec;

/// Runs all in-kernel File Manager self-tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("       Running Micro-OS Phase 4E File Manager Self-Tests        ");
    log::info!("================================================================");

    test_files_initial_state();
    test_files_folder_navigation();
    test_files_history_back_forward();
    test_files_sidebar_navigation();
    test_files_mouse_click_hit_testing();
    test_files_keyboard_navigation();
    test_files_empty_directory_handling();
    test_files_window_integration();

    log::info!("================================================================");
    log::info!("       [OK] All Phase 4E File Manager Self-Tests Passed!        ");
    log::info!("================================================================");
}

/// 1. Verifies initial path and directory listing of the File Manager.
fn test_files_initial_state() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();

    assert_eq!(app.current_path, "/home/user");
    assert!(!app.can_go_back());
    assert!(!app.can_go_forward());

    let entries = app.current_entries();
    assert!(entries.len() >= 5, "Initial /home/user must contain sample folders");
    assert!(entries.iter().any(|e| e.name == "Documents" && e.kind == FsNodeKind::Directory));
    assert!(entries.iter().any(|e| e.name == "Downloads" && e.kind == FsNodeKind::Directory));

    log::info!("  [PASS] 1. File Manager initial state & directory listing test");
}

/// 2. Verifies navigating into subfolders.
fn test_files_folder_navigation() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();

    // Navigate to Documents
    let changed = app.navigate_to("/home/user/Documents");
    assert!(changed);
    assert_eq!(app.current_path, "/home/user/Documents");
    assert!(app.can_go_back());
    assert!(!app.can_go_forward());

    let doc_entries = app.current_entries();
    assert!(doc_entries.iter().any(|e| e.name == "notes.txt" && e.kind == FsNodeKind::File));
    assert!(doc_entries.iter().any(|e| e.name == "roadmap.md" && e.kind == FsNodeKind::File));

    // Navigate to invalid path should fail safely without changing path
    let invalid = app.navigate_to("/home/user/NonExistent");
    assert!(!invalid);
    assert_eq!(app.current_path, "/home/user/Documents");

    log::info!("  [PASS] 2. File Manager folder navigation & validation test");
}

/// 3. Verifies Back, Forward, and Up navigation history.
fn test_files_history_back_forward() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();

    // 1. /home/user -> /home/user/Documents
    app.navigate_to("/home/user/Documents");
    // 2. /home/user/Documents -> /home/user/Downloads
    app.navigate_to("/home/user/Downloads");
    assert_eq!(app.current_path, "/home/user/Downloads");

    // 3. Go Back -> /home/user/Documents
    assert!(app.go_back());
    assert_eq!(app.current_path, "/home/user/Documents");
    assert!(app.can_go_forward());

    // 4. Go Back -> /home/user
    assert!(app.go_back());
    assert_eq!(app.current_path, "/home/user");
    assert!(!app.can_go_back());

    // 5. Go Forward -> /home/user/Documents
    assert!(app.go_forward());
    assert_eq!(app.current_path, "/home/user/Documents");

    // 6. Go Up -> /home/user
    assert!(app.go_up());
    assert_eq!(app.current_path, "/home/user");

    // 7. Go Up -> /home
    assert!(app.go_up());
    assert_eq!(app.current_path, "/home");

    // 8. Go Up -> /
    assert!(app.go_up());
    assert_eq!(app.current_path, "/");

    // 9. Up from root stays at root
    assert!(!app.go_up());
    assert_eq!(app.current_path, "/");

    log::info!("  [PASS] 3. File Manager Back, Forward, and Up navigation history test");
}

/// 4. Verifies sidebar shortcut navigation.
fn test_files_sidebar_navigation() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();

    for shortcut in SIDEBAR_SHORTCUTS.iter() {
        let changed = app.navigate_to(shortcut.path);
        assert!(changed || app.current_path == shortcut.path);
        assert_eq!(app.current_path, shortcut.path);
        assert!(VFS.lock().is_dir(shortcut.path));
    }

    log::info!("  [PASS] 4. File Manager sidebar shortcut navigation test");
}

/// 5. Verifies mouse click hit-testing inside window client area.
fn test_files_mouse_click_hit_testing() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();
    let content_rect = Rect::new(100, 100, 560, 350);

    // Click on Up button: content_rect.x + 68 + 4 = 172, content_rect.y + 6 + 4 = 110
    let up_click = app.handle_click(Point::new(content_rect.x + 72, content_rect.y + 10), content_rect);
    assert!(up_click, "Up button click must navigate to /home");
    assert_eq!(app.current_path, "/home");

    // Click on Back button: content_rect.x + 8 + 4 = 112, content_rect.y + 6 + 4 = 110
    let back_click = app.handle_click(Point::new(content_rect.x + 12, content_rect.y + 10), content_rect);
    assert!(back_click, "Back button click must navigate back to /home/user");
    assert_eq!(app.current_path, "/home/user");

    // Click on Sidebar Documents shortcut (2nd item, index 1)
    // start_y = content_rect.y + 36 + 22 = 158. Item 1 y = 158 + 26 = 184
    let sidebar_click = app.handle_click(Point::new(content_rect.x + 40, content_rect.y + 36 + 22 + 30), content_rect);
    assert!(sidebar_click, "Sidebar click must navigate to Documents");
    assert_eq!(app.current_path, "/home/user/Documents");

    log::info!("  [PASS] 5. File Manager mouse click hit-testing test");
}

/// 6. Verifies keyboard navigation: Arrow selection, Enter to open, Backspace to go back.
fn test_files_keyboard_navigation() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();
    assert_eq!(app.current_path, "/home/user");

    // Press Down to select 1st entry
    let down1 = app.handle_key(KeyEvent {
        code: KeyCode::Down,
        character: None,
        action: KeyAction::Pressed,
        modifiers: KeyModifiers::default(),
    });
    assert!(down1);
    assert_eq!(app.selected_index, Some(0));

    // Press Enter to open the 1st entry (which is Documents directory)
    let enter = app.handle_key(KeyEvent {
        code: KeyCode::Enter,
        character: None,
        action: KeyAction::Pressed,
        modifiers: KeyModifiers::default(),
    });
    assert!(enter);
    assert_eq!(app.current_path, "/home/user/Documents");

    // Press Backspace to navigate back
    let backspace = app.handle_key(KeyEvent {
        code: KeyCode::Backspace,
        character: None,
        action: KeyAction::Pressed,
        modifiers: KeyModifiers::default(),
    });
    assert!(backspace);
    assert_eq!(app.current_path, "/home/user");

    log::info!("  [PASS] 6. File Manager keyboard navigation (Arrows, Enter, Backspace) test");
}

/// 7. Verifies handling empty directory presentation.
fn test_files_empty_directory_handling() {
    let mut app = FileManagerApp::new();
    app.ensure_initialized();

    // Navigate to /home/user/Videos which is intentionally empty
    app.navigate_to("/home/user/Videos");
    assert_eq!(app.current_path, "/home/user/Videos");

    let entries = app.current_entries();
    assert_eq!(entries.len(), 0, "Videos directory must be empty");

    // Verify rendering empty directory does not panic
    let mut buf = vec![0u8; 280 * 180 * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, 280, 180, 280, 4, bootloader_api::info::PixelFormat::Bgr);
    surface.clear(Color::BLACK);

    let dummy_win = Window::new(crate::window::state::WindowId::new(99), "Files", 0, 0, 280, 180);
    super::files::render_files_content(&dummy_win, &mut surface, Rect::new(0, 0, 280, 180));

    log::info!("  [PASS] 7. File Manager empty directory handling & presentation test");
}

/// 8. Verifies File Manager window integration with WindowManager.
fn test_files_window_integration() {
    let mut wm = WindowManager::new(1280, 800);

    // Open Files window via helper
    super::files::open_or_focus_files_window(&mut wm);

    let files_win = wm.windows().iter().find(|w| w.title() == "Files");
    assert!(files_win.is_some(), "Files window must be registered in WindowManager");
    let win = files_win.unwrap();
    assert_eq!(win.state(), WindowState::Normal);
    assert!(win.is_active(), "Newly opened Files window must be active");

    // Calling open_or_focus again must bring existing window to front without duplicating
    super::files::open_or_focus_files_window(&mut wm);
    let count = wm.windows().iter().filter(|w| w.title() == "Files").count();
    assert_eq!(count, 1, "There must be exactly 1 Files window instance");

    log::info!("  [PASS] 8. File Manager window manager registration & focus integration test");
}
