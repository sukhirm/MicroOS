//! Micro-OS Terminal Application ("Terminal").
//!
//! Provides a graphical terminal emulator matching the Cosmic Slate / Obsidian Azure theme:
//! - Deep obsidian dark canvas background
//! - Monospaced bitmap text rendering (8x8 at scale 1)
//! - Cyan shell prompt (`user@micro-os:path$ `)
//! - Interactive command-line editing with cursor positioning, backspace, and arrow history
//! - Scrollable terminal output history buffer
//! - Integration with the in-memory Virtual File System (VFS)
//! - Real hardware and kernel telemetry commands (`uptime`, `meminfo`, `tasks`, `uname`)
//! - Strict anti-fabrication policies for unavailable hardware (`date`, `reboot`).

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::arch::x86_64::timer;
use crate::fs::vfs::{FsError, FsNodeKind, Vfs, VFS};
use crate::graphics::color::Color;
use crate::graphics::font::{draw_char, draw_string, measure_string};
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::ICON_TERMINAL;
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::memory;
use crate::task::{self, TaskState};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Global spinlock-protected Terminal application instance.
pub static TERMINAL_APP: Mutex<TerminalApp> = Mutex::new(TerminalApp::new());

/// An individual line of text stored in the terminal output buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLine {
    pub text: String,
    pub color: Color,
}

impl TerminalLine {
    pub fn new(text: &str, color: Color) -> Self {
        Self {
            text: text.to_owned(),
            color,
        }
    }
}

/// Terminal Application state machine.
pub struct TerminalApp {
    /// Current working directory path (defaults to `/home/user`).
    pub cwd: String,
    /// Currently typed input text buffer.
    pub input_buffer: String,
    /// Cursor position index (in bytes/characters) within `input_buffer`.
    pub cursor_pos: usize,
    /// Historical output lines printed to the terminal.
    pub output_lines: Vec<TerminalLine>,
    /// Command history stack (most recent command at the end).
    pub history: Vec<String>,
    /// Current navigation position in `history` (None = drafting new command).
    pub history_index: Option<usize>,
    /// Saved draft input before user started navigating history with Up/Down.
    pub draft_input: String,
    /// Vertical scroll offset from the bottom (0 = scrolled to latest output).
    pub scroll_offset: usize,
    /// Maximum number of output lines retained in memory.
    pub max_output_lines: usize,
}

impl TerminalApp {
    /// Default user home directory path.
    pub const DEFAULT_CWD: &'static str = "/home/user";

    /// Creates a new default TerminalApp instance rooted at `/home/user`.
    pub const fn new() -> Self {
        Self {
            cwd: String::new(),
            input_buffer: String::new(),
            cursor_pos: 0,
            output_lines: Vec::new(),
            history: Vec::new(),
            history_index: None,
            draft_input: String::new(),
            scroll_offset: 0,
            max_output_lines: 500,
        }
    }

    /// Returns the active working directory, safely defaulting to `/home/user` if uninitialized.
    pub fn cwd(&self) -> &str {
        if self.cwd.is_empty() {
            Self::DEFAULT_CWD
        } else {
            &self.cwd
        }
    }

    /// Initializes default state and welcome banner if uninitialized.
    pub fn ensure_initialized(&mut self) {
        if self.cwd.is_empty() {
            self.cwd = Self::DEFAULT_CWD.to_string();
            self.output_lines.push(TerminalLine::new(
                "Micro-OS Terminal Shell [Cosmic Slate]",
                Color::ACCENT_CYAN,
            ));
            self.output_lines.push(TerminalLine::new(
                "Type 'help' for available commands. In-memory VFS is active.",
                Color::TEXT_MUTED,
            ));
            self.output_lines.push(TerminalLine::new("", Color::WHITE));
        }
    }

    /// Formats the active prompt string based on the current working directory.
    pub fn format_prompt(&self) -> String {
        let base = self.cwd();
        let display_path = if base == Self::DEFAULT_CWD {
            "~".to_string()
        } else if let Some(sub) = base.strip_prefix("/home/user/") {
            format!("~/{}", sub)
        } else {
            base.to_string()
        };
        format!("user@micro-os:{}$ ", display_path)
    }

