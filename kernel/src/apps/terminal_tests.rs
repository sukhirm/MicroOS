//! In-Kernel Unit and Self-Tests for the Terminal Application ("Terminal").
//!
//! Verifies terminal initialization, command line parsing, empty input handling,
//! unknown command reporting, filesystem commands (`pwd`, `ls`, `cd`, `mkdir`, `touch`, `cat`, `rm`),
//! command history navigation (Up/Down arrows), system telemetry commands (`uname`, `uptime`, `meminfo`, `tasks`, `whoami`, `echo`),
//! hardware clock and reboot honesty warnings (`date`, `reboot`), WindowManager registration,
//! and rendering clipping safety.

use super::terminal::{
    open_or_focus_terminal_window, render_terminal_content, TerminalApp,
};
use bootloader_api::info::PixelFormat;
use crate::fs::vfs::VFS;
use crate::graphics::geometry::Rect;
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::window::manager::WindowManager;
use crate::window::state::WindowState;
use crate::window::window::Window;
use alloc::vec;

/// Runs all in-kernel Terminal application self-tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("       Running Micro-OS Phase 4E Terminal Self-Tests            ");
    log::info!("================================================================");

    test_terminal_initial_state();
    test_terminal_command_parsing();
    test_terminal_empty_input();
    test_terminal_unknown_command();
    test_terminal_pwd_and_ls();
    test_terminal_cd_navigation();
    test_terminal_mkdir_and_touch();
    test_terminal_cat();
    test_terminal_rm();
    test_terminal_command_history();
    test_terminal_system_commands();
    test_terminal_date_and_reboot();
    test_terminal_window_integration();
    test_terminal_rendering_clipping_safety();

    log::info!("================================================================");
    log::info!("       [OK] All Phase 4E Terminal Self-Tests Passed!            ");
    log::info!("================================================================");
}

/// Helper to simulate typing text into the terminal app.
fn type_string(app: &mut TerminalApp, text: &str) {
    for c in text.chars() {
        let key = KeyEvent {
            code: match c {
                '\n' => KeyCode::Enter,
                _ => KeyCode::Unknown(0),
            },
            action: KeyAction::Pressed,
            character: if c == '\n' { None } else { Some(c) },
            modifiers: KeyModifiers::default(),
        };
        app.handle_key(key);
    }
}

/// Helper to simulate pressing Enter.
fn press_enter(app: &mut TerminalApp) {
    let key = KeyEvent {
        code: KeyCode::Enter,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };
    app.handle_key(key);
}

/// 1. Verifies default initial state and shell prompt format.
fn test_terminal_initial_state() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    assert_eq!(app.cwd, "/home/user");
    assert_eq!(app.format_prompt(), "user@micro-os:~$ ");
    assert!(app.input_buffer.is_empty());
    assert_eq!(app.cursor_pos, 0);
    assert!(app.history.is_empty());
    assert!(!app.output_lines.is_empty(), "Welcome banner lines should be present");

    log::info!("  [PASS] 1. Terminal initial state & prompt format test");
}

/// 2. Verifies command parsing and argument tokenization.
fn test_terminal_command_parsing() {
    // 2a. Regression test for physical bare-metal boot panic:
    // Uninitialized TerminalApp instance must safely resolve paths relative to /home/user rather than /
    let uninit_app = TerminalApp::new();
    assert_eq!(uninit_app.cwd, "");
    assert_eq!(uninit_app.resolve_path("Documents"), "/home/user/Documents");
    assert_eq!(uninit_app.resolve_path(".."), "/home");
    assert_eq!(uninit_app.resolve_path("/sys/version.txt"), "/sys/version.txt");
    assert_eq!(uninit_app.resolve_path("~"), "/home/user");
    assert_eq!(uninit_app.resolve_path("~/Downloads"), "/home/user/Downloads");
    assert_eq!(uninit_app.format_prompt(), "user@micro-os:~$ ");

    // 2b. Explicitly initialized TerminalApp instance
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    assert_eq!(app.cwd, "/home/user");
    assert_eq!(app.resolve_path("Documents"), "/home/user/Documents");
    assert_eq!(app.resolve_path(".."), "/home");
    assert_eq!(app.resolve_path("/sys/version.txt"), "/sys/version.txt");
    assert_eq!(app.resolve_path("~"), "/home/user");
    assert_eq!(app.resolve_path("~/Downloads"), "/home/user/Downloads");
    assert_eq!(app.format_prompt(), "user@micro-os:~$ ");

    log::info!("  [PASS] 2. Terminal path resolution & token parsing test");
}

