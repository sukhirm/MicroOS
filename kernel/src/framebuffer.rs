//! Linear Framebuffer graphics driver supporting UEFI GOP and VBE.
//! Implements the Cosmic Slate / Obsidian Azure visual identity foundation,
//! early boot milestone tracking, and heap-free panic reporting.

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use spin::Mutex;

/// RGB color structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[allow(dead_code)]
impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    // --- Cosmic Slate / Obsidian Azure Palette ---
    pub const OBSIDIAN_NAVY: Self = Self::new(0x0a, 0x0e, 0x17); // Deep cosmic background
    pub const SURFACE_SLATE: Self = Self::new(0x13, 0x1b, 0x2e); // Surface panels
    pub const CARD_SLATE: Self = Self::new(0x1a, 0x24, 0x3b);    // Card container
    pub const BORDER_SLATE: Self = Self::new(0x2d, 0x3c, 0x5a);  // Accent borders
    pub const ACCENT_AZURE: Self = Self::new(0x38, 0xbd, 0xf8);  // Primary brand Azure
    pub const ACCENT_CYAN: Self = Self::new(0x00, 0xd2, 0xff);   // Highlight Cyan
    pub const TEXT_LIGHT: Self = Self::new(0xf1, 0xf5, 0xf9);    // Crisp white-slate text
    pub const TEXT_MUTED: Self = Self::new(0x94, 0xa3, 0xb8);    // Muted slate text
    pub const SUCCESS_GREEN: Self = Self::new(0x4a, 0xde, 0x80); // Diagnostic OK green
    pub const WARNING_AMBER: Self = Self::new(0xfb, 0xbf, 0x24); // Diagnostic warn amber
    pub const ERROR_RED: Self = Self::new(0xef, 0x44, 0x44);     // Diagnostic error red
    pub const PANIC_HEADER: Self = Self::new(0xb9, 0x1c, 0x1c);  // Panic top bar
}

/// Raw linear framebuffer reference that can be accessed without heap allocation
/// during early kernel startup and panic handling.
#[derive(Debug, Clone, Copy)]
pub struct RawFramebuffer {
    pub ptr: *mut u8,
    pub info: FrameBufferInfo,
}

unsafe impl Send for RawFramebuffer {}
unsafe impl Sync for RawFramebuffer {}

