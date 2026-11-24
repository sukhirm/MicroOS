//! Milestone 4D Input Subsystem Self-Tests.
//!
//! Validates keyboard scancode decoding, modifier states, Shift/Caps combinations,
//! mouse 3-byte packet synchronization, signed movement, overflow handling,
//! button transitions, bounded event queue FIFO operations, overrun behavior,
//! window activation, title bar dragging, control buttons, and desktop shortcut dispatch.

use super::dispatcher::EventDispatcher;
use super::event::{
    ButtonAction, InputEvent, KeyAction, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseButtons,
    MouseEvent,
};
use super::keyboard::KeyboardDecoder;
use super::mouse::MouseDecoder;
use super::queue::EventQueue;
use crate::desktop::dock::hit_test_dock;
use crate::desktop::shell::DesktopShell;
use crate::graphics::geometry::Point;
use crate::window::state::{WindowControl, WindowState};
use crate::window::WindowManager;

/// Executes the complete Milestone 4D Input Subsystem self-test suite.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("        Running Micro-OS Phase 4D Input Subsystem Self-Tests    ");
    log::info!("================================================================");

    test_keyboard_scancode_translation();
    test_keyboard_press_and_release();
    test_keyboard_modifier_tracking();
    test_keyboard_shift_and_caps_lock();
    test_keyboard_extended_scancodes();

    test_mouse_packet_synchronization();
    test_mouse_movement_decoding();
    test_mouse_overflow_handling();
    test_mouse_button_transitions();
    test_mouse_cursor_bounds_clamping();

    test_event_queue_fifo();
    test_event_queue_overrun_behavior();
    test_event_queue_ordering();

    test_desktop_window_activation();
    test_desktop_window_dragging();
    test_desktop_window_controls();
    test_desktop_dock_hit_testing();
    test_desktop_keyboard_shortcuts();

    log::info!("================================================================");
    log::info!("   [OK] All Phase 4D Input Subsystem Self-Tests Passed!         ");
    log::info!("================================================================");
}

/// 1. Verifies keyboard scancode translation to characters (letters, digits, whitespace).
fn test_keyboard_scancode_translation() {
    let mut decoder = KeyboardDecoder::new();

    // 0x1E: Key 'A' (lowercase without shift)
    let event_a = decoder.process_byte(0x1E).expect("Key A make failed");
    assert_eq!(event_a.code, KeyCode::A);
    assert_eq!(event_a.action, KeyAction::Pressed);
    assert_eq!(event_a.character, Some('a'));

    // 0x02: Digit '1'
    let event_1 = decoder.process_byte(0x02).expect("Key 1 make failed");
    assert_eq!(event_1.code, KeyCode::Num1);
    assert_eq!(event_1.character, Some('1'));

    // 0x39: Space
    let event_sp = decoder.process_byte(0x39).expect("Space make failed");
    assert_eq!(event_sp.code, KeyCode::Space);
    assert_eq!(event_sp.character, Some(' '));

    // 0x1C: Enter
    let event_ent = decoder.process_byte(0x1C).expect("Enter make failed");
    assert_eq!(event_ent.code, KeyCode::Enter);
    assert_eq!(event_ent.character, Some('\n'));

    log::info!("  [PASS] 1. Keyboard scancode character translation test");
}

/// 2. Verifies key press and release tracking (Set 1 bit 7 convention).
fn test_keyboard_press_and_release() {
    let mut decoder = KeyboardDecoder::new();

    // Press 'W' (0x11)
    let press = decoder.process_byte(0x11).expect("Key W press failed");
    assert_eq!(press.code, KeyCode::W);
    assert_eq!(press.action, KeyAction::Pressed);
    assert_eq!(press.character, Some('w'));

    // Release 'W' (0x11 | 0x80 = 0x91)
    let release = decoder.process_byte(0x91).expect("Key W release failed");
    assert_eq!(release.code, KeyCode::W);
    assert_eq!(release.action, KeyAction::Released);
    assert_eq!(release.character, None, "Key release must not emit a character");

    log::info!("  [PASS] 2. Key press and release tracking test");
}