/// 3. Verifies entering empty lines or spaces does not error and does not pollute history.
fn test_terminal_empty_input() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    let initial_hist_len = app.history.len();
    let initial_output_len = app.output_lines.len();

    // Type spaces and Enter
    type_string(&mut app, "   ");
    press_enter(&mut app);

    assert_eq!(app.history.len(), initial_hist_len, "Empty input must not be added to history");
    assert_eq!(app.output_lines.len(), initial_output_len + 1, "Must echo blank prompt line");
    assert!(app.input_buffer.is_empty());

    log::info!("  [PASS] 3. Terminal empty input handling test");
}

/// 4. Verifies unknown command execution produces descriptive warning.
fn test_terminal_unknown_command() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    type_string(&mut app, "xyz_unknown_cmd");
    press_enter(&mut app);

    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0], "xyz_unknown_cmd");

    let last_line = app.output_lines.last().expect("Should have output line");
    assert!(last_line.text.contains("command not found: xyz_unknown_cmd"));

    log::info!("  [PASS] 4. Terminal unknown command rejection test");
}

/// 5. Verifies `pwd` and `ls` commands.
fn test_terminal_pwd_and_ls() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // 1. Test pwd
    type_string(&mut app, "pwd");
    press_enter(&mut app);
    let pwd_output = app.output_lines.last().expect("Must have pwd output");
    assert_eq!(pwd_output.text, "/home/user");

    // 2. Test ls
    type_string(&mut app, "ls");
    press_enter(&mut app);

    // Verify output lines contain sample directories like Documents, Downloads
    let has_docs = app.output_lines.iter().any(|l| l.text.contains("Documents/"));
    let has_downloads = app.output_lines.iter().any(|l| l.text.contains("Downloads/"));
    assert!(has_docs, "ls output must list Documents/");
    assert!(has_downloads, "ls output must list Downloads/");

    log::info!("  [PASS] 5. Terminal pwd and ls command execution test");
}

/// 6. Verifies `cd` directory navigation.
fn test_terminal_cd_navigation() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // 1. cd Documents
    type_string(&mut app, "cd Documents");
    press_enter(&mut app);
    assert_eq!(app.cwd, "/home/user/Documents");
    assert_eq!(app.format_prompt(), "user@micro-os:~/Documents$ ");

    // 2. cd ..
    type_string(&mut app, "cd ..");
    press_enter(&mut app);
    assert_eq!(app.cwd, "/home/user");
    assert_eq!(app.format_prompt(), "user@micro-os:~$ ");

    // 3. cd /sys
    type_string(&mut app, "cd /sys");
    press_enter(&mut app);
    assert_eq!(app.cwd, "/sys");
    assert_eq!(app.format_prompt(), "user@micro-os:/sys$ ");

    // 4. cd to non-existent directory
    type_string(&mut app, "cd /nonexistent_directory");
    press_enter(&mut app);
    assert_eq!(app.cwd, "/sys", "Failed cd must not alter working directory");
    let err_line = app.output_lines.last().expect("Must produce error line");
    assert!(err_line.text.contains("No such file or directory"));

    // 5. cd with no args returns to /home/user
    type_string(&mut app, "cd");
    press_enter(&mut app);
    assert_eq!(app.cwd, "/home/user");

    log::info!("  [PASS] 6. Terminal cd directory navigation test");
}

/// 7. Verifies `mkdir` and `touch` in VFS.
fn test_terminal_mkdir_and_touch() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // Create a unique test directory
    type_string(&mut app, "mkdir terminal_test_dir");
    press_enter(&mut run_with_app(&mut app));

    assert!(VFS.lock().exists("/home/user/terminal_test_dir"));
    assert!(VFS.lock().is_dir("/home/user/terminal_test_dir"));

    // Touch a file inside it
    type_string(&mut app, "touch terminal_test_dir/test_file.txt");
    press_enter(&mut app);

    assert!(VFS.lock().exists("/home/user/terminal_test_dir/test_file.txt"));

    // Cleanup: Remove test file and directory to ensure idempotent filesystem state
    let mut vfs = VFS.lock();
    let _ = vfs.remove("/home/user/terminal_test_dir/test_file.txt");
    let _ = vfs.remove("/home/user/terminal_test_dir");

    log::info!("  [PASS] 7. Terminal mkdir and touch VFS integration test");
}