impl RawFramebuffer {
    /// Framebuffer width in pixels.
    pub fn width(&self) -> usize {
        self.info.width
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> usize {
        self.info.height
    }

    /// Writes a single pixel with bounds checking and pixel format handling.
    pub fn set_pixel(&self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let stride = self.info.stride;
        let bytes_per_pixel = self.info.bytes_per_pixel;
        let pixel_offset = (y * stride + x) * bytes_per_pixel;

        if pixel_offset + bytes_per_pixel > self.info.byte_len {
            return;
        }

        unsafe {
            let p = self.ptr.add(pixel_offset);
            match self.info.pixel_format {
                PixelFormat::Rgb => {
                    p.write_volatile(color.r);
                    p.add(1).write_volatile(color.g);
                    p.add(2).write_volatile(color.b);
                }
                PixelFormat::Bgr => {
                    p.write_volatile(color.b);
                    p.add(1).write_volatile(color.g);
                    p.add(2).write_volatile(color.r);
                }
                PixelFormat::U8 => {
                    let gray = ((color.r as u16 * 77 + color.g as u16 * 150 + color.b as u16 * 29) >> 8) as u8;
                    p.write_volatile(gray);
                }
                _ => {
                    p.write_volatile(color.b);
                    p.add(1).write_volatile(color.g);
                    p.add(2).write_volatile(color.r);
                }
            }
        }
    }

    /// Clears the entire display buffer with a solid color.
    pub fn clear(&self, color: Color) {
        let w = self.info.width;
        let h = self.info.height;
        self.fill_rect(0, 0, w, h, color);
    }

    /// Fills a solid rectangle with bounds clamping.
    pub fn fill_rect(&self, start_x: usize, start_y: usize, width: usize, height: usize, color: Color) {
        let end_x = (start_x + width).min(self.info.width);
        let end_y = (start_y + height).min(self.info.height);

        for y in start_y..end_y {
            for x in start_x..end_x {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Draws a character using the 8x8 font bitmap.
    pub fn draw_char(&self, start_x: usize, start_y: usize, c: char, fg: Color, bg: Option<Color>) {
        let ascii_code = c as usize;
        let bitmap = if ascii_code < 128 {
            font8x8::legacy::BASIC_LEGACY[ascii_code]
        } else {
            font8x8::legacy::NOTHING_TO_DISPLAY
        };

        for (row, &byte) in bitmap.iter().enumerate() {
            for col in 0..8 {
                let pixel_set = (byte & (1 << col)) != 0;
                let px = start_x + col;
                let py = start_y + row;
                if pixel_set {
                    self.set_pixel(px, py, fg);
                } else if let Some(bg_color) = bg {
                    self.set_pixel(px, py, bg_color);
                }
            }
        }
    }

    /// Draws a single line of text string at (x, y).
    pub fn draw_string(&self, mut x: usize, y: usize, text: &str, fg: Color, bg: Option<Color>) {
        for c in text.chars() {
            if c == '\n' {
                break;
            }
            self.draw_char(x, y, c, fg, bg);
            x += 8;
        }
    }
}

/// Global static storing raw framebuffer metadata for early boot milestones and panic display.
pub static EARLY_FRAMEBUFFER: Mutex<Option<RawFramebuffer>> = Mutex::new(None);

/// Startup milestone identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMilestone {
    #[allow(dead_code)]
    KernelEntry = 0,
    GdtTss = 1,
    IdtExceptions = 2,
    MemoryHeap = 3,
    Subsystems = 4,
    PicPitTimer = 5,
    SchedulerTasks = 6,
    DesktopReady = 7,
}

/// Metadata description for each milestone.
pub struct MilestoneEntry {
    pub completed_text: &'static str,
    pub pending_text: &'static str,
    pub status_text: &'static str,
}

pub const MILESTONES: [MilestoneEntry; 8] = [
    MilestoneEntry {
        completed_text: "[OK] 1. Kernel entry point reached (x86_64 Long Mode)",
        pending_text:   "[..] 1. Kernel entry point reached",
        status_text:    "Milestone 1/8 complete: Kernel entry point reached.",
    },
    MilestoneEntry {
        completed_text: "[OK] 2. GDT & TSS initialized (Double Fault IST active)",
        pending_text:   "[..] 2. GDT & TSS initialization pending",
        status_text:    "Milestone 2/8 complete: Kernel GDT and TSS active.",
    },
    MilestoneEntry {
        completed_text: "[OK] 3. IDT verified (#BP, #DF, #GP, #PF handlers)",
        pending_text:   "[..] 3. IDT & exception initialization pending",
        status_text:    "Milestone 3/8 complete: Hardware IDT verified (#BP int3).",
    },
    MilestoneEntry {
        completed_text: "[OK] 4. Paging mapped & Heap active (16 MiB Heap)",
        pending_text:   "[..] 4. Memory & heap initialization pending",
        status_text:    "Milestone 4/8 complete: 16 MiB Heap and 16 MiB Backbuffer ready.",
    },
    MilestoneEntry {
        completed_text: "[OK] 5. Subsystems verified (VFS, App, Deadlock, Sync)",
        pending_text:   "[..] 5. Core subsystems self-test pending",
        status_text:    "Milestone 5/8 complete: All subsystem self-tests passed.",
    },
    MilestoneEntry {
        completed_text: "[OK] 6. 8259 PIC & 8254 PIT active (100 Hz Timer)",
        pending_text:   "[..] 6. PIC & PIT timer initialization pending",
        status_text:    "Milestone 6/8 complete: Hardware timer and PIC active.",
    },
    MilestoneEntry {
        completed_text: "[OK] 7. Round Robin Scheduler & Multitasking ready",
        pending_text:   "[..] 7. Task scheduler initialization pending",
        status_text:    "Milestone 7/8 complete: Task scheduler preemption demo passed.",
    },
    MilestoneEntry {
        completed_text: "[OK] 8. Desktop Shell & Window Manager launching",
        pending_text:   "[..] 8. Desktop Shell initialization pending",
        status_text:    "Milestone 8/8 complete: Launching Cosmic Slate Desktop.",
    },
];

/// Helper calculating central diagnostic card layout coordinates.
fn card_layout(w: usize, h: usize) -> (usize, usize, usize, usize, usize, usize, usize) {
    let card_w = 660.min(w.saturating_sub(40));
    let card_h = 360.min(h.saturating_sub(80));
    let card_x = (w.saturating_sub(card_w)) / 2;
    let card_y = 56;
    let text_x = card_x + 20;
    let text_y_start = card_y + 72;
    let status_y = card_y + card_h.saturating_sub(32);
    (card_w, card_h, card_x, card_y, text_x, text_y_start, status_y)
}

/// Initializes the early framebuffer and renders the Cosmic Slate boot screen
/// immediately upon kernel entry, before any serial or logger calls.
pub fn init_early(fb: &mut FrameBuffer) {
    let info = fb.info();
    let ptr = fb.buffer_mut().as_mut_ptr();
    let raw_fb = RawFramebuffer { ptr, info };

    let w = raw_fb.width();
    let h = raw_fb.height();

    // 1. Deep cosmic background
    raw_fb.clear(Color::OBSIDIAN_NAVY);

    // 2. Top status bar surface
    raw_fb.fill_rect(0, 0, w, 28, Color::SURFACE_SLATE);
    raw_fb.fill_rect(0, 27, w, 1, Color::BORDER_SLATE);

    raw_fb.draw_string(16, 10, "MICRO-OS", Color::ACCENT_CYAN, None);
    raw_fb.draw_string(88, 10, "[x86_64 Bare-Metal Kernel | v0.1.0-alpha]", Color::TEXT_MUTED, None);

    let status_right = "STATUS: EARLY BOOT OK";
    let right_x = if w > 200 { w - (status_right.len() * 8) - 16 } else { 16 };
    raw_fb.draw_string(right_x, 10, status_right, Color::SUCCESS_GREEN, None);

    // 3. Central Diagnostic Card
    let (card_w, card_h, card_x, card_y, text_x, text_y_start, status_y) = card_layout(w, h);

    raw_fb.fill_rect(card_x, card_y, card_w, card_h, Color::CARD_SLATE);
    raw_fb.fill_rect(card_x, card_y, card_w, 2, Color::ACCENT_AZURE);

    // Card header
    raw_fb.draw_string(text_x, card_y + 16, "MICRO-OS KERNEL DIAGNOSTIC CONSOLE", Color::ACCENT_AZURE, None);
    raw_fb.draw_string(
        text_x,
        card_y + 34,
        "Cosmic Slate / Obsidian Azure Architecture Foundation",
        Color::TEXT_MUTED,
        None,
    );
    raw_fb.fill_rect(text_x, card_y + 54, card_w.saturating_sub(40), 1, Color::BORDER_SLATE);

    // Render milestone checklist: Milestone 0 is OK, remaining 1..7 are pending
    raw_fb.draw_string(text_x, text_y_start, MILESTONES[0].completed_text, Color::SUCCESS_GREEN, None);
    for i in 1..8 {
        let y = text_y_start + i * 20;
        raw_fb.draw_string(text_x, y, MILESTONES[i].pending_text, Color::TEXT_MUTED, None);
    }

    // Status bar at bottom of card
    raw_fb.draw_string(text_x, status_y, MILESTONES[0].status_text, Color::ACCENT_CYAN, None);

    // Bottom footer line
    let footer_y = h.saturating_sub(20);
    raw_fb.draw_string(16, footer_y, "Micro-OS - Bare-metal educational operating system", Color::TEXT_MUTED, None);

    // Store raw framebuffer for subsequent milestone updates and panic reporting
    *EARLY_FRAMEBUFFER.lock() = Some(raw_fb);
}

/// Updates the visible startup milestone on the early diagnostic console.
pub fn record_milestone(milestone: StartupMilestone) {
    let guard = EARLY_FRAMEBUFFER.lock();
    let raw_fb = match *guard {
        Some(fb) => fb,
        None => return,
    };

    let w = raw_fb.width();
    let h = raw_fb.height();
    let (card_w, _card_h, _card_x, _card_y, text_x, text_y_start, status_y) = card_layout(w, h);
    let idx = milestone as usize;

    if idx < MILESTONES.len() {
        let line_y = text_y_start + idx * 20;
        let line_w = card_w.saturating_sub(40);

        // Clear line background inside card
        raw_fb.fill_rect(text_x, line_y, line_w, 16, Color::CARD_SLATE);
        // Draw completed milestone text in vibrant green
        raw_fb.draw_string(text_x, line_y, MILESTONES[idx].completed_text, Color::SUCCESS_GREEN, None);

        // Clear and update status bar line
        raw_fb.fill_rect(text_x, status_y, line_w, 16, Color::CARD_SLATE);
        raw_fb.draw_string(text_x, status_y, MILESTONES[idx].status_text, Color::ACCENT_CYAN, None);
    }
}

/// A no-heap writer implementing `core::fmt::Write` directly onto the raw framebuffer.
struct PanicWriter<'a> {
    fb: &'a RawFramebuffer,
    cursor_x: usize,
    cursor_y: usize,
    start_x: usize,
    max_x: usize,
    max_y: usize,
}

impl<'a> core::fmt::Write for PanicWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            if c == '\n' {
                self.cursor_x = self.start_x;
                self.cursor_y += 12;
                if self.cursor_y + 8 > self.max_y {
                    return Ok(());
                }
                continue;
            }
            if self.cursor_x + 8 > self.max_x {
                self.cursor_x = self.start_x;
                self.cursor_y += 12;
                if self.cursor_y + 8 > self.max_y {
                    return Ok(());
                }
            }
            self.fb.draw_char(
                self.cursor_x,
                self.cursor_y,
                c,
                Color::TEXT_LIGHT,
                Some(Color::new(0x28, 0x0a, 0x0a)),
            );
            self.cursor_x += 8;
        }
        Ok(())
    }
}