/// 3. Verifies Shift, Ctrl, and Alt modifier key tracking.
fn test_keyboard_modifier_tracking() {
    let mut decoder = KeyboardDecoder::new();

    // Press Left Shift (0x2A)
    let shift_press = decoder.process_byte(0x2A).expect("Shift press failed");
    assert_eq!(shift_press.code, KeyCode::LeftShift);
    assert!(decoder.modifiers().shift);

    // Press Left Ctrl (0x1D)
    decoder.process_byte(0x1D);
    assert!(decoder.modifiers().ctrl);

    // Press Left Alt (0x38)
    decoder.process_byte(0x38);
    assert!(decoder.modifiers().alt);

    // Release Left Shift (0xAA)
    decoder.process_byte(0xAA);
    assert!(!decoder.modifiers().shift);
    assert!(decoder.modifiers().ctrl);
    assert!(decoder.modifiers().alt);

    // Release Ctrl (0x9D) & Alt (0xB8)
    decoder.process_byte(0x9D);
    decoder.process_byte(0xB8);
    assert!(!decoder.modifiers().ctrl);
    assert!(!decoder.modifiers().alt);

    log::info!("  [PASS] 3. Keyboard modifier state tracking test");
}

/// 4. Verifies Shift and Caps Lock interaction logic.
fn test_keyboard_shift_and_caps_lock() {
    let mut decoder = KeyboardDecoder::new();

    // 1. Shift + 'A' -> 'A'
    decoder.process_byte(0x2A); // Shift press
    let ev = decoder.process_byte(0x1E).expect("Shift+A failed");
    assert_eq!(ev.character, Some('A'));
    decoder.process_byte(0xAA); // Shift release

    // 2. Caps Lock ON (0x3A)
    decoder.process_byte(0x3A); // Caps press (toggles on)
    assert!(decoder.modifiers().caps_lock);
    let ev_caps = decoder.process_byte(0x1E).expect("Caps+A failed");
    assert_eq!(ev_caps.character, Some('A'));

    // 3. Shift + Caps Lock -> lowercase 'a' (Shift inverts Caps Lock)
    decoder.process_byte(0x2A); // Shift press
    let ev_both = decoder.process_byte(0x1E).expect("Shift+Caps+A failed");
    assert_eq!(ev_both.character, Some('a'));
    decoder.process_byte(0xAA); // Shift release

    // 4. Toggle Caps Lock OFF (0x3A)
    decoder.process_byte(0x3A);
    assert!(!decoder.modifiers().caps_lock);

    log::info!("  [PASS] 4. Keyboard Shift and Caps Lock combination logic test");
}

/// 5. Verifies extended 0xE0 multi-byte scancode decoding (Arrow keys, Delete, Super).
fn test_keyboard_extended_scancodes() {
    let mut decoder = KeyboardDecoder::new();

    // Up Arrow: 0xE0, 0x48
    assert!(decoder.process_byte(0xE0).is_none());
    let up = decoder.process_byte(0x48).expect("Up arrow failed");
    assert_eq!(up.code, KeyCode::Up);
    assert_eq!(up.action, KeyAction::Pressed);

    // Down Arrow release: 0xE0, 0xD0 (0x50 | 0x80)
    assert!(decoder.process_byte(0xE0).is_none());
    let down_rel = decoder.process_byte(0xD0).expect("Down arrow release failed");
    assert_eq!(down_rel.code, KeyCode::Down);
    assert_eq!(down_rel.action, KeyAction::Released);

    // Left Arrow: 0xE0, 0x4B
    assert!(decoder.process_byte(0xE0).is_none());
    let left = decoder.process_byte(0x4B).expect("Left arrow failed");
    assert_eq!(left.code, KeyCode::Left);

    // Right Arrow: 0xE0, 0x4D
    assert!(decoder.process_byte(0xE0).is_none());
    let right = decoder.process_byte(0x4D).expect("Right arrow failed");
    assert_eq!(right.code, KeyCode::Right);

    log::info!("  [PASS] 5. Keyboard extended scancode decoding test");
}