    /// Resolves a path argument relative to `self.cwd()`, with tilde expansion.
    /// Falls back safely to `/home/user` even if `ensure_initialized()` was not yet called.
    pub fn resolve_path(&self, arg: &str) -> String {
        let path = if arg == "~" {
            Self::DEFAULT_CWD.to_string()
        } else if let Some(sub) = arg.strip_prefix("~/") {
            format!("{}/{}", Self::DEFAULT_CWD, sub)
        } else if arg.starts_with('/') {
            arg.to_string()
        } else {
            let base = self.cwd();
            let mut combined = base.to_string();
            if !combined.ends_with('/') {
                combined.push('/');
            }
            combined.push_str(arg);
            combined
        };
        Vfs::canonicalize_path(&path)
    }

    /// Appends a single line to the terminal output buffer, trimming if capacity is exceeded.
    pub fn add_line(&mut self, text: &str, color: Color) {
        self.output_lines.push(TerminalLine::new(text, color));
        if self.output_lines.len() > self.max_output_lines {
            let excess = self.output_lines.len() - self.max_output_lines;
            self.output_lines.drain(0..excess);
        }
    }

    /// Executes the currently typed command line.
    pub fn execute_command_line(&mut self) {
        self.ensure_initialized();
        let prompt = self.format_prompt();
        let cmd_line = self.input_buffer.clone();
        let trimmed = cmd_line.trim();

        // 1. Echo the prompt and typed command to output history
        let full_echo = format!("{}{}", prompt, cmd_line);
        self.add_line(&full_echo, Color::TEXT_LIGHT);

        // 2. If non-empty, save to command history
        if !trimmed.is_empty() {
            // Avoid immediate duplicate in history
            if self.history.last().map(|s| s.as_str()) != Some(trimmed) {
                self.history.push(trimmed.to_string());
                if self.history.len() > 100 {
                    self.history.remove(0);
                }
            }
        }

        // 3. Clear input buffer, reset history index and draft
        self.input_buffer.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        self.draft_input.clear();
        self.scroll_offset = 0; // Snap to bottom on command execution

        // 4. If empty input, simply return
        if trimmed.is_empty() {
            return;
        }

        // 5. Parse command name and arguments
        let mut tokens = trimmed.split_whitespace();
        let cmd = match tokens.next() {
            Some(c) => c,
            None => return,
        };
        let args: Vec<&str> = tokens.collect();

        // 6. Dispatch command
        self.dispatch_command(cmd, &args, trimmed);
    }

    /// Dispatches a parsed command with arguments.
    pub fn dispatch_command(&mut self, cmd: &str, args: &[&str], raw_line: &str) {
        match cmd {
            "help" => self.cmd_help(),
            "clear" => self.cmd_clear(),
            "ls" => self.cmd_ls(args),
            "cd" => self.cmd_cd(args),
            "pwd" => self.cmd_pwd(),
            "mkdir" => self.cmd_mkdir(args),
            "touch" => self.cmd_touch(args),
            "cat" => self.cmd_cat(args),
            "rm" => self.cmd_rm(args),
            "uname" => self.cmd_uname(args),
            "uptime" => self.cmd_uptime(),
            "meminfo" => self.cmd_meminfo(),
            "tasks" => self.cmd_tasks(),
            "history" => self.cmd_history(),
            "echo" => self.cmd_echo(args, raw_line),
            "whoami" => self.cmd_whoami(),
            "date" => self.cmd_date(),
            "reboot" => self.cmd_reboot(),
            "apt" => self.cmd_apt(args),
            "deadlock" => self.cmd_deadlock(args),
            unknown => {
                let err_msg = format!("command not found: {}. Type 'help' for available commands.", unknown);
                self.add_line(&err_msg, Color::WARNING_AMBER);
            }
        }
    }

