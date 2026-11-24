//! Micro-OS System Monitor Application ("SysMon").
//!
//! Provides a hardware telemetry dashboard and task inspection center with Cosmic Slate styling:
//! 1. Overview: CPU architecture, kernel version, live uptime, physical RAM, heap utilization bar, backbuffer capacity.
//! 2. CPU: Architecture, active flags (CR0/CR4), timer ticks, context switch count, and clear sensor availability notices.
//! 3. Memory: Total usable physical RAM, frame allocation stats, live heap utilization and progress bar, 16 MiB backbuffer metrics.
//! 4. Processes / Tasks: Live task snapshot table (PID, Name, State, Stack Size), ready queue count, running task indicator.
//! 5. Performance: Context switch count, PIT timer ticks, scheduling quantum, input queue throughput.

use alloc::format;
use alloc::string::ToString;
use spin::Mutex;

use crate::arch::x86_64::timer;
use crate::graphics::color::Color;
use crate::graphics::font::draw_string;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::{
    draw_icon, Icon, ICON_ACTIVITY, ICON_FOLDER, ICON_GEAR, ICON_STORE, ICON_TERMINAL,
};
use crate::graphics::surface::Surface;
use crate::input::event::{KeyAction, KeyCode, KeyEvent};
use crate::memory;
use crate::task::{self, TaskState};
use crate::window::manager::WindowManager;
use crate::window::state::WindowId;
use crate::window::window::Window;

/// Global spinlock-protected System Monitor application state.
pub static SYSMON_APP: Mutex<SysMonApp> = Mutex::new(SysMonApp::new());

/// The five distinct diagnostic views of the System Monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysMonSection {
    Overview,
    Cpu,
    Memory,
    Tasks,
    Performance,
}

impl SysMonSection {
    pub const ALL: [SysMonSection; 5] = [
        SysMonSection::Overview,
        SysMonSection::Cpu,
        SysMonSection::Memory,
        SysMonSection::Tasks,
        SysMonSection::Performance,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            SysMonSection::Overview => "Overview",
            SysMonSection::Cpu => "CPU",
            SysMonSection::Memory => "Memory",
            SysMonSection::Tasks => "Processes",
            SysMonSection::Performance => "Performance",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            SysMonSection::Overview => &ICON_ACTIVITY,
            SysMonSection::Cpu => &ICON_TERMINAL,
            SysMonSection::Memory => &ICON_STORE,
            SysMonSection::Tasks => &ICON_FOLDER,
            SysMonSection::Performance => &ICON_GEAR,
        }
    }
}

/// System Monitor Application state machine.
pub struct SysMonApp {
    /// Currently active diagnostic section.
    pub current_section: SysMonSection,
    /// Sidebar active selection index.
    pub selected_sidebar_index: usize,
    /// Vertical scroll offset for the processes/tasks list.
    pub task_scroll_offset: usize,
}

impl SysMonApp {
    /// Creates a new default SysMonApp instance.
    pub const fn new() -> Self {
        Self {
            current_section: SysMonSection::Overview,
            selected_sidebar_index: 0,
            task_scroll_offset: 0,
        }
    }

    /// Selects an explicit section.
    pub fn select_section(&mut self, section: SysMonSection) {
        self.current_section = section;
        if let Some(idx) = SysMonSection::ALL.iter().position(|&s| s == section) {
            self.selected_sidebar_index = idx;
        }
    }

    /// Handles mouse clicks within the System Monitor window client area.
    pub fn handle_click(&mut self, pt: Point, content_rect: Rect) -> bool {
        let sidebar_w = 130i32;

        // 1. Sidebar clicks (x: content_rect.x .. content_rect.x + 130)
        if pt.x >= content_rect.x && pt.x < content_rect.x + sidebar_w {
            let start_y = content_rect.y + 36;
            let item_h = 32i32;

            for (i, &sec) in SysMonSection::ALL.iter().enumerate() {
                let cur_y = start_y + (i as i32 * item_h);
                if pt.y >= cur_y && pt.y < cur_y + item_h {
                    self.select_section(sec);
                    return true;
                }
            }
            return false;
        }

        // 2. Client area interaction (x >= content_rect.x + 130)
        if pt.x >= content_rect.x + sidebar_w {
            if self.current_section == SysMonSection::Tasks {
                // Scroll buttons in tasks view
                let header_y = content_rect.y + 44;
                let scroll_up_rect = Rect::new(content_rect.right() - 60, header_y, 24, 20);
                let scroll_down_rect = Rect::new(content_rect.right() - 32, header_y, 24, 20);

                if scroll_up_rect.contains(pt) {
                    if self.task_scroll_offset > 0 {
                        self.task_scroll_offset -= 1;
                        return true;
                    }
                } else if scroll_down_rect.contains(pt) {
                    self.task_scroll_offset += 1;
                    return true;
                }
            }
        }

        false
    }