/// 6. Verifies mouse packet synchronization (Header byte bit 3 check).
fn test_mouse_packet_synchronization() {
    let mut decoder = MouseDecoder::new(1280, 800);

    // Feed out-of-sync byte where bit 3 is 0 (must be rejected)
    assert!(decoder.process_byte(0x00).is_none());
    assert!(decoder.process_byte(0x07).is_none());

    // Feed valid packet: Byte 0 = 0x08 (bit 3 is 1), Byte 1 = 10, Byte 2 = 20
    assert!(decoder.process_byte(0x08).is_none());
    assert!(decoder.process_byte(10).is_none());
    let ev = decoder.process_byte(20).expect("Valid mouse packet failed");
    assert_eq!(ev.delta_x, 10);
    assert_eq!(ev.delta_y, -20); // Inverted Y

    log::info!("  [PASS] 6. Mouse packet boundary synchronization test");
}

/// 7. Verifies signed movement decoding (positive, negative, and Y-inversion).
fn test_mouse_movement_decoding() {
    let mut decoder = MouseDecoder::new(1280, 800);
    decoder.set_position(500, 400);

    // Move Right (+15) and Down (hardware delta_y = -10, screen_delta_y = +10)
    // Byte 0: 0x08 | y_sign (0x20) = 0x28
    // Byte 1: 15
    // Byte 2: 256 - 10 = 246 (0xF6)
    decoder.process_byte(0x28);
    decoder.process_byte(15);
    let ev = decoder.process_byte(246).expect("Movement packet failed");

    assert_eq!(ev.delta_x, 15);
    assert_eq!(ev.delta_y, 10, "Screen delta Y must be positive when moving down");
    assert_eq!(ev.x, 515);
    assert_eq!(ev.y, 410);

    log::info!("  [PASS] 7. Mouse signed movement decoding and coordinate inversion test");
}

/// 8. Verifies mouse overflow bit handling (resets delta to 0).
fn test_mouse_overflow_handling() {
    let mut decoder = MouseDecoder::new(1280, 800);
    decoder.set_position(100, 100);

    // Byte 0 with X overflow (0x40) and Y overflow (0x80): 0x08 | 0x40 | 0x80 = 0xC8
    decoder.process_byte(0xC8);
    decoder.process_byte(255);
    let ev = decoder.process_byte(255).expect("Overflow packet failed");

    assert_eq!(ev.delta_x, 0, "Overflow delta X must be suppressed to 0");
    assert_eq!(ev.delta_y, 0, "Overflow delta Y must be suppressed to 0");
    assert_eq!(ev.x, 100);
    assert_eq!(ev.y, 100);

    log::info!("  [PASS] 8. Mouse overflow bit suppression test");
}

/// 9. Verifies mouse button transitions (Left, Right, Middle pressed/released).
fn test_mouse_button_transitions() {
    let mut decoder = MouseDecoder::new(1280, 800);

    // 1. Left button press (b0 bit 0 = 1 -> 0x09)
    decoder.process_byte(0x09);
    decoder.process_byte(0);
    let ev_left = decoder.process_byte(0).expect("Left press failed");
    assert_eq!(
        ev_left.button_change,
        Some((MouseButton::Left, ButtonAction::Pressed))
    );
    assert!(ev_left.buttons.left);

    // 2. Right button press while left held (b0 = 0x08 | 0x01 | 0x02 = 0x0B)
    decoder.process_byte(0x0B);
    decoder.process_byte(0);
    let ev_right = decoder.process_byte(0).expect("Right press failed");
    assert_eq!(
        ev_right.button_change,
        Some((MouseButton::Right, ButtonAction::Pressed))
    );
    assert!(ev_right.buttons.left);
    assert!(ev_right.buttons.right);

    // 3. Left button release (b0 = 0x08 | 0x02 = 0x0A)
    decoder.process_byte(0x0A);
    decoder.process_byte(0);
    let ev_left_rel = decoder.process_byte(0).expect("Left release failed");
    assert_eq!(
        ev_left_rel.button_change,
        Some((MouseButton::Left, ButtonAction::Released))
    );
    assert!(!ev_left_rel.buttons.left);
    assert!(ev_left_rel.buttons.right);

    log::info!("  [PASS] 9. Mouse button state transition detection test");
}

