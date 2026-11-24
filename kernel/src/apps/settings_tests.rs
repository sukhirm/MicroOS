//! In-Kernel Unit and Self-Tests for the Settings Application ("Settings").
//!
//! Verifies Settings initialization, section navigation, accent color switching,
//! preference toggles, display/input specification fidelity, mouse hit-testing,
//! keyboard navigation, window manager registration, and rendering clipping safety.

use super::settings::{
    open_or_focus_settings_window, render_settings_content, AccentColorChoice, SettingsApp,
    SettingsSection,
};
use bootloader_api::info::PixelFormat;
use crate::graphics::color::Color;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::window::manager::WindowManager;
use crate::window::state::WindowState;
use crate::window::window::Window;
use alloc::vec;

/// Runs all in-kernel Settings application self-tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("         Running Micro-OS Phase 4E Settings Self-Tests          ");
    log::info!("================================================================");

    test_settings_initial_state();
    test_settings_section_navigation();
    test_settings_accent_color_selection();
    test_settings_toggle_interactions();
    test_settings_display_information();
    test_settings_input_subsystem_info();
    test_settings_sidebar_mouse_click();
    test_settings_appearance_controls_click();
    test_settings_keyboard_navigation();
    test_settings_window_manager_integration();
    test_settings_rendering_clipping_safety();

    log::info!("================================================================");
    log::info!("         [OK] All Phase 4E Settings Self-Tests Passed!          ");
    log::info!("================================================================");
}

/// 1. Verifies default initialization values of SettingsApp.
fn test_settings_initial_state() {
    let app = SettingsApp::new();

    assert_eq!(app.current_section, SettingsSection::Appearance);
    assert_eq!(app.selected_accent, AccentColorChoice::Cyan);
    assert!(app.desktop_grid_enabled);
    assert!(app.transparency_enabled);
    assert!(app.animations_enabled);
    assert_eq!(app.selected_sidebar_index, 0);

    log::info!("  [PASS] 1. Settings initial state & default preferences test");
}

/// 2. Verifies section transitions and consistency across all 5 sections.
fn test_settings_section_navigation() {
    let mut app = SettingsApp::new();

    let sections = [
        (SettingsSection::Appearance, "Appearance", 0),
        (SettingsSection::Display, "Display", 1),
        (SettingsSection::SystemInfo, "System Info", 2),
        (SettingsSection::Input, "Input", 3),
        (SettingsSection::About, "About", 4),
    ];

    for (sec, title, idx) in sections {
        app.select_section(sec);
        assert_eq!(app.current_section, sec);
        assert_eq!(app.current_section.title(), title);
        assert_eq!(app.selected_sidebar_index, idx);
    }

    log::info!("  [PASS] 2. Settings section navigation & metadata consistency test");
}

/// 3. Verifies accent color choices, names, and color palettes.
fn test_settings_accent_color_selection() {
    let mut app = SettingsApp::new();

    let accents = [
        (AccentColorChoice::Cyan, "Cyan", Color::ACCENT_CYAN),
        (AccentColorChoice::Azure, "Azure", Color::ACCENT_AZURE),
        (AccentColorChoice::Emerald, "Emerald", Color::SUCCESS_GREEN),
        (AccentColorChoice::Amber, "Amber", Color::WARNING_AMBER),
    ];

    for (acc, name, expected_color) in accents {
        app.select_accent(acc);
        assert_eq!(app.selected_accent, acc);
        assert_eq!(app.selected_accent.name(), name);
        assert_eq!(app.selected_accent.color(), expected_color);
    }

    log::info!("  [PASS] 3. Settings accent colour selection & palette mapping test");
}