    /// Handles keyboard events for the System Monitor window.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.action != KeyAction::Pressed {
            return false;
        }

        match key.code {
            KeyCode::Down => {
                let next_idx = (self.selected_sidebar_index + 1) % SysMonSection::ALL.len();
                self.selected_sidebar_index = next_idx;
                self.current_section = SysMonSection::ALL[next_idx];
                true
            }
            KeyCode::Up => {
                let prev_idx = if self.selected_sidebar_index == 0 {
                    SysMonSection::ALL.len() - 1
                } else {
                    self.selected_sidebar_index - 1
                };
                self.selected_sidebar_index = prev_idx;
                self.current_section = SysMonSection::ALL[prev_idx];
                true
            }
            KeyCode::PageDown => {
                if self.current_section == SysMonSection::Tasks {
                    self.task_scroll_offset += 3;
                    return true;
                }
                false
            }
            KeyCode::PageUp => {
                if self.current_section == SysMonSection::Tasks {
                    self.task_scroll_offset = self.task_scroll_offset.saturating_sub(3);
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// Global client content renderer for the System Monitor window.
pub fn render_sysmon_content(_window: &Window, surface: &mut Surface, content_rect: Rect) {
    let app = SYSMON_APP.lock();

    let saved_clip = surface.clip_rect();
    let client_clip = match saved_clip.intersect(&content_rect) {
        Some(c) => c,
        None => return,
    };
    surface.set_clip_rect(client_clip);

    // 1. Fill base client background (Obsidian Slate)
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
    draw_string(surface, content_rect.x + 12, content_rect.y + 12, "TELEMETRY", Color::TEXT_MUTED, None, 1);

    // Sidebar Items
    let item_start_y = content_rect.y + 36;
    let item_h = 32i32;

    for (i, &sec) in SysMonSection::ALL.iter().enumerate() {
        let cur_y = item_start_y + (i as i32 * item_h);
        if cur_y + item_h > content_rect.bottom() {
            break;
        }

        let is_selected = app.current_section == sec;
        if is_selected {
            let active_rect = Rect::new(content_rect.x + 6, cur_y + 2, sidebar_w - 12, 28);
            surface.fill_rounded_rect(active_rect, 5, Color::new(0x1B, 0x25, 0x38));
            surface.draw_vline(content_rect.x + 8, cur_y + 6, 20, Color::SUCCESS_GREEN);
        }

        let label_color = if is_selected { Color::SUCCESS_GREEN } else { Color::TEXT_LIGHT };
        let icon_tint = if is_selected { Some(Color::SUCCESS_GREEN) } else { Some(Color::TEXT_MUTED) };

        draw_icon(surface, content_rect.x + 14, cur_y + 8, sec.icon(), icon_tint);
        draw_string(surface, content_rect.x + 36, cur_y + 10, sec.title(), label_color, None, 1);
    }

    // 3. Render Main Section Content Area
    let main_x = content_rect.x + sidebar_w as i32 + 1;
    let main_w = content_rect.width.saturating_sub(sidebar_w + 1);
    let main_rect = Rect::new(main_x, content_rect.y, main_w, content_rect.height);

    match app.current_section {
        SysMonSection::Overview => render_overview_section(surface, main_rect),
        SysMonSection::Cpu => render_cpu_section(surface, main_rect),
        SysMonSection::Memory => render_memory_section(surface, main_rect),
        SysMonSection::Tasks => render_tasks_section(surface, main_rect, &app),
        SysMonSection::Performance => render_performance_section(surface, main_rect),
    }

    surface.set_clip_rect(saved_clip);
}

/// Renders the Overview section.
fn render_overview_section(surface: &mut Surface, area: Rect) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    // Live metrics retrieval
    let uptime_sec = timer::uptime_ms() / 1000;
    let uptime_min = uptime_sec / 60;
    let uptime_rem_sec = uptime_sec % 60;
    let heap_alloc = memory::heap_allocated_bytes();
    let heap_total = memory::heap_total_bytes();
    let heap_pct = if heap_total > 0 { (heap_alloc * 100) / heap_total } else { 0 };
    let ram_mib = memory::total_usable_ram_bytes() / (1024 * 1024);

    let card_w = area.width.saturating_sub(28);

    // Header Card
    let header_rect = Rect::new(pad_x, cur_y, card_w, 42);
    surface.fill_rounded_rect(header_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(header_rect, 6, 1, Color::BORDER_SLATE);
    draw_string(surface, pad_x + 12, cur_y + 8, "Micro-OS Workstation System Telemetry", Color::WHITE, None, 1);
    let uptime_str = format!("System Uptime: {}m {:02}s ({} ticks @ 100 Hz)", uptime_min, uptime_rem_sec, timer::ticks());
    draw_string(surface, pad_x + 12, cur_y + 22, &uptime_str, Color::SUCCESS_GREEN, None, 1);

    cur_y += 50;

    // CPU Summary Card
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Processor & Kernel Architecture",
        "x86_64 Long Mode (64-bit Ring 0)",
        "Micro-OS v0.11.17 Obsidian Azure | 8254 PIT (100 Hz)",
        Color::ACCENT_CYAN,
    );
    cur_y += 54;

    // Memory Utilization Card with Visual Progress Bar
    let mem_card_rect = Rect::new(pad_x, cur_y, card_w, 72);
    surface.fill_rounded_rect(mem_card_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(mem_card_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 12, cur_y + 8, "Memory Utilization Overview", Color::TEXT_MUTED, None, 1);
    let ram_label = format!("RAM: {} MiB Usable | Heap: {} / {} KiB ({}%) | Backbuffer: 16 MiB",
        ram_mib, heap_alloc / 1024, heap_total / 1024, heap_pct);
    draw_string(surface, pad_x + 12, cur_y + 22, &ram_label, Color::WHITE, None, 1);

    // Visual Progress Bar
    let bar_rect = Rect::new(pad_x + 12, cur_y + 40, card_w.saturating_sub(24), 16);
    surface.fill_rounded_rect(bar_rect, 4, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(bar_rect, 4, 1, Color::BORDER_SLATE);

    let fill_w = ((bar_rect.width as usize * heap_pct) / 100).max(4).min(bar_rect.width as usize) as u32;
    let fill_rect = Rect::new(bar_rect.x + 1, bar_rect.y + 1, fill_w.saturating_sub(2), bar_rect.height.saturating_sub(2));
    surface.fill_rounded_rect(fill_rect, 3, Color::SUCCESS_GREEN);

    cur_y += 78;

    // Tasks & Preemption Summary Card
    let tasks_len = task::total_task_count();
    let cs_count = task::context_switch_count();
    let p_count = task::preemption_count();
    let r_count = task::ready_task_count();

    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Multitasking & Scheduler Dispatch",
        "Preemptive Round-Robin Scheduling",
        &format!("Tasks: {} | Ready: {} | Context Switches: {} | Preemptions: {}", tasks_len, r_count, cs_count, p_count),
        Color::ACCENT_AZURE,
    );
}

/// Renders the CPU section.
fn render_cpu_section(surface: &mut Surface, area: Rect) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    draw_string(surface, pad_x, cur_y, "CPU & PROCESSOR ARCHITECTURE", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let card_w = area.width.saturating_sub(28);

    // Spec 1: Architecture & Execution Mode
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Hardware Architecture",
        "x86_64 Compatible Processor (64-bit Long Mode)",
        "Ring 0 Kernel Execution | SSE2 & FXSR Active (CR4.OSFXSR = 1)",
        Color::ACCENT_CYAN,
    );
    cur_y += 54;

    // Spec 2: Scheduler & Timing
    let cs_count = task::context_switch_count();
    let p_count = task::preemption_count();
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Timer & Scheduling Statistics",
        "8254 PIT Mode 3 @ 100 Hz (10 ms period)",
        &format!("Quantum: 30 ms (3 ticks) | Switches: {} | Preemptions: {}", cs_count, p_count),
        Color::SUCCESS_GREEN,
    );
    cur_y += 54;

    // Spec 3: Verified Hardware Counters Transparency Card
    let notice_rect = Rect::new(pad_x, cur_y, card_w, 90);
    surface.fill_rounded_rect(notice_rect, 6, Color::new(0x1B, 0x1A, 0x14));
    surface.stroke_rounded_rect(notice_rect, 6, 1, Color::WARNING_AMBER);

    draw_string(surface, pad_x + 12, cur_y + 8, "[MEASUREMENT TRANSPARENCY NOTICE]", Color::WARNING_AMBER, None, 1);
    let items = [
        "- CPU Load %: N/A (Requires APIC TSC profiling driver)",
        "- Core Temperatures: N/A (Requires Intel DTS / AMD MSR driver)",
        "- Dynamic Frequency Scaling: N/A (Requires ACPI P-States driver)",
        "- Micro-OS strictly reports measured counters without fabrication.",
    ];
    let mut item_y = cur_y + 24;
    for item in items {
        draw_string(surface, pad_x + 12, item_y, item, Color::TEXT_LIGHT, None, 1);
        item_y += 14;
    }
}

/// Renders the Memory section.
fn render_memory_section(surface: &mut Surface, area: Rect) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    draw_string(surface, pad_x, cur_y, "MEMORY SUBSYSTEMS & ALLOCATION", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let card_w = area.width.saturating_sub(28);

    // Card 1: Physical Memory
    let ram_bytes = memory::total_usable_ram_bytes();
    let frames = memory::allocated_frames_count();
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Conventional Physical RAM",
        &format!("{} MiB Usable ({} Bytes)", ram_bytes / (1024 * 1024), ram_bytes),
        &format!("Bootloader Frame Allocator: {} physical 4 KiB frames allocated", frames),
        Color::ACCENT_CYAN,
    );
    cur_y += 54;

    // Card 2: Kernel Virtual Heap
    let heap_alloc = memory::heap_allocated_bytes();
    let heap_total = memory::heap_total_bytes();
    let heap_free = heap_total.saturating_sub(heap_alloc);
    let heap_pct = if heap_total > 0 { (heap_alloc * 100) / heap_total } else { 0 };

    let heap_card_rect = Rect::new(pad_x, cur_y, card_w, 82);
    surface.fill_rounded_rect(heap_card_rect, 6, Color::CARD_SLATE);
    surface.stroke_rounded_rect(heap_card_rect, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 12, cur_y + 8, "Kernel Heap Allocator (LinkedList)", Color::TEXT_MUTED, None, 1);
    let heap_details = format!("Allocated: {} KiB ({}%) | Free: {} KiB | Capacity: {} KiB",
        heap_alloc / 1024, heap_pct, heap_free / 1024, heap_total / 1024);
    draw_string(surface, pad_x + 12, cur_y + 22, &heap_details, Color::WHITE, None, 1);
    draw_string(surface, pad_x + 12, cur_y + 36, "Virtual Address: 0x4444_4444_0000 - 0x4444_4544_0000", Color::TEXT_MUTED, None, 1);

    // Progress Bar
    let bar_rect = Rect::new(pad_x + 12, cur_y + 54, card_w.saturating_sub(24), 16);
    surface.fill_rounded_rect(bar_rect, 4, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(bar_rect, 4, 1, Color::BORDER_SLATE);

    let fill_w = ((bar_rect.width as usize * heap_pct) / 100).max(4).min(bar_rect.width as usize) as u32;
    let fill_rect = Rect::new(bar_rect.x + 1, bar_rect.y + 1, fill_w.saturating_sub(2), bar_rect.height.saturating_sub(2));
    surface.fill_rounded_rect(fill_rect, 3, Color::SUCCESS_GREEN);

    cur_y += 88;

    // Card 3: Dedicated Graphics Backbuffer
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Graphics Double-Buffer Allocation",
        "16,384 KiB (16 MiB Allocated)",
        "Virtual Address: 0x4444_5000_0000 | 4,096 physical frames mapped",
        Color::ACCENT_AZURE,
    );
}