fn run_with_app(app: &mut TerminalApp) -> &mut TerminalApp {
    app
}

/// 8. Verifies `cat` file viewing.
fn test_terminal_cat() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // Cat notes.txt in Documents
    type_string(&mut app, "cat Documents/notes.txt");
    press_enter(&mut app);

    let found_welcome = app.output_lines.iter().any(|l| l.text.contains("Welcome to Micro-OS!"));
    assert!(found_welcome, "cat Documents/notes.txt must display file content");

    // Cat non-existent file
    type_string(&mut app, "cat missing.txt");
    press_enter(&mut app);
    let err_line = app.output_lines.last().expect("Must have error line");
    assert!(err_line.text.contains("No such file or directory"));

    log::info!("  [PASS] 8. Terminal cat file reading test");
}

/// 9. Verifies `rm` file and directory deletion.
fn test_terminal_rm() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // 1. Create temporary file and delete it
    VFS.lock().create_file("/home/user/temp_delete.txt", b"delete me").unwrap();
    assert!(VFS.lock().exists("/home/user/temp_delete.txt"));

    type_string(&mut app, "rm temp_delete.txt");
    press_enter(&mut app);
    assert!(!VFS.lock().exists("/home/user/temp_delete.txt"));

    // 2. Reject root deletion
    type_string(&mut app, "rm /");
    press_enter(&mut app);
    let err_line = app.output_lines.last().expect("Must error on root deletion");
    assert!(err_line.text.contains("cannot remove root directory"));

    log::info!("  [PASS] 9. Terminal rm file and directory deletion test");
}

/// 10. Verifies command history navigation with Up and Down arrow keys.
fn test_terminal_command_history() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // Execute 3 distinct commands
    type_string(&mut app, "echo first");
    press_enter(&mut app);
    type_string(&mut app, "echo second");
    press_enter(&mut app);
    type_string(&mut app, "echo third");
    press_enter(&mut app);

    assert_eq!(app.history.len(), 3);
    assert_eq!(app.history[0], "echo first");
    assert_eq!(app.history[1], "echo second");
    assert_eq!(app.history[2], "echo third");

    // Type draft text: "my draft"
    type_string(&mut app, "my draft");
    assert_eq!(app.input_buffer, "my draft");

    // Press Up: should recall "echo third" and save draft
    let up_key = KeyEvent {
        code: KeyCode::Up,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };
    app.handle_key(up_key);
    assert_eq!(app.input_buffer, "echo third");
    assert_eq!(app.history_index, Some(2));
    assert_eq!(app.draft_input, "my draft");

    // Press Up again: should recall "echo second"
    app.handle_key(up_key);
    assert_eq!(app.input_buffer, "echo second");
    assert_eq!(app.history_index, Some(1));

    // Press Down: should recall "echo third"
    let down_key = KeyEvent {
        code: KeyCode::Down,
        action: KeyAction::Pressed,
        character: None,
        modifiers: KeyModifiers::default(),
    };
    app.handle_key(down_key);
    assert_eq!(app.input_buffer, "echo third");
    assert_eq!(app.history_index, Some(2));

    // Press Down again: should restore draft "my draft"
    app.handle_key(down_key);
    assert_eq!(app.input_buffer, "my draft");
    assert_eq!(app.history_index, None);

    log::info!("  [PASS] 10. Terminal command history navigation test");
}

