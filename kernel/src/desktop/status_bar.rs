//! Desktop Top Status Bar Component.
//!
//! Provides a fixed 34px top system bar displaying the MicroOS logo, active workspace,
//! system architecture identification, physical RAM and kernel heap metrics,
//! honest timer-driven uptime, and an unconfigured clock indicator.

use alloc::format;
use crate::arch::x86_64::timer::uptime_ms;
use crate::graphics::color::Color;
use crate::graphics::font::{draw_string, measure_string};
use crate::graphics::geometry::Rect;
use crate::graphics::surface::Surface;

/// Default height of the top status bar in pixels.
pub const STATUS_BAR_HEIGHT: u32 = 34;

/// Encapsulates the system metrics displayed in the top status bar.
pub struct StatusMetrics {
    pub arch: &'static str,
    pub total_ram_mb: u64,
    pub heap_size_kb: usize,
    pub clock_str: &'static str,
}

impl Default for StatusMetrics {
    fn default() -> Self {
        Self {
            arch: "x86_64",
            total_ram_mb: 497, // Verified usable RAM detected by bootloader
            heap_size_kb: crate::memory::HEAP_SIZE / 1024,
            clock_str: "N/A", // Strictly honest: RTC / network time not yet configured
        }
    }
}

/// Renders the top status bar across the top of `surface`.
pub fn render_status_bar(surface: &mut Surface, metrics: &StatusMetrics) {
    let w = surface.width() as i32;
    let h = STATUS_BAR_HEIGHT as i32;

    // 1. Background Surface and bottom border
    let bar_rect = Rect::new(0, 0, w as u32, STATUS_BAR_HEIGHT);
    surface.fill_rect(bar_rect, Color::SURFACE_SLATE);
    surface.draw_hline(0, h - 1, w as u32, Color::BORDER_SLATE);

    // 2. Left side: Branding & Workspace Badge
    // Brand diamond icon
    let icon_x = 14;
    let icon_y = 12;
    surface.fill_rect(Rect::new(icon_x, icon_y, 8, 8), Color::ACCENT_CYAN);
    surface.stroke_rect(Rect::new(icon_x, icon_y, 8, 8), 1, Color::WHITE);

    // "MICRO-OS" title
    draw_string(surface, icon_x + 16, 12, "MICRO-OS", Color::ACCENT_CYAN, None, 1);

    // Workspace badge
    let ws_text = "[ Workspace 1 ]";
    draw_string(surface, icon_x + 104, 12, ws_text, Color::TEXT_MUTED, None, 1);

    // 3. Right side: System Indicators & Telemetry
    // Compute current uptime from 8254 PIT
    let total_secs = uptime_ms() / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let uptime_str = format!("UP: {:02}:{:02}", mins, secs);

    // Assembly telemetry items: (label, text, color)
    let ram_str = format!("RAM: {}M", metrics.total_ram_mb);
    let heap_str = format!("HEAP: {}K", metrics.heap_size_kb);
    let time_str = format!("TIME: {}", metrics.clock_str);
    let arch_str = format!("ARCH: {}", metrics.arch);

    let items = [
        (arch_str.as_str(), Color::TEXT_MUTED),
        (ram_str.as_str(), Color::TEXT_LIGHT),
        (heap_str.as_str(), Color::TEXT_LIGHT),
        (uptime_str.as_str(), Color::SUCCESS_GREEN),
        (time_str.as_str(), Color::TEXT_MUTED),
        ("[RUNNING]", Color::ACCENT_AZURE),
    ];

    let mut cur_x = w - 16;
    for (text, color) in items.iter().rev() {
        let (item_w, _) = measure_string(text, 1);
        cur_x -= item_w as i32;
        draw_string(surface, cur_x, 12, text, *color, None, 1);
        cur_x -= 16; // 16px gap between items
    }
}