/// 4. Verifies toggle interaction and boolean state inversion.
fn test_settings_toggle_interactions() {
    let mut app = SettingsApp::new();

    // Initial states: all true
    assert!(app.desktop_grid_enabled);
    assert!(app.transparency_enabled);
    assert!(app.animations_enabled);

    // Toggle grid
    let grid_state = app.toggle_desktop_grid();
    assert!(!grid_state);
    assert!(!app.desktop_grid_enabled);
    app.toggle_desktop_grid();
    assert!(app.desktop_grid_enabled);

    // Toggle transparency
    let transp_state = app.toggle_transparency();
    assert!(!transp_state);
    assert!(!app.transparency_enabled);
    app.toggle_transparency();
    assert!(app.transparency_enabled);

    // Toggle animations
    let anim_state = app.toggle_animations();
    assert!(!anim_state);
    assert!(!app.animations_enabled);
    app.toggle_animations();
    assert!(app.animations_enabled);

    log::info!("  [PASS] 4. Settings toggle switch state inversion test");
}

/// 5. Verifies display specs accuracy (1280x800 / 16 MiB backbuffer).
fn test_settings_display_information() {
    let mut app = SettingsApp::new();
    app.select_section(SettingsSection::Display);
    assert_eq!(app.current_section, SettingsSection::Display);

    // Verify backbuffer size constant matches kernel architecture
    assert_eq!(
        crate::memory::BACKBUFFER_SIZE,
        16 * 1024 * 1024,
        "Double-buffer allocation must be exactly 16 MiB"
    );

    log::info!("  [PASS] 5. Settings display information & backbuffer capacity test");
}

/// 6. Verifies input subsystem driver information and shortcuts.
fn test_settings_input_subsystem_info() {
    let mut app = SettingsApp::new();
    app.select_section(SettingsSection::Input);
    assert_eq!(app.current_section, SettingsSection::Input);

    // Verify input queue capacity matches 128 entries
    assert_eq!(
        crate::input::queue::QUEUE_CAPACITY,
        128,
        "Input event queue capacity must be 128"
    );

    log::info!("  [PASS] 6. Settings input subsystem info & queue capacity test");
}

/// 7. Verifies mouse click hit-testing for sidebar navigation.
fn test_settings_sidebar_mouse_click() {
    let mut app = SettingsApp::new();
    let content_rect = Rect::new(100, 100, 540, 360);

    // Click item 1: Display (x: 120, y: 100 + 36 + 32 + 10 = 178)
    let clicked_display = app.handle_click(Point::new(120, 178), content_rect);
    assert!(clicked_display);
    assert_eq!(app.current_section, SettingsSection::Display);

    // Click item 3: Input (x: 120, y: 100 + 36 + (3 * 32) + 10 = 242)
    let clicked_input = app.handle_click(Point::new(120, 242), content_rect);
    assert!(clicked_input);
    assert_eq!(app.current_section, SettingsSection::Input);

    // Click item 0: Appearance (x: 120, y: 100 + 36 + 10 = 146)
    let clicked_appearance = app.handle_click(Point::new(120, 146), content_rect);
    assert!(clicked_appearance);
    assert_eq!(app.current_section, SettingsSection::Appearance);

    log::info!("  [PASS] 7. Settings sidebar mouse hit-testing test");
}

/// 8. Verifies mouse click hit-testing on Appearance accent swatches and toggles.
fn test_settings_appearance_controls_click() {
    let mut app = SettingsApp::new();
    app.select_section(SettingsSection::Appearance);
    let content_rect = Rect::new(100, 100, 540, 360);

    // Click on swatch 2 (Emerald)
    // swatch_w = 72, gap = 8; main_x = 100 + 130 + 14 = 244
    // swatch 2 x = 244 + 2 * 80 = 404; y = 100 + 70 + 10 = 180
    let clicked_emerald = app.handle_click(Point::new(410, 180), content_rect);
    assert!(clicked_emerald);
    assert_eq!(app.selected_accent, AccentColorChoice::Emerald);

    // Click on Toggle 0 (Desktop Grid)
    // toggle_start_y = 100 + 130 = 230
    assert!(app.desktop_grid_enabled);
    let clicked_grid = app.handle_click(Point::new(300, 240), content_rect);
    assert!(clicked_grid);
    assert!(!app.desktop_grid_enabled);

    // Click on Toggle 1 (Transparency)
    // toggle 1 y = 230 + 34 = 264
    assert!(app.transparency_enabled);
    let clicked_transp = app.handle_click(Point::new(300, 274), content_rect);
    assert!(clicked_transp);
    assert!(!app.transparency_enabled);

    log::info!("  [PASS] 8. Settings appearance controls mouse hit-testing test");
}