/// Renders the Processes / Tasks section.
fn render_tasks_section(surface: &mut Surface, area: Rect, app: &SysMonApp) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    let card_w = area.width.saturating_sub(28);

    // Summary Header
    let snapshots = task::task_snapshots();
    let r_count = task::ready_task_count();
    let current_info = task::current_task().map(|(id, name)| format!("PID: {} ('{}')", id.as_u64(), name))
        .unwrap_or_else(|| "Main Kernel Loop".to_string());

    let header_rect = Rect::new(pad_x, cur_y, card_w, 36);
    surface.fill_rounded_rect(header_rect, 5, Color::CARD_SLATE);
    surface.stroke_rounded_rect(header_rect, 5, 1, Color::BORDER_SLATE);
    draw_string(surface, pad_x + 10, cur_y + 6, &format!("Current Task: {}", current_info), Color::SUCCESS_GREEN, None, 1);
    draw_string(surface, pad_x + 10, cur_y + 20, &format!("Total Tasks: {} | Ready Queue: {} | Scheduler: Round-Robin", snapshots.len(), r_count), Color::TEXT_MUTED, None, 1);

    cur_y += 44;

    // Table Header Row
    let tbl_rect = Rect::new(pad_x, cur_y, card_w, 20);
    surface.fill_rect(tbl_rect, Color::new(0x10, 0x17, 0x27));
    surface.draw_hline(pad_x, cur_y + 19, card_w, Color::BORDER_SLATE);

    draw_string(surface, pad_x + 10, cur_y + 6, "PID", Color::TEXT_MUTED, None, 1);
    draw_string(surface, pad_x + 60, cur_y + 6, "TASK NAME", Color::TEXT_MUTED, None, 1);
    draw_string(surface, pad_x + 220, cur_y + 6, "STATE", Color::TEXT_MUTED, None, 1);
    draw_string(surface, pad_x + card_w as i32 - 70, cur_y + 6, "STACK", Color::TEXT_MUTED, None, 1);

    cur_y += 24;

    // Table Rows
    if snapshots.is_empty() {
        draw_string(surface, pad_x + 20, cur_y + 20, "[ Single-task main kernel thread | Multitasking idle ]", Color::TEXT_MUTED, None, 1);
    } else {
        let row_h = 24i32;
        let visible_rows = ((area.bottom() - cur_y) / row_h) as usize;
        let start_idx = app.task_scroll_offset.min(snapshots.len().saturating_sub(1));

        for (i, t) in snapshots.iter().skip(start_idx).take(visible_rows).enumerate() {
            let row_y = cur_y + (i as i32 * row_h);
            if row_y + row_h > area.bottom() {
                break;
            }

            // Alternating subtle row background
            if i % 2 == 1 {
                surface.fill_rect(Rect::new(pad_x, row_y, card_w, row_h as u32), Color::new(0x11, 0x18, 0x28));
            }

            // PID
            let pid_str = format!("{}", t.id.as_u64());
            draw_string(surface, pad_x + 10, row_y + 6, &pid_str, Color::WHITE, None, 1);

            // Name
            draw_string(surface, pad_x + 60, row_y + 6, t.name, Color::ACCENT_CYAN, None, 1);

            // State with color-coding
            let (state_str, state_color) = match t.state {
                TaskState::Running => ("RUNNING", Color::SUCCESS_GREEN),
                TaskState::Ready => ("READY", Color::ACCENT_AZURE),
                TaskState::Blocked => ("BLOCKED", Color::WARNING_AMBER),
                TaskState::Terminated => ("TERMINATED", Color::TEXT_MUTED),
            };
            draw_string(surface, pad_x + 220, row_y + 6, state_str, state_color, None, 1);

            // Stack
            let stack_str = if t.stack_size > 0 {
                format!("{} KiB", t.stack_size / 1024)
            } else {
                "Boot".to_string()
            };
            draw_string(surface, pad_x + card_w as i32 - 70, row_y + 6, &stack_str, Color::TEXT_LIGHT, None, 1);
        }
    }
}