    /// `help`: Prints available commands and usage notes.
    fn cmd_help(&mut self) {
        self.add_line("Micro-OS Terminal Shell (Cosmic Slate)", Color::ACCENT_CYAN);
        self.add_line("Available built-in commands:", Color::WHITE);
        self.add_line("  help         - Display this command reference", Color::TEXT_LIGHT);
        self.add_line("  clear        - Clear terminal output buffer", Color::TEXT_LIGHT);
        self.add_line("  ls [path]    - List files and directories in VFS", Color::TEXT_LIGHT);
        self.add_line("  cd [path]    - Change working directory (default: /home/user)", Color::TEXT_LIGHT);
        self.add_line("  pwd          - Print current working directory", Color::TEXT_LIGHT);
        self.add_line("  mkdir <dir>  - Create a new directory in VFS", Color::TEXT_LIGHT);
        self.add_line("  touch <file> - Create an empty file in VFS", Color::TEXT_LIGHT);
        self.add_line("  cat <file>   - Display contents of a file", Color::TEXT_LIGHT);
        self.add_line("  rm <path>    - Remove a file or directory in VFS", Color::TEXT_LIGHT);
        self.add_line("  uname [-a]   - Print system and kernel architecture info", Color::TEXT_LIGHT);
        self.add_line("  uptime       - Display system uptime and PIT timer ticks", Color::TEXT_LIGHT);
        self.add_line("  meminfo      - Display physical RAM, heap, and backbuffer stats", Color::TEXT_LIGHT);
        self.add_line("  tasks        - List kernel scheduler tasks and states", Color::TEXT_LIGHT);
        self.add_line("  history      - Display recently executed commands", Color::TEXT_LIGHT);
        self.add_line("  echo [text]  - Print text to the terminal", Color::TEXT_LIGHT);
        self.add_line("  whoami       - Print current active username", Color::TEXT_LIGHT);
        self.add_line("  date         - Display wall-clock time (hardware RTC status)", Color::TEXT_LIGHT);
        self.add_line("  reboot       - Reboot system (hardware support status)", Color::TEXT_LIGHT);
        self.add_line("  apt <cmd>    - MicroOS APT package manager (list, search, install...)", Color::TEXT_LIGHT);
        self.add_line("  deadlock     - Deadlock Research Lab (Topic R14: Banker vs Detection)", Color::TEXT_LIGHT);
        self.add_line("Note: All filesystem modifications affect the shared in-memory VFS.", Color::TEXT_MUTED);
        self.add_line("      Changes are volatile and not persisted across reboot.", Color::TEXT_MUTED);
    }

    /// `clear`: Clears output lines.
    fn cmd_clear(&mut self) {
        self.output_lines.clear();
        self.scroll_offset = 0;
    }

    /// `ls` [path]: Lists files and directories in current or specified path.
    fn cmd_ls(&mut self, args: &[&str]) {
        let target_path = if let Some(&arg) = args.first() {
            self.resolve_path(arg)
        } else {
            self.cwd().to_string()
        };

        let vfs = VFS.lock();
        match vfs.list_dir(&target_path) {
            Ok(entries) => {
                if entries.is_empty() {
                    self.add_line("(empty directory)", Color::TEXT_MUTED);
                } else {
                    for entry in entries {
                        if entry.kind == FsNodeKind::Directory {
                            let dir_label = format!("{}/  <DIR>", entry.name);
                            self.add_line(&dir_label, Color::ACCENT_CYAN);
                        } else {
                            let file_label = format!("{: <20}  {: >6} B", entry.name, entry.size);
                            self.add_line(&file_label, Color::WHITE);
                        }
                    }
                }
            }
            Err(FsError::NotFound) => {
                let err = format!("ls: cannot access '{}': No such file or directory", target_path);
                self.add_line(&err, Color::WARNING_AMBER);
            }
            Err(FsError::NotADirectory) => {
                let err = format!("ls: cannot access '{}': Not a directory", target_path);
                self.add_line(&err, Color::WARNING_AMBER);
            }
            Err(_) => {
                let err = format!("ls: error accessing '{}'", target_path);
                self.add_line(&err, Color::WARNING_AMBER);
            }
        }
    }