/// 9. Verifies keyboard navigation (Down/Up arrow cycling, Enter/Space toggling).
fn test_settings_keyboard_navigation() {
    let mut app = SettingsApp::new();
    assert_eq!(app.selected_sidebar_index, 0);
    assert_eq!(app.current_section, SettingsSection::Appearance);

    let down_key = KeyEvent {
        code: KeyCode::Down,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };
    let up_key = KeyEvent {
        code: KeyCode::Up,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };
    let right_key = KeyEvent {
        code: KeyCode::Right,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };

    // Down arrow -> Display
    assert!(app.handle_key(down_key));
    assert_eq!(app.current_section, SettingsSection::Display);
    assert_eq!(app.selected_sidebar_index, 1);

    // Up arrow -> Appearance
    assert!(app.handle_key(up_key));
    assert_eq!(app.current_section, SettingsSection::Appearance);
    assert_eq!(app.selected_sidebar_index, 0);

    // Right arrow on Appearance -> cycles accent from Cyan to Azure
    assert!(app.handle_key(right_key));
    assert_eq!(app.selected_accent, AccentColorChoice::Azure);

    log::info!("  [PASS] 9. Settings keyboard navigation (Arrows & cycling) test");
}

/// 10. Verifies WindowManager integration (open or bring to front, focus, single-instance).
fn test_settings_window_manager_integration() {
    let mut wm = WindowManager::new(1280, 800);

    // 1. Initial open spawns Settings window
    open_or_focus_settings_window(&mut wm);
    assert_eq!(wm.windows().len(), 1);

    let win = &wm.windows()[0];
    assert_eq!(win.title(), "Settings");
    assert_eq!(win.id().as_u32(), 3);
    assert_eq!(win.state(), WindowState::Normal);
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(3));

    // 2. Open again: must NOT spawn duplicate window, must focus existing
    open_or_focus_settings_window(&mut wm);
    assert_eq!(wm.windows().len(), 1, "Settings must remain single-instance");
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(3));

    // 3. Minimize and re-open: must restore to Normal and bring to front
    wm.minimize_window(crate::window::state::WindowId::new(3));
    assert_eq!(wm.get_window(crate::window::state::WindowId::new(3)).unwrap().state(), WindowState::Minimized);

    open_or_focus_settings_window(&mut wm);
    assert_eq!(wm.get_window(crate::window::state::WindowId::new(3)).unwrap().state(), WindowState::Normal);
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(3));

    log::info!("  [PASS] 10. Settings window manager registration & focus integration test");
}

/// 11. Verifies rendering with surface boundary clipping does not overflow or panic.
fn test_settings_rendering_clipping_safety() {
    let mut buffer = vec![0u8; 100 * 100 * 4];
    let mut surface = Surface::from_raw_parts(&mut buffer, 100, 100, 100, 4, PixelFormat::Bgr);

    let dummy_win = Window::new(
        crate::window::state::WindowId::new(3),
        "Settings",
        10,
        10,
        80,
        80,
    );

    // Clipped small rect
    let content_rect = Rect::new(10, 10, 80, 80);
    render_settings_content(&dummy_win, &mut surface, content_rect);

    // Degenerate zero-size rect
    let zero_rect = Rect::new(0, 0, 0, 0);
    render_settings_content(&dummy_win, &mut surface, zero_rect);

    log::info!("  [PASS] 11. Settings rendering clipping safety test");
}