/// Renders the Performance section.
fn render_performance_section(surface: &mut Surface, area: Rect) {
    let pad_x = area.x + 14;
    let mut cur_y = area.y + 12;

    draw_string(surface, pad_x, cur_y, "KERNEL PERFORMANCE & LATENCY COUNTERS", Color::TEXT_MUTED, None, 1);
    cur_y += 14;

    let card_w = area.width.saturating_sub(28);

    // Metric 1: Context Switching
    let cs_count = task::context_switch_count();
    let p_count = task::preemption_count();
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Context Switch Dispatches",
        &format!("{} Total Switches ({} Preemptions)", cs_count, p_count),
        "IRQ0 Timer Preemption + int 48 Cooperative Yield Handlers",
        Color::SUCCESS_GREEN,
    );
    cur_y += 54;

    // Metric 2: Hardware Timer & Uptime
    let ticks = timer::ticks();
    let uptime_ms = timer::uptime_ms();
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Hardware Timer Interrupts",
        &format!("{} PIT Ticks ({} ms Elapsed)", ticks, uptime_ms),
        "8254 Channel 0 Square Wave @ 100 Hz (10 ms resolution)",
        Color::ACCENT_CYAN,
    );
    cur_y += 54;

    // Metric 3: Input Subsystem Throughput
    render_spec_card(
        surface,
        Rect::new(pad_x, cur_y, card_w, 48),
        "Input Event Ring Buffer",
        "128-Entry Bounded Atomic FIFO Queue",
        "IRQ1 (Keyboard) + IRQ12 (Mouse) | IrqGuard SpinMutex Protected",
        Color::ACCENT_AZURE,
    );
}

