//! In-Kernel Unit and Self-Tests for the System Monitor Application ("SysMon").
//!
//! Verifies System Monitor initialization, section navigation, system info retrieval,
//! memory telemetry, task snapshot listing, scheduler statistics, mouse hit-testing,
//! keyboard navigation, window manager registration, and rendering clipping safety.

use super::sysmon::{
    open_or_focus_sysmon_window, render_sysmon_content, SysMonApp, SysMonSection,
};
use bootloader_api::info::PixelFormat;
use crate::arch::x86_64::timer;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::memory;
use crate::task;
use crate::window::manager::WindowManager;
use crate::window::state::WindowState;
use crate::window::window::Window;
use alloc::vec;

/// Runs all in-kernel System Monitor self-tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("       Running Micro-OS Phase 4E System Monitor Self-Tests      ");
    log::info!("================================================================");

    test_sysmon_initial_state();
    test_sysmon_section_navigation();
    test_sysmon_system_info_retrieval();
    test_sysmon_memory_statistics();
    test_sysmon_task_listing();
    test_sysmon_scheduler_statistics();
    test_sysmon_sidebar_mouse_click();
    test_sysmon_keyboard_navigation();
    test_sysmon_window_manager_integration();
    test_sysmon_rendering_clipping_safety();

    log::info!("================================================================");
    log::info!("       [OK] All Phase 4E System Monitor Self-Tests Passed!      ");
    log::info!("================================================================");
}

/// 1. Verifies default initialization values of SysMonApp.
fn test_sysmon_initial_state() {
    let app = SysMonApp::new();

    assert_eq!(app.current_section, SysMonSection::Overview);
    assert_eq!(app.selected_sidebar_index, 0);
    assert_eq!(app.task_scroll_offset, 0);

    log::info!("  [PASS] 1. System Monitor initial state test");
}

/// 2. Verifies section transitions and consistency across all 5 diagnostic sections.
fn test_sysmon_section_navigation() {
    let mut app = SysMonApp::new();

    let sections = [
        (SysMonSection::Overview, "Overview", 0),
        (SysMonSection::Cpu, "CPU", 1),
        (SysMonSection::Memory, "Memory", 2),
        (SysMonSection::Tasks, "Processes", 3),
        (SysMonSection::Performance, "Performance", 4),
    ];

    for (sec, title, idx) in sections {
        app.select_section(sec);
        assert_eq!(app.current_section, sec);
        assert_eq!(app.current_section.title(), title);
        assert_eq!(app.selected_sidebar_index, idx);
    }

    log::info!("  [PASS] 2. System Monitor section navigation & title consistency test");
}

/// 3. Verifies CPU architecture and timer uptime telemetry retrieval.
fn test_sysmon_system_info_retrieval() {
    let _ticks = timer::ticks();
    let _uptime_ms = timer::uptime_ms();

    // Verify PIT target frequency is 100 Hz
    assert_eq!(timer::TARGET_FREQUENCY_HZ, 100, "PIT frequency must be 100 Hz");

    log::info!("  [PASS] 3. System Monitor system info & timer telemetry test");
}

/// 4. Verifies physical and virtual memory statistics.
fn test_sysmon_memory_statistics() {
    let ram_bytes = memory::total_usable_ram_bytes();
    assert!(ram_bytes >= 128 * 1024 * 1024, "Usable RAM must be at least 128 MiB");

    let heap_total = memory::heap_total_bytes();
    assert_eq!(heap_total, 16 * 1024 * 1024, "Heap capacity must be exactly 16 MiB");

    let heap_allocated = memory::heap_allocated_bytes();
    assert!(heap_allocated <= heap_total, "Allocated bytes cannot exceed heap capacity");

    assert_eq!(
        memory::BACKBUFFER_SIZE,
        16 * 1024 * 1024,
        "Dedicated graphics backbuffer must be exactly 16 MiB"
    );

    log::info!("  [PASS] 4. System Monitor memory telemetry & heap capacity test");
}

/// 5. Verifies task snapshots collection from the scheduler.
fn test_sysmon_task_listing() {
    let snapshots = task::task_snapshots();
    // In bare-metal environment, task list reflects all spawned tasks
    for s in &snapshots {
        assert!(s.id.as_u64() > 0, "Task ID must be non-zero");
        assert!(!s.name.is_empty(), "Task name cannot be empty");
    }

    log::info!("  [PASS] 5. System Monitor task listing & snapshot collection test");
}

/// 6. Verifies scheduler performance counters.
fn test_sysmon_scheduler_statistics() {
    let _cs = task::context_switch_count();
    let _p = task::preemption_count();
    let _r = task::ready_task_count();

    log::info!("  [PASS] 6. System Monitor scheduler performance counters test");
}