/// 10. Verifies cursor boundary clamping (cannot move past screen edges).
fn test_mouse_cursor_bounds_clamping() {
    let mut decoder = MouseDecoder::new(800, 600);
    decoder.set_position(10, 10);

    // Try moving left (-50) and up (-50)
    // Negative X: bit 4 (0x10) | Negative Y: bit 5 (0x20) | 0x08 = 0x38
    // Delta X = 256 - 50 = 206
    // Delta Y = 50 (hardware positive up) -> screen delta Y = -50
    decoder.process_byte(0x08 | 0x10);
    decoder.process_byte(206);
    let ev = decoder.process_byte(50).expect("Clamping packet failed");

    assert_eq!(ev.x, 0, "Cursor X must clamp at left screen boundary (0)");
    assert_eq!(ev.y, 0, "Cursor Y must clamp at top screen boundary (0)");

    log::info!("  [PASS] 10. Mouse cursor screen boundary clamping test");
}

/// 11. Verifies event queue FIFO insertion and removal.
fn test_event_queue_fifo() {
    let mut q = EventQueue::new();
    assert!(q.is_empty());
    assert_eq!(q.len(), 0);

    let ev1 = InputEvent::Key(KeyEvent::new(
        KeyCode::A,
        KeyAction::Pressed,
        Some('a'),
        KeyModifiers::default(),
    ));
    let ev2 = InputEvent::Key(KeyEvent::new(
        KeyCode::B,
        KeyAction::Pressed,
        Some('b'),
        KeyModifiers::default(),
    ));

    q.push(ev1);
    q.push(ev2);
    assert_eq!(q.len(), 2);

    assert_eq!(q.pop(), Some(ev1));
    assert_eq!(q.pop(), Some(ev2));
    assert!(q.is_empty());

    log::info!("  [PASS] 11. Event queue FIFO insertion and retrieval test");
}

/// 12. Verifies event queue bounded overrun behavior (overwrites oldest without corruption).
fn test_event_queue_overrun_behavior() {
    let mut q = EventQueue::new();

    // Fill queue to capacity (128)
    for i in 0..128 {
        q.push(InputEvent::Key(KeyEvent::new(
            KeyCode::Num0,
            KeyAction::Pressed,
            Some((b'0' + (i % 10) as u8) as char),
            KeyModifiers::default(),
        )));
    }
    assert_eq!(q.len(), 128);
    assert_eq!(q.overruns(), 0);

    // Push 5 more events: must increment overrun count and overwrite oldest
    for _ in 0..5 {
        q.push(InputEvent::Key(KeyEvent::new(
            KeyCode::Z,
            KeyAction::Pressed,
            Some('z'),
            KeyModifiers::default(),
        )));
    }

    assert_eq!(q.len(), 128);
    assert_eq!(q.overruns(), 5);

    log::info!("  [PASS] 12. Event queue bounded capacity and overrun safety test");
}