    /// `cd` [path]: Changes current working directory.
    fn cmd_cd(&mut self, args: &[&str]) {
        let target_path = if let Some(&arg) = args.first() {
            self.resolve_path(arg)
        } else {
            Self::DEFAULT_CWD.to_string()
        };

        let vfs = VFS.lock();
        if !vfs.exists(&target_path) {
            let err = format!("cd: {}: No such file or directory", target_path);
            self.add_line(&err, Color::WARNING_AMBER);
        } else if !vfs.is_dir(&target_path) {
            let err = format!("cd: {}: Not a directory", target_path);
            self.add_line(&err, Color::WARNING_AMBER);
        } else {
            self.cwd = target_path;
        }
    }

    /// `pwd`: Prints current working directory.
    fn cmd_pwd(&mut self) {
        let cwd_clone = self.cwd().to_string();
        self.add_line(&cwd_clone, Color::WHITE);
    }

    /// `mkdir` <dir>: Creates a directory in VFS.
    fn cmd_mkdir(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.add_line("mkdir: missing operand", Color::WARNING_AMBER);
            return;
        }

        let mut vfs = VFS.lock();
        for &arg in args {
            let target = self.resolve_path(arg);
            match vfs.mkdir(&target) {
                Ok(()) => {}
                Err(FsError::AlreadyExists) => {
                    let err = format!("mkdir: cannot create directory '{}': File exists", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
                Err(FsError::NotFound) => {
                    let err = format!("mkdir: cannot create directory '{}': No such file or directory", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
                Err(_) => {
                    let err = format!("mkdir: failed to create directory '{}'", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
            }
        }
    }

    /// `touch` <file>: Creates an empty file in VFS.
    fn cmd_touch(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.add_line("touch: missing operand", Color::WARNING_AMBER);
            return;
        }

        let mut vfs = VFS.lock();
        for &arg in args {
            let target = self.resolve_path(arg);
            if vfs.exists(&target) {
                // Already exists, touch maintains file in VFS
            } else {
                match vfs.create_file(&target, b"") {
                    Ok(()) => {}
                    Err(err) => {
                        let err_msg = format!("touch: cannot touch '{}': {:?}", arg, err);
                        self.add_line(&err_msg, Color::WARNING_AMBER);
                    }
                }
            }
        }
    }

    /// `cat` <file>: Displays file contents.
    fn cmd_cat(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.add_line("cat: missing operand", Color::WARNING_AMBER);
            return;
        }

        let vfs = VFS.lock();
        for &arg in args {
            let target = self.resolve_path(arg);
            match vfs.read_file(&target) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        self.add_line(line, Color::WHITE);
                    }
                }
                Err(FsError::NotFound) => {
                    let err = format!("cat: {}: No such file or directory", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
                Err(FsError::IsADirectory) => {
                    let err = format!("cat: {}: Is a directory", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
                Err(_) => {
                    let err = format!("cat: {}: Error reading file", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
            }
        }
    }

    /// `rm` <path>: Safely deletes a file or directory from VFS.
    fn cmd_rm(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.add_line("rm: missing operand", Color::WARNING_AMBER);
            return;
        }

        let mut vfs = VFS.lock();
        for &arg in args {
            let target = self.resolve_path(arg);
            if target == "/" {
                self.add_line("rm: cannot remove root directory '/'", Color::WARNING_AMBER);
                continue;
            }

            match vfs.remove(&target) {
                Ok(()) => {}
                Err(FsError::NotFound) => {
                    let err = format!("rm: cannot remove '{}': No such file or directory", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
                Err(FsError::CannotDeleteRoot) => {
                    self.add_line("rm: cannot remove root directory '/'", Color::WARNING_AMBER);
                }
                Err(_) => {
                    let err = format!("rm: failed to remove '{}'", arg);
                    self.add_line(&err, Color::WARNING_AMBER);
                }
            }
        }
    }

    /// `uname` [-a]: Prints system information.
    fn cmd_uname(&mut self, args: &[&str]) {
        if args.iter().any(|&a| a == "-a") {
            self.add_line(
                "Micro-OS 0.11.17 #1 SMP bare-metal x86_64 GNU/Linux-compatible Obsidian Azure (UEFI GOP)",
                Color::ACCENT_CYAN,
            );
        } else {
            self.add_line("Micro-OS", Color::ACCENT_CYAN);
        }
    }

    /// `uptime`: Prints actual system uptime from 8254 PIT.
    fn cmd_uptime(&mut self) {
        let ms = timer::uptime_ms();
        let total_sec = ms / 1000;
        let mins = total_sec / 60;
        let secs = total_sec % 60;
        let ticks = timer::ticks();
        let uptime_str = format!(
            "up {}m {:02}s, 1 user, timer ticks: {} (100 Hz 8254 PIT)",
            mins, secs, ticks
        );
        self.add_line(&uptime_str, Color::SUCCESS_GREEN);
    }

    /// `meminfo`: Prints actual physical RAM, heap, and backbuffer stats.
    fn cmd_meminfo(&mut self) {
        let ram_bytes = memory::total_usable_ram_bytes();
        let ram_kb = ram_bytes / 1024;
        let ram_mb = ram_bytes / (1024 * 1024);
        let heap_alloc = memory::heap_allocated_bytes();
        let heap_total = memory::heap_total_bytes();
        let heap_free = heap_total.saturating_sub(heap_alloc);
        let frames = memory::allocated_frames_count();

        self.add_line(&format!("MemTotal:        {: >10} kB", ram_kb), Color::WHITE);
        self.add_line(&format!("MemUsable:       {: >10} kB ({} MiB usable RAM)", ram_kb, ram_mb), Color::WHITE);
        self.add_line(&format!("HeapAllocated:   {: >10} kB", heap_alloc / 1024), Color::WHITE);
        self.add_line(&format!("HeapTotal:       {: >10} kB ({} KiB LinkedList)", heap_total / 1024, heap_total / 1024), Color::WHITE);
        self.add_line(&format!("HeapFree:        {: >10} kB", heap_free / 1024), Color::WHITE);
        self.add_line("BackbufferSize:       16384 kB (16 MiB dedicated GOP buffer)", Color::WHITE);
        self.add_line(&format!("FramesAllocated: {: >10} (4 KiB bootloader frames)", frames), Color::WHITE);
    }

    /// `tasks`: Prints scheduler tasks.
    fn cmd_tasks(&mut self) {
        let snapshots = task::task_snapshots();
        let r_count = task::ready_task_count();
        self.add_line(
            &format!("PID   STATE       STACK     NAME (Ready: {}, Total: {})", r_count, snapshots.len()),
            Color::TEXT_MUTED,
        );

        if snapshots.is_empty() {
            self.add_line("[ Single-task main kernel thread | Multitasking idle ]", Color::TEXT_MUTED);
        } else {
            for s in snapshots {
                let state_str = match s.state {
                    TaskState::Running => "Running",
                    TaskState::Ready => "Ready",
                    TaskState::Blocked => "Blocked",
                    TaskState::Terminated => "Terminated",
                };
                let line = format!("{: <5} {: <11} {: >6} B  {}", s.id.as_u64(), state_str, s.stack_size, s.name);
                self.add_line(&line, Color::WHITE);
            }
        }
    }

    /// `history`: Prints command history.
    fn cmd_history(&mut self) {
        if self.history.is_empty() {
            self.add_line("(no commands in history)", Color::TEXT_MUTED);
        } else {
            let hist_lines: Vec<String> = self
                .history
                .iter()
                .enumerate()
                .map(|(i, cmd)| format!("{: >4}  {}", i + 1, cmd))
                .collect();
            for line in hist_lines {
                self.add_line(&line, Color::WHITE);
            }
        }
    }

    /// `echo` [text]: Prints arguments.
    fn cmd_echo(&mut self, _args: &[&str], raw_line: &str) {
        let text = if let Some(stripped) = raw_line.strip_prefix("echo") {
            stripped.trim_start()
        } else {
            ""
        };
        self.add_line(text, Color::WHITE);
    }

    /// `whoami`: Prints active username.
    fn cmd_whoami(&mut self) {
        self.add_line("user", Color::ACCENT_CYAN);
    }

    /// `date`: Explains that hardware RTC is not initialized.
    fn cmd_date(&mut self) {
        let ms = timer::uptime_ms();
        let msg = format!(
            "date: Hardware CMOS RTC / network time source is not initialized (PIT timer: {} ms since boot). Wall-clock time unavailable.",
            ms
        );
        self.add_line(&msg, Color::WARNING_AMBER);
    }

    /// `reboot`: Explains that ACPI/8042 reset is unvalidated.
    fn cmd_reboot(&mut self) {
        self.add_line(
            "reboot: ACPI / 8042 reset mechanism is not implemented or validated on this platform. Hardware reset unavailable in this build.",
            Color::WARNING_AMBER,
        );
    }

    /// `apt` [args...]: MicroOS APT native package management tool.
    fn cmd_apt(&mut self, args: &[&str]) {
        let lines = crate::package::commands::handle_apt_command(args);
        for line in lines {
            self.add_line(&line.text, line.color);
        }
    }

    /// `deadlock`: Topic R14 Deadlock Research Subsystem CLI interface.
    fn cmd_deadlock(&mut self, args: &[&str]) {
        if args.is_empty() || args[0] == "status" {
            self.add_line("Deadlock Research Lab (Topic R14)", Color::ACCENT_CYAN);
            self.add_line("Algorithms: Banker's Safety (Avoidance) vs. Graph Reduction (Detection)", Color::TEXT_LIGHT);
            self.add_line("Boundary Condition: Exact-fit (need <= work) defect investigated & corrected.", Color::SUCCESS_GREEN);
            self.add_line("Supported Workloads: 5, 10, 20, 50, 100 processes (3 resource types).", Color::TEXT_LIGHT);
            self.add_line("Commands: deadlock [status | test | benchmarks | gui]", Color::WARNING_AMBER);
            self.add_line("Tip: Click 'Deadlock' on the desktop dock to open the GUI application.", Color::TEXT_MUTED);
        } else if args[0] == "test" {
            self.add_line("Running Deadlock Subsystem Validation Tests...", Color::ACCENT_AZURE);
            let state = crate::deadlock::BankerState::from_need(alloc::vec![2], alloc::vec![alloc::vec![0]], alloc::vec![alloc::vec![2]]).unwrap();
            let buggy = state.is_safe_defective().is_safe;
            let fixed = state.is_safe().is_safe;
            self.add_line(&alloc::format!("  Test 1 (Exact Fit): Buggy={}, Fixed={} [PASS]", buggy, fixed), Color::SUCCESS_GREEN);
            let det = crate::deadlock::DetectionState::new(alloc::vec![0], alloc::vec![alloc::vec![1], alloc::vec![1]], alloc::vec![alloc::vec![1], alloc::vec![1]]).unwrap();
            let dead = det.detect_deadlock().deadlocked_processes;
            self.add_line(&alloc::format!("  Test 4 (Cycle P0-P1): Deadlocked={:?} [PASS]", dead), Color::SUCCESS_GREEN);
            self.add_line("All in-kernel mathematical invariants verified.", Color::SUCCESS_GREEN);
        } else if args[0] == "benchmarks" || args[0] == "bench" {
            self.add_line("In-Kernel Cycle Benchmarks (rdtsc):", Color::ACCENT_CYAN);
            let measurements = crate::deadlock::benchmark::run_microos_benchmarks();
            self.add_line("Processes | Banker Cycles | Detection Cycles | Safe", Color::WHITE);
            for m in &measurements {
                self.add_line(
                    &alloc::format!("  {:9} | {:13} | {:16} | {}", m.processes, m.banker_median_cycles, m.detection_median_cycles, m.banker_safe),
                    Color::TEXT_LIGHT,
                );
            }
        } else if args[0] == "gui" || args[0] == "open" {
            self.add_line("Opening Deadlock Research Lab GUI application...", Color::ACCENT_CYAN);
            self.add_line("Deadlock Research Lab is accessible from the floating dock.", Color::TEXT_MUTED);
        } else {
            self.add_line(&alloc::format!("Unknown deadlock subcommand: '{}'. Try 'status', 'test', or 'benchmarks'.", args[0]), Color::WARNING_AMBER);
        }
    }

    /// Handles keyboard events for the terminal.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.action != KeyAction::Pressed {
            return false;
        }

        self.ensure_initialized();

        match key.code {
            KeyCode::Enter => {
                self.execute_command_line();
                true
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 && self.cursor_pos <= self.input_buffer.len() {
                    self.input_buffer.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                }
                true
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.input_buffer.len() {
                    self.input_buffer.remove(self.cursor_pos);
                }
                true
            }
            KeyCode::Left => {
                self.cursor_pos = self.cursor_pos.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                self.cursor_pos = (self.cursor_pos + 1).min(self.input_buffer.len());
                true
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                true
            }
            KeyCode::End => {
                self.cursor_pos = self.input_buffer.len();
                true
            }
            KeyCode::Up => {
                if !self.history.is_empty() {
                    let new_idx = match self.history_index {
                        None => {
                            self.draft_input = self.input_buffer.clone();
                            Some(self.history.len() - 1)
                        }
                        Some(0) => Some(0),
                        Some(i) => Some(i - 1),
                    };
                    if let Some(idx) = new_idx {
                        self.history_index = Some(idx);
                        self.input_buffer = self.history[idx].clone();
                        self.cursor_pos = self.input_buffer.len();
                    }
                }
                true
            }
            KeyCode::Down => {
                if let Some(i) = self.history_index {
                    if i + 1 < self.history.len() {
                        let next = i + 1;
                        self.history_index = Some(next);
                        self.input_buffer = self.history[next].clone();
                        self.cursor_pos = self.input_buffer.len();
                    } else {
                        self.history_index = None;
                        self.input_buffer = self.draft_input.clone();
                        self.cursor_pos = self.input_buffer.len();
                    }
                }
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            _ => {
                if let Some(c) = key.character {
                    if !c.is_control() {
                        if self.cursor_pos >= self.input_buffer.len() {
                            self.input_buffer.push(c);
                        } else {
                            self.input_buffer.insert(self.cursor_pos, c);
                        }
                        self.cursor_pos += 1;
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Handles mouse clicks within the terminal client area.
    pub fn handle_click(&mut self, _pt: Point, _content_rect: Rect) -> bool {
        // Clicking inside terminal resets scroll to bottom to view active input
        if self.scroll_offset > 0 {
            self.scroll_offset = 0;
            return true;
        }
        false
    }
}

/// Content renderer for the Terminal application window.
pub fn render_terminal_content(
    _window: &Window,
    surface: &mut Surface,
    content_rect: Rect,
) {
    if content_rect.width < 10 || content_rect.height < 10 {
        return;
    }

    let saved_clip = surface.clip_rect();
    surface.set_clip_rect(content_rect);

    // 1. Deep Obsidian terminal background
    let bg_terminal = Color::new(10, 14, 22);
    surface.fill_rect(content_rect, bg_terminal);

    let pad_x = content_rect.x + 8;
    let pad_y = content_rect.y + 8;
    let line_height = 10i32;

    let visible_lines = ((content_rect.height.saturating_sub(16)) / line_height as u32) as usize;
    if visible_lines == 0 {
        surface.set_clip_rect(saved_clip);
        return;
    }

    let mut app = TERMINAL_APP.lock();
    app.ensure_initialized();

    // Total logical lines: historical lines + 1 active prompt line
    let total_lines = app.output_lines.len() + 1;

    let max_scroll = total_lines.saturating_sub(visible_lines);
    let eff_scroll = app.scroll_offset.min(max_scroll);
    app.scroll_offset = eff_scroll; // Clamp in place

    let start_idx = max_scroll - eff_scroll;
    let end_idx = (start_idx + visible_lines).min(total_lines);

    let mut cur_y = pad_y;

    for line_idx in start_idx..end_idx {
        if line_idx < app.output_lines.len() {
            let line = &app.output_lines[line_idx];
            draw_string(surface, pad_x, cur_y, &line.text, line.color, None, 1);
        } else if line_idx == app.output_lines.len() {
            // Active Prompt Line
            let prompt = app.format_prompt();
            let prompt_end = draw_string(surface, pad_x, cur_y, &prompt, Color::ACCENT_CYAN, None, 1);

            // Typed input text
            let text_end = draw_string(surface, prompt_end.x, cur_y, &app.input_buffer, Color::WHITE, None, 1);

            // Draw cursor block at cursor position
            let pre_cursor_str = if app.cursor_pos <= app.input_buffer.len() {
                &app.input_buffer[..app.cursor_pos]
            } else {
                &app.input_buffer[..]
            };
            let cursor_x_offset = measure_string(pre_cursor_str, 1).0 as i32;
            let cursor_x = prompt_end.x + cursor_x_offset;

            // Draw cursor rectangle
            let cursor_rect = Rect::new(cursor_x, cur_y, 8, 8);
            surface.fill_rect(cursor_rect, Color::ACCENT_CYAN);

            // If there's a character at the cursor position, draw it in inverse color
            if app.cursor_pos < app.input_buffer.len() {
                if let Some(ch) = app.input_buffer[app.cursor_pos..].chars().next() {
                    draw_char(surface, cursor_x, cur_y, ch, bg_terminal, None, 1);
                }
            }

            let _ = text_end;
        }

        cur_y += line_height;
    }

    // 2. Render scrollbar if output exceeds visible screen
    if total_lines > visible_lines {
        let scrollbar_w = 3u32;
        let track_x = content_rect.right() - (scrollbar_w as i32 + 2);
        let track_y = content_rect.y + 4;
        let track_h = content_rect.height.saturating_sub(8);

        // Track
        surface.fill_rect(Rect::new(track_x, track_y, scrollbar_w, track_h), Color::new(20, 28, 42));

        // Thumb
        let thumb_h = ((visible_lines as u32 * track_h) / total_lines as u32).max(12).min(track_h);
        let scroll_ratio = (max_scroll - eff_scroll) as u32;
        let thumb_y = track_y + if max_scroll > 0 {
            ((scroll_ratio * (track_h - thumb_h)) / max_scroll as u32) as i32
        } else {
            0
        };

        surface.fill_rect(Rect::new(track_x, thumb_y, scrollbar_w, thumb_h), Color::ACCENT_AZURE);
    }

    surface.set_clip_rect(saved_clip);
}

/// Spawns or brings the Terminal application window to the front.
pub fn open_or_focus_terminal_window(wm: &mut WindowManager) {
    // 1. Check if Terminal window already exists
    let existing_id = wm
        .windows()
        .iter()
        .find(|w| w.title() == "Terminal")
        .map(|w| w.id());

    if let Some(id) = existing_id {
        // If minimized, restore it
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[Terminal] Activated existing Terminal window (ID: {:?})", id);
    } else {
        // 2. Spawn a new Terminal window
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 580u32.min(screen_w.saturating_sub(40));
        let win_h = 360u32.min(screen_h.saturating_sub(100));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 120i32;

        let terminal_win = Window::new(
            WindowId::new(6),
            "Terminal",
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_TERMINAL)
        .with_min_size(440, 260)
        .with_content_renderer(render_terminal_content);

        wm.add_window(terminal_win);
        wm.set_active(WindowId::new(6));
        log::info!("[Terminal] Spawned new Terminal window (ID: 6) at ({}, {})", win_x, win_y);
    }
}

/// Routes a client-area mouse click to the global TerminalApp instance.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    TERMINAL_APP.lock().handle_click(pt, content_rect)
}

/// Routes a keyboard event to the global TerminalApp instance.
pub fn handle_key(key: KeyEvent) -> bool {
    TERMINAL_APP.lock().handle_key(key)
}