/// 11. Verifies system information and telemetry commands (`uname`, `uptime`, `meminfo`, `tasks`, `whoami`, `echo`).
fn test_terminal_system_commands() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // 1. uname -a
    type_string(&mut app, "uname -a");
    press_enter(&mut app);
    assert!(app.output_lines.iter().any(|l| l.text.contains("x86_64") && l.text.contains("Micro-OS")));

    // 2. uptime
    type_string(&mut app, "uptime");
    press_enter(&mut app);
    assert!(app.output_lines.iter().any(|l| l.text.contains("timer ticks") && l.text.contains("PIT")));

    // 3. meminfo
    type_string(&mut app, "meminfo");
    press_enter(&mut app);
    assert!(app.output_lines.iter().any(|l| l.text.contains("MemTotal:")));
    assert!(app.output_lines.iter().any(|l| l.text.contains("HeapAllocated:")));
    assert!(app.output_lines.iter().any(|l| l.text.contains("BackbufferSize:")));

    // 4. tasks
    type_string(&mut app, "tasks");
    press_enter(&mut app);
    assert!(app.output_lines.iter().any(|l| l.text.contains("PID") && l.text.contains("STATE")));

    // 5. whoami
    type_string(&mut app, "whoami");
    press_enter(&mut app);
    assert!(app.output_lines.iter().any(|l| l.text == "user"));

    // 6. echo
    type_string(&mut app, "echo Hello MicroOS Obsidian Azure!");
    press_enter(&mut app);
    assert!(app.output_lines.iter().any(|l| l.text == "Hello MicroOS Obsidian Azure!"));

    log::info!("  [PASS] 11. Terminal system telemetry & utility commands test");
}

/// 12. Verifies `date` and `reboot` honesty warnings.
fn test_terminal_date_and_reboot() {
    let mut app = TerminalApp::new();
    app.ensure_initialized();

    // 1. date
    type_string(&mut app, "date");
    press_enter(&mut app);
    let date_line = app.output_lines.last().expect("Must have date line");
    assert!(date_line.text.contains("Hardware CMOS RTC") && date_line.text.contains("unavailable"));

    // 2. reboot
    type_string(&mut app, "reboot");
    press_enter(&mut app);
    let reboot_line = app.output_lines.last().expect("Must have reboot line");
    assert!(reboot_line.text.contains("ACPI / 8042 reset mechanism is not implemented"));

    log::info!("  [PASS] 12. Terminal date & reboot hardware honesty warnings test");
}

/// 13. Verifies WindowManager integration (open or bring to front, focus, single-instance).
fn test_terminal_window_integration() {
    let mut wm = WindowManager::new(1280, 800);

    // 1. Initial open spawns Terminal window
    open_or_focus_terminal_window(&mut wm);
    assert_eq!(wm.windows().len(), 1);

    let win = &wm.windows()[0];
    assert_eq!(win.title(), "Terminal");
    assert_eq!(win.id().as_u32(), 6);
    assert_eq!(win.state(), WindowState::Normal);
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(6));

    // 2. Open again: must NOT spawn duplicate window, must focus existing
    open_or_focus_terminal_window(&mut wm);
    assert_eq!(wm.windows().len(), 1, "Terminal must remain single-instance");
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(6));

    // 3. Minimize and re-open: must restore to Normal and bring to front
    wm.minimize_window(crate::window::state::WindowId::new(6));
    assert_eq!(
        wm.get_window(crate::window::state::WindowId::new(6)).unwrap().state(),
        WindowState::Minimized
    );

    open_or_focus_terminal_window(&mut wm);
    assert_eq!(
        wm.get_window(crate::window::state::WindowId::new(6)).unwrap().state(),
        WindowState::Normal
    );
    assert_eq!(wm.active_window_id().map(|id| id.as_u32()), Some(6));

    log::info!("  [PASS] 13. Terminal window manager registration & focus integration test");
}

/// 14. Verifies rendering with surface boundary clipping does not overflow or panic.
fn test_terminal_rendering_clipping_safety() {
    let mut buffer = vec![0u8; 100 * 100 * 4];
    let mut surface = Surface::from_raw_parts(&mut buffer, 100, 100, 100, 4, PixelFormat::Bgr);

    let dummy_win = Window::new(
        crate::window::state::WindowId::new(6),
        "Terminal",
        10,
        10,
        80,
        80,
    );

    // Clipped small rect
    let content_rect = Rect::new(10, 10, 80, 80);
    render_terminal_content(&dummy_win, &mut surface, content_rect);

    // Degenerate zero-size rect
    let zero_rect = Rect::new(0, 0, 0, 0);
    render_terminal_content(&dummy_win, &mut surface, zero_rect);

    log::info!("  [PASS] 14. Terminal rendering clipping safety test");
}