/// 13. Verifies event queue ordering preservation.
fn test_event_queue_ordering() {
    let mut q = EventQueue::new();
    let keys = [KeyCode::H, KeyCode::E, KeyCode::L, KeyCode::L, KeyCode::O];

    for &k in &keys {
        q.push(InputEvent::Key(KeyEvent::new(
            k,
            KeyAction::Pressed,
            None,
            KeyModifiers::default(),
        )));
    }

    for &expected in &keys {
        if let Some(InputEvent::Key(ev)) = q.pop() {
            assert_eq!(ev.code, expected);
        } else {
            panic!("Expected key event");
        }
    }

    log::info!("  [PASS] 13. Event queue sequential ordering fidelity test");
}

/// 14. Verifies desktop window activation upon mouse click.
fn test_desktop_window_activation() {
    let mut wm = WindowManager::new(1280, 800);
    let id1 = wm.create_window("Window 1", 100, 100, 200, 150);
    let id2 = wm.create_window("Window 2", 150, 150, 200, 150);

    // Initial state: id2 is front-most and active
    assert_eq!(wm.active_window_id(), Some(id2));

    let mut shell = DesktopShell::new(1280, 800);
    let mut dispatcher = EventDispatcher::new(1280, 800);

    // Click inside Window 1 at (120, 120)
    let click_ev = InputEvent::Mouse(MouseEvent {
        x: 120,
        y: 120,
        delta_x: 0,
        delta_y: 0,
        buttons: MouseButtons { left: true, right: false, middle: false },
        button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
    });

    dispatcher.dispatch(click_ev, &mut wm, &mut shell);

    // Window 1 must now be active and front-most
    assert_eq!(wm.active_window_id(), Some(id1));

    log::info!("  [PASS] 14. Desktop window activation via mouse click test");
}

/// 15. Verifies window dragging via title bar.
fn test_desktop_window_dragging() {
    let mut wm = WindowManager::new(1280, 800);
    let id = wm.create_window("Draggable", 100, 100, 200, 150);

    let mut shell = DesktopShell::new(1280, 800);
    let mut dispatcher = EventDispatcher::new(1280, 800);

    // 1. Press left mouse in title bar at (120, 110)
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: 120,
            y: 110,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
        }),
        &mut wm,
        &mut shell,
    );

    assert_eq!(dispatcher.dragged_window(), Some(id));

    // 2. Drag mouse to (180, 160) (moved +60, +50)
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: 180,
            y: 160,
            delta_x: 60,
            delta_y: 50,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: None,
        }),
        &mut wm,
        &mut shell,
    );

    // Window must move from (100, 100) to (160, 150)
    assert_eq!(wm.get_window(id).unwrap().bounds().x, 160);
    assert_eq!(wm.get_window(id).unwrap().bounds().y, 150);

    // 3. Release left mouse button
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: 180,
            y: 160,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: false, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Released)),
        }),
        &mut wm,
        &mut shell,
    );

    assert_eq!(dispatcher.dragged_window(), None);

    log::info!("  [PASS] 15. Desktop window dragging via title bar test");
}

/// 16. Verifies window control buttons (Minimize, Maximize, Close) via mouse click.
fn test_desktop_window_controls() {
    let mut wm = WindowManager::new(1280, 800);
    let id = wm.create_window("Controls Window", 100, 100, 220, 140);
    let mut shell = DesktopShell::new(1280, 800);
    let mut dispatcher = EventDispatcher::new(1280, 800);

    // 1. Click Maximize button
    let max_rect = wm.get_window(id).unwrap().control_button_rect(WindowControl::Maximize);
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: max_rect.x + 4,
            y: max_rect.y + 4,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
        }),
        &mut wm,
        &mut shell,
    );
    assert_eq!(wm.get_window(id).unwrap().state(), WindowState::Maximized);

    // Click Maximize again: restores to Normal
    let max_rect_2 = wm.get_window(id).unwrap().control_button_rect(WindowControl::Maximize);
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: max_rect_2.x + 4,
            y: max_rect_2.y + 4,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
        }),
        &mut wm,
        &mut shell,
    );
    assert_eq!(wm.get_window(id).unwrap().state(), WindowState::Normal);

    // 2. Click Minimize button
    let min_rect = wm.get_window(id).unwrap().control_button_rect(WindowControl::Minimize);
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: min_rect.x + 4,
            y: min_rect.y + 4,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
        }),
        &mut wm,
        &mut shell,
    );
    assert_eq!(wm.get_window(id).unwrap().state(), WindowState::Minimized);

    // 3. Restore and click Close button
    wm.restore_window(id);
    let close_rect = wm.get_window(id).unwrap().control_button_rect(WindowControl::Close);
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: close_rect.x + 4,
            y: close_rect.y + 4,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
        }),
        &mut wm,
        &mut shell,
    );
    assert_eq!(wm.get_window(id).unwrap().state(), WindowState::Closed);

    log::info!("  [PASS] 16. Window control buttons (Min, Max, Close) click dispatch test");
}