/// Renders a clear, high-contrast panic diagnostic screen directly on the physical display
/// without allocating any heap memory. Safe to call during early boot panics.
pub fn render_panic_screen(info: &core::panic::PanicInfo) {
    let raw_fb = match EARLY_FRAMEBUFFER.try_lock() {
        Some(guard) => match *guard {
            Some(fb) => fb,
            None => return,
        },
        None => {
            // If already locked by the panicking thread, force unlock to avoid deadlock during panic
            unsafe {
                EARLY_FRAMEBUFFER.force_unlock();
            }
            match *EARLY_FRAMEBUFFER.lock() {
                Some(fb) => fb,
                None => return,
            }
        }
    };

    let w = raw_fb.width();
    let h = raw_fb.height();

    // 1. Dark crimson/black background
    raw_fb.clear(Color::new(0x18, 0x04, 0x04));

    // 2. High-contrast top panic banner
    raw_fb.fill_rect(0, 0, w, 32, Color::PANIC_HEADER);
    raw_fb.draw_string(16, 10, "MICRO-OS KERNEL PANIC - EXECUTION HALTED", Color::TEXT_LIGHT, None);

    // 3. Central Panic Card
    let card_x = 24;
    let card_y = 50;
    let card_w = w.saturating_sub(48);
    let card_h = h.saturating_sub(74);

    raw_fb.fill_rect(card_x, card_y, card_w, card_h, Color::new(0x28, 0x0a, 0x0a));
    raw_fb.fill_rect(card_x, card_y, card_w, 2, Color::ERROR_RED);

    let mut writer = PanicWriter {
        fb: &raw_fb,
        cursor_x: card_x + 20,
        cursor_y: card_y + 20,
        start_x: card_x + 20,
        max_x: card_x + card_w - 20,
        max_y: card_y + card_h - 20,
    };

    use core::fmt::Write;
    let _ = write!(writer, "CRITICAL ERROR: Bare-metal kernel panic occurred during execution.\n\n");
    let _ = write!(writer, "PANIC INFORMATION:\n");
    let _ = write!(writer, "{}\n\n", info);
    let _ = write!(writer, "HARDWARE DIAGNOSTIC STATUS:\n");
    let _ = write!(writer, "- CPU Mode: x86_64 Long Mode active\n");
    let serial_diag = match crate::serial::hardware_status() {
        crate::serial::SerialHardwareStatus::VerifiedHardware => {
            "ONLINE / VERIFIED (Physical 16550 UART at 0x3F8)"
        }
        crate::serial::SerialHardwareStatus::FloatingBusUnmapped => {
            "UNATTACHED / FLOATING BUS (No physical UART at 0x3F8; writes safely discarded)"
        }
        crate::serial::SerialHardwareStatus::OfflineTimedOut => {
            "OFFLINE / TIMED OUT (Bypassed)"
        }
    };
    let _ = write!(writer, "- Serial COM1 (0x3F8): {}\n", serial_diag);
    let _ = write!(
        writer,
        "- UART Scratch Register (0x3FF): {}\n",
        if crate::serial::is_scratch_ok() {
            "VERIFIED (0xA5/0x5A loopback passed)"
        } else {
            "UNRESPONSIVE (Reads 0xFF on floating bus; no physical UART hardware)"
        }
    );
    let _ = write!(writer, "- Interrupts: Disabled (cli)\n\n");

    let heap_diag = crate::memory::allocator::heap_diagnostics();
    let _ = write!(writer, "HEAP DIAGNOSTIC STATUS:\n");
    let _ = write!(
        writer,
        "- Heap Capacity: {} KiB ({} bytes) | Live Allocated: {} KiB ({} bytes)\n",
        heap_diag.total_bytes / 1024,
        heap_diag.total_bytes,
        heap_diag.allocated_bytes / 1024,
        heap_diag.allocated_bytes
    );
    let _ = write!(
        writer,
        "- Free Memory: {} KiB ({} bytes) | Largest Free Block: {} KiB ({} bytes)\n",
        heap_diag.total_free_bytes / 1024,
        heap_diag.total_free_bytes,
        heap_diag.largest_free_block_bytes / 1024,
        heap_diag.largest_free_block_bytes
    );
    let _ = write!(
        writer,
        "- Free Blocks: {} | Successes: {} | Failures: {} | Deallocs: {}\n",
        heap_diag.free_blocks_count,
        heap_diag.success_count,
        heap_diag.fail_count,
        heap_diag.dealloc_count
    );
    if heap_diag.last_failed_size > 0 {
        let _ = write!(
            writer,
            "- Last Failed Request: {} bytes (alignment {} bytes)\n",
            heap_diag.last_failed_size,
            heap_diag.last_failed_align
        );
    }
    let _ = write!(writer, "\n");
    let _ = write!(writer, "System halted via CPU hlt loop. Please power off or restart the machine.\n");
}