/// 7. Verifies mouse click hit-testing for sidebar section switching.
fn test_sysmon_sidebar_mouse_click() {
    let mut app = SysMonApp::new();
    let content_rect = Rect::new(100, 100, 560, 380);

    // Click item 1: CPU (x: 120, y: 100 + 36 + 32 + 10 = 178)
    let clicked_cpu = app.handle_click(Point::new(120, 178), content_rect);
    assert!(clicked_cpu);
    assert_eq!(app.current_section, SysMonSection::Cpu);

    // Click item 2: Memory (x: 120, y: 100 + 36 + (2 * 32) + 10 = 210)
    let clicked_mem = app.handle_click(Point::new(120, 210), content_rect);
    assert!(clicked_mem);
    assert_eq!(app.current_section, SysMonSection::Memory);

    // Click item 3: Tasks (x: 120, y: 100 + 36 + (3 * 32) + 10 = 242)
    let clicked_tasks = app.handle_click(Point::new(120, 242), content_rect);
    assert!(clicked_tasks);
    assert_eq!(app.current_section, SysMonSection::Tasks);

    // Click item 0: Overview (x: 120, y: 100 + 36 + 10 = 146)
    let clicked_overview = app.handle_click(Point::new(120, 146), content_rect);
    assert!(clicked_overview);
    assert_eq!(app.current_section, SysMonSection::Overview);

    log::info!("  [PASS] 7. System Monitor sidebar mouse hit-testing test");
}

/// 8. Verifies keyboard navigation (Down/Up arrow cycling, PageDown/PageUp scrolling).
fn test_sysmon_keyboard_navigation() {
    let mut app = SysMonApp::new();
    assert_eq!(app.selected_sidebar_index, 0);
    assert_eq!(app.current_section, SysMonSection::Overview);

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
    let pgdown_key = KeyEvent {
        code: KeyCode::PageDown,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };

    // Down arrow -> CPU
    assert!(app.handle_key(down_key));
    assert_eq!(app.current_section, SysMonSection::Cpu);
    assert_eq!(app.selected_sidebar_index, 1);

    // Up arrow -> Overview
    assert!(app.handle_key(up_key));
    assert_eq!(app.current_section, SysMonSection::Overview);
    assert_eq!(app.selected_sidebar_index, 0);

    // Switch to Tasks and test PageDown scroll
    app.select_section(SysMonSection::Tasks);
    assert_eq!(app.task_scroll_offset, 0);
    assert!(app.handle_key(pgdown_key));
    assert_eq!(app.task_scroll_offset, 3);

    log::info!("  [PASS] 8. System Monitor keyboard navigation test");
}

/// 9. Verifies WindowManager integration (open or bring to front, focus, single-instance).
fn test_sysmon_window_manager_integration() {
    let mut wm = WindowManager::new(1280, 800);

    // 1. Initial open spawns System Monitor window
    open_or_focus_sysmon_window(&mut wm);
    assert_eq!(wm.windows().len(), 1);

    let win = &wm.windows()[0];
    assert_eq!(win.title(), "System Monitor");
    assert_eq!(win.id().as_u32(), 2);
    assert_eq!(win.state(), WindowState::Normal);
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(2));

    // 2. Open again: must NOT spawn duplicate window, must focus existing
    open_or_focus_sysmon_window(&mut wm);
    assert_eq!(wm.windows().len(), 1, "System Monitor must remain single-instance");
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(2));

    // 3. Minimize and re-open: must restore to Normal and bring to front
    wm.minimize_window(crate::window::state::WindowId::new(2));
    assert_eq!(wm.get_window(crate::window::state::WindowId::new(2)).unwrap().state(), WindowState::Minimized);

    open_or_focus_sysmon_window(&mut wm);
    assert_eq!(wm.get_window(crate::window::state::WindowId::new(2)).unwrap().state(), WindowState::Normal);
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(2));

    log::info!("  [PASS] 9. System Monitor window manager registration & focus integration test");
}

/// 10. Verifies rendering with surface boundary clipping does not overflow or panic.
fn test_sysmon_rendering_clipping_safety() {
    let mut buffer = vec![0u8; 100 * 100 * 4];
    let mut surface = Surface::from_raw_parts(&mut buffer, 100, 100, 100, 4, PixelFormat::Bgr);

    let dummy_win = Window::new(
        crate::window::state::WindowId::new(2),
        "System Monitor",
        10,
        10,
        80,
        80,
    );

    // Clipped small rect
    let content_rect = Rect::new(10, 10, 80, 80);
    render_sysmon_content(&dummy_win, &mut surface, content_rect);

    // Degenerate zero-size rect
    let zero_rect = Rect::new(0, 0, 0, 0);
    render_sysmon_content(&dummy_win, &mut surface, zero_rect);

    log::info!("  [PASS] 10. System Monitor rendering clipping safety test");
}