/// 17. Verifies dock hit-testing and item identification.
fn test_desktop_dock_hit_testing() {
    let screen_w = 1280;
    let screen_h = 800;

    // Test clicking the 1st dock entry (Files)
    // Dock is centered: total width = 7 * 64 + 32 = 480
    // start x = (1280 - 480) / 2 = 400
    // 1st slot center = 400 + 16 + 32 = 448, Y = 800 - 62 - 14 + 30 = 754
    let hit_0 = hit_test_dock(screen_w, screen_h, Point::new(448, 754));
    assert_eq!(hit_0, Some(0), "Must hit 1st dock item (Files)");

    // Test clicking the 7th dock entry (Deadlock)
    let hit_6 = hit_test_dock(screen_w, screen_h, Point::new(832, 754));
    assert_eq!(hit_6, Some(6), "Must hit 7th dock item (Deadlock)");

    // Test clicking outside dock
    let hit_outside = hit_test_dock(screen_w, screen_h, Point::new(100, 100));
    assert_eq!(hit_outside, None);

    log::info!("  [PASS] 17. Desktop application dock hit-testing test");
}

/// 18. Verifies keyboard shortcuts (Alt+Tab window cycling, Escape drag cancel).
fn test_desktop_keyboard_shortcuts() {
    let mut wm = WindowManager::new(1280, 800);
    let id1 = wm.create_window("Window 1", 100, 100, 200, 150);
    let id2 = wm.create_window("Window 2", 150, 150, 200, 150);

    let mut shell = DesktopShell::new(1280, 800);
    let mut dispatcher = EventDispatcher::new(1280, 800);

    // Initial active is id2
    assert_eq!(wm.active_window_id(), Some(id2));

    // Send Alt+Tab: must cycle active window to id1
    dispatcher.dispatch(
        InputEvent::Key(KeyEvent::new(
            KeyCode::Tab,
            KeyAction::Pressed,
            None,
            KeyModifiers {
                alt: true,
                shift: false,
                caps_lock: false,
                ctrl: false,
                super_key: false,
            },
        )),
        &mut wm,
        &mut shell,
    );

    assert_eq!(wm.active_window_id(), Some(id1));

    // Start a drag, then press Escape to cancel
    dispatcher.dispatch(
        InputEvent::Mouse(MouseEvent {
            x: 120,
            y: 110,
            delta_x: 0,
            delta_y: 0,
            buttons: MouseButtons { left: true, right: false, middle: false },
            button_change: Some((MouseButton::Left, ButtonAction::Pressed)),
        }),
        &mut wm,
        &mut shell,
    );
    assert_eq!(dispatcher.dragged_window(), Some(id1));

    dispatcher.dispatch(
        InputEvent::Key(KeyEvent::new(
            KeyCode::Escape,
            KeyAction::Pressed,
            None,
            KeyModifiers::default(),
        )),
        &mut wm,
        &mut shell,
    );
    assert_eq!(dispatcher.dragged_window(), None, "Escape must cancel drag");

    log::info!("  [PASS] 18. Desktop keyboard shortcut dispatch (Alt+Tab, Escape) test");
}