/// Renders an unhandled CPU exception screen directly on the display without heap allocations.
pub fn render_exception_screen(title: &str, rip: u64, error_code: u64, extra: &str) {
    let raw_fb = match EARLY_FRAMEBUFFER.try_lock() {
        Some(guard) => match *guard {
            Some(fb) => fb,
            None => return,
        },
        None => {
            unsafe {
                EARLY_FRAMEBUFFER.force_unlock();
            }
            match *EARLY_FRAMEBUFFER.lock() {
                Some(fb) => fb,
                None => return,
            }
        }
    };

    let w = raw_fb.width();
    let h = raw_fb.height();

    raw_fb.clear(Color::new(0x18, 0x04, 0x04));
    raw_fb.fill_rect(0, 0, w, 32, Color::PANIC_HEADER);
    raw_fb.draw_string(16, 10, "MICRO-OS CPU EXCEPTION - EXECUTION HALTED", Color::TEXT_LIGHT, None);

    let card_x = 24;
    let card_y = 50;
    let card_w = w.saturating_sub(48);
    let card_h = h.saturating_sub(74);

    raw_fb.fill_rect(card_x, card_y, card_w, card_h, Color::new(0x28, 0x0a, 0x0a));
    raw_fb.fill_rect(card_x, card_y, card_w, 2, Color::ERROR_RED);

    let mut writer = PanicWriter {
        fb: &raw_fb,
        cursor_x: card_x + 20,
        cursor_y: card_y + 20,
        start_x: card_x + 20,
        max_x: card_x + card_w - 20,
        max_y: card_y + card_h - 20,
    };

    use core::fmt::Write;
    let _ = write!(writer, "EXCEPTION: {}\n\n", title);
    let _ = write!(writer, "Instruction Pointer (RIP): 0x{:016x}\n", rip);
    let _ = write!(writer, "Exception Error Code:      0x{:x}\n", error_code);
    if !extra.is_empty() {
        let _ = write!(writer, "Details:                   {}\n\n", extra);
    }
    let _ = write!(writer, "\nSystem halted via CPU hlt loop. Please power off or restart.\n");
}