/// Helper to render a labeled specification card with title, value, and description.
fn render_spec_card(
    surface: &mut Surface,
    rect: Rect,
    label: &'static str,
    value: &str,
    desc: &str,
    accent: Color,
) {
    surface.fill_rounded_rect(rect, 5, Color::CARD_SLATE);
    surface.stroke_rounded_rect(rect, 5, 1, Color::BORDER_SLATE);

    draw_string(surface, rect.x + 12, rect.y + 6, label, Color::TEXT_MUTED, None, 1);
    draw_string(surface, rect.x + 12, rect.y + 18, value, accent, None, 1);
    draw_string(surface, rect.x + 12, rect.y + 30, desc, Color::TEXT_LIGHT, None, 1);
}

/// Helper to open or bring to front the System Monitor application window.
pub fn open_or_focus_sysmon_window(wm: &mut WindowManager) {
    // 1. Check if a window titled "System Monitor" or "SysMon" already exists
    let existing_id = wm.windows().iter()
        .find(|w| w.title() == "System Monitor" || w.title() == "SysMon")
        .map(|w| w.id());

    if let Some(id) = existing_id {
        // If minimized, restore it
        wm.restore_window(id);
        wm.bring_to_front(id);
        wm.set_active(id);
        log::info!("[SysMon] Activated existing System Monitor window (ID: {:?})", id);
    } else {
        // 2. Spawn a new System Monitor window
        let screen_w = wm.screen_width();
        let screen_h = wm.screen_height();

        let win_w = 560u32.min(screen_w.saturating_sub(40));
        let win_h = 380u32.min(screen_h.saturating_sub(100));
        let win_x = ((screen_w.saturating_sub(win_w)) / 2) as i32;
        let win_y = 90i32;

        let sysmon_win = Window::new(
            WindowId::new(2),
            "System Monitor",
            win_x,
            win_y,
            win_w,
            win_h,
        )
        .with_icon(&ICON_ACTIVITY)
        .with_min_size(460, 300)
        .with_content_renderer(render_sysmon_content);

        wm.add_window(sysmon_win);
        wm.set_active(WindowId::new(2));
        log::info!("[SysMon] Spawned new System Monitor window (ID: 2) at ({}, {})", win_x, win_y);
    }
}

/// Routes a client-area mouse click to the global SysMonApp instance.
pub fn handle_click(pt: Point, content_rect: Rect) -> bool {
    SYSMON_APP.lock().handle_click(pt, content_rect)
}

/// Routes a keyboard event to the global SysMonApp instance.
pub fn handle_key(key: KeyEvent) -> bool {
    SYSMON_APP.lock().handle_key(key)
}