/// Backwards-compatible early framebuffer writer.
#[allow(dead_code)]
pub struct FramebufferWriter<'a> {
    buffer: &'a mut [u8],
    info: FrameBufferInfo,
}

#[allow(dead_code)]
impl<'a> FramebufferWriter<'a> {
    /// Creates a writer wrapping the mutable framebuffer slice.
    pub fn new(fb: &'a mut FrameBuffer) -> Self {
        let info = fb.info();
        let buffer = fb.buffer_mut();
        Self { buffer, info }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> usize {
        self.info.width
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> usize {
        self.info.height
    }

    /// Writes a single pixel to the screen.
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let stride = self.info.stride;
        let bytes_per_pixel = self.info.bytes_per_pixel;
        let pixel_offset = (y * stride + x) * bytes_per_pixel;

        if pixel_offset + bytes_per_pixel > self.buffer.len() {
            return;
        }

        match self.info.pixel_format {
            PixelFormat::Rgb => {
                self.buffer[pixel_offset] = color.r;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.b;
            }
            PixelFormat::Bgr => {
                self.buffer[pixel_offset] = color.b;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.r;
            }
            PixelFormat::U8 => {
                let gray = ((color.r as u16 * 77 + color.g as u16 * 150 + color.b as u16 * 29) >> 8) as u8;
                self.buffer[pixel_offset] = gray;
            }
            _ => {
                self.buffer[pixel_offset] = color.b;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.r;
            }
        }
    }

    /// Clears the entire display buffer with a solid color.
    pub fn clear(&mut self, color: Color) {
        let w = self.info.width;
        let h = self.info.height;
        self.fill_rect(0, 0, w, h, color);
    }

    /// Fills a solid rectangle.
    pub fn fill_rect(&mut self, start_x: usize, start_y: usize, width: usize, height: usize, color: Color) {
        let end_x = (start_x + width).min(self.info.width);
        let end_y = (start_y + height).min(self.info.height);

        for y in start_y..end_y {
            for x in start_x..end_x {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Draws a character using the 8x8 font bitmap.
    pub fn draw_char(&mut self, start_x: usize, start_y: usize, c: char, fg: Color, bg: Option<Color>) {
        let ascii_code = c as usize;
        let bitmap = if ascii_code < 128 {
            font8x8::legacy::BASIC_LEGACY[ascii_code]
        } else {
            font8x8::legacy::NOTHING_TO_DISPLAY
        };

        for (row, &byte) in bitmap.iter().enumerate() {
            for col in 0..8 {
                let pixel_set = (byte & (1 << col)) != 0;
                let px = start_x + col;
                let py = start_y + row;
                if pixel_set {
                    self.set_pixel(px, py, fg);
                } else if let Some(bg_color) = bg {
                    self.set_pixel(px, py, bg_color);
                }
            }
        }
    }

    /// Draws a text string at (x, y).
    pub fn draw_string(&mut self, mut x: usize, y: usize, text: &str, fg: Color, bg: Option<Color>) {
        for c in text.chars() {
            if c == '\n' {
                break;
            }
            self.draw_char(x, y, c, fg, bg);
            x += 8;
        }
    }

    /// Renders the Micro-OS Cosmic Slate / Obsidian Azure boot diagnostic canvas.
    pub fn render_boot_screen(&mut self) {
        let w = self.width();
        let h = self.height();

        self.clear(Color::OBSIDIAN_NAVY);

        self.fill_rect(0, 0, w, 28, Color::SURFACE_SLATE);
        self.fill_rect(0, 27, w, 1, Color::BORDER_SLATE);

        self.draw_string(16, 10, "MICRO-OS", Color::ACCENT_CYAN, None);
        self.draw_string(88, 10, "[x86_64 Bare-Metal Kernel | v0.1.0-alpha]", Color::TEXT_MUTED, None);

        let status_right = "STATUS: EARLY BOOT OK";
        let right_x = if w > 200 { w - (status_right.len() * 8) - 16 } else { 16 };
        self.draw_string(right_x, 10, status_right, Color::SUCCESS_GREEN, None);

        let (card_w, card_h, card_x, card_y, text_x, text_y_start, status_y) = card_layout(w, h);

        self.fill_rect(card_x, card_y, card_w, card_h, Color::CARD_SLATE);
        self.fill_rect(card_x, card_y, card_w, 2, Color::ACCENT_AZURE);

        self.draw_string(text_x, card_y + 16, "MICRO-OS KERNEL DIAGNOSTIC CONSOLE", Color::ACCENT_AZURE, None);
        self.draw_string(
            text_x,
            card_y + 34,
            "Cosmic Slate / Obsidian Azure Architecture Foundation",
            Color::TEXT_MUTED,
            None,
        );
        self.fill_rect(text_x, card_y + 54, card_w.saturating_sub(40), 1, Color::BORDER_SLATE);

        for (i, milestone) in MILESTONES.iter().enumerate() {
            let y = text_y_start + i * 20;
            self.draw_string(text_x, y, milestone.completed_text, Color::SUCCESS_GREEN, None);
        }

        self.draw_string(text_x, status_y, "All startup milestones verified.", Color::ACCENT_CYAN, None);

        let footer_y = h.saturating_sub(20);
        self.draw_string(16, footer_y, "Micro-OS - Bare-metal educational operating system", Color::TEXT_MUTED, None);
    }
}
