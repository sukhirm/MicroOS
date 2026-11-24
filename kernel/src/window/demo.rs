//! Demonstration Desktop Windows.
//!
//! Provides sample visual content for the Phase 4C desktop showcase:
//! 1. Welcome Window (Micro-OS branding & architecture specs)
//! 2. System Monitor Window (RAM, heap, double-buffer, and timer metrics)
//! 3. Settings Window (display resolution, theme options, and category cards)

use super::manager::WindowManager;
use super::state::WindowId;
use super::window::Window;
use crate::graphics::color::Color;
use crate::graphics::font::draw_string;
use crate::graphics::geometry::Rect;
use crate::graphics::icon::{ICON_ACTIVITY, ICON_GEAR, ICON_TERMINAL};
use crate::graphics::surface::Surface;

/// Creates and populates the demonstration desktop with the three overlapping sample windows.
pub fn create_demo_desktop(screen_w: u32, screen_h: u32) -> WindowManager {
    let mut wm = WindowManager::new(screen_w, screen_h);

    // 1. Welcome to Micro-OS Window (Back-most)
    let win_welcome = Window::new(
        WindowId::new(1),
        "Welcome to Micro-OS",
        100,
        70,
        380,
        250,
    )
    .with_icon(&ICON_TERMINAL)
    .with_content_renderer(render_welcome_content);
    wm.add_window(win_welcome);

    // 2. System Monitor Window (Middle)
    let win_sysmon = Window::new(
        WindowId::new(2),
        "System Monitor",
        280,
        140,
        520,
        350,
    )
    .with_icon(&ICON_ACTIVITY)
    .with_content_renderer(crate::apps::sysmon::render_sysmon_content);
    wm.add_window(win_sysmon);

    // 3. Settings Window (Front-most & Active)
    let win_settings = Window::new(
        WindowId::new(3),
        "Settings",
        480,
        220,
        500,
        340,
    )
    .with_icon(&ICON_GEAR)
    .with_content_renderer(crate::apps::settings::render_settings_content);
    wm.add_window(win_settings);

    // Activate the front-most Settings window
    wm.set_active(WindowId::new(3));

    wm
}

/// Content renderer for the Welcome Window.
fn render_welcome_content(_window: &Window, surface: &mut Surface, rect: Rect) {
    let padding = 14i32;
    let mut cur_y = rect.y + padding;
    let text_x = rect.x + padding;

    // Header wordmark
    draw_string(surface, text_x, cur_y, "Micro-OS v0.11", Color::ACCENT_CYAN, None, 1);
    cur_y += 14;
    draw_string(
        surface,
        text_x,
        cur_y,
        "Obsidian Azure Desktop",
        Color::ACCENT_AZURE,
        None,
        1,
    );
    cur_y += 18;

    // Subtle horizontal divider
    surface.draw_hline(
        text_x,
        cur_y,
        rect.width.saturating_sub((padding * 2) as u32),
        Color::BORDER_SLATE,
    );
    cur_y += 12;

    // Architectural specifications
    let lines = [
        ("Kernel:", "x86_64 Bare-Metal (Rust)"),
        ("Display:", "1280x800 GOP Framebuffer"),
        ("Engine:", "Double-Buffered 60Hz"),
        ("Manager:", "Z-Stacking & Rect Clipping"),
        ("Preemption:", "8254 PIT (100 Hz Mode 3)"),
    ];

    for (label, val) in lines {
        draw_string(surface, text_x, cur_y, label, Color::TEXT_MUTED, None, 1);
        draw_string(surface, text_x + 90, cur_y, val, Color::WHITE, None, 1);
        cur_y += 16;
    }

    cur_y += 8;
    // Readiness badge card
    let badge_rect = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 24);
    surface.fill_rounded_rect(badge_rect, 6, Color::new(0x1a, 0x24, 0x3b));
    surface.stroke_rounded_rect(badge_rect, 6, 1, Color::BORDER_SLATE);
    draw_string(
        surface,
        text_x + 12,
        cur_y + 8,
        "[ READY FOR PHASE 4D INPUT ]",
        Color::ACCENT_CYAN,
        None,
        1,
    );
}

/// Content renderer for the System Monitor Window (legacy fallback).
#[allow(dead_code)]
fn render_sysmon_content(_window: &Window, surface: &mut Surface, rect: Rect) {
    let padding = 14i32;
    let mut cur_y = rect.y + padding;
    let text_x = rect.x + padding;

    // Header
    draw_string(surface, text_x, cur_y, "Hardware Telemetry", Color::WHITE, None, 1);
    cur_y += 16;

    // Divider
    surface.draw_hline(
        text_x,
        cur_y,
        rect.width.saturating_sub((padding * 2) as u32),
        Color::BORDER_SLATE,
    );
    cur_y += 12;

    let metrics = [
        ("Physical RAM:", "497 MiB usable"),
        ("Kernel Heap:", "16 MiB (Dynamic)"),
        ("Backbuffer:", "16384 KiB (Direct VRAM)"),
        ("Timer Tick:", "10 ms (100 Hz IRQ0)"),
        ("Execution:", "Ring 0 (Supervisor)"),
        ("CPU Registers:", "CR0/CR3/CR4 Validated"),
    ];

    for (label, val) in metrics {
        draw_string(surface, text_x, cur_y, label, Color::TEXT_MUTED, None, 1);
        draw_string(surface, text_x + 120, cur_y, val, Color::WHITE, None, 1);
        cur_y += 16;
    }

    cur_y += 8;
    // Visual RAM consumption bar
    let bar_rect = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 16);
    surface.fill_rounded_rect(bar_rect, 4, Color::new(0x11, 0x17, 0x24));
    surface.stroke_rounded_rect(bar_rect, 4, 1, Color::BORDER_SLATE);

    // 15% simulated heap/kernel utilization fill
    let fill_w = (bar_rect.width as f32 * 0.15) as u32;
    let fill_rect = Rect::new(bar_rect.x + 1, bar_rect.y + 1, fill_w, bar_rect.height.saturating_sub(2));
    surface.fill_rounded_rect(fill_rect, 3, Color::ACCENT_CYAN);
}

/// Content renderer for the Settings Window (legacy demo fallback).
#[allow(dead_code)]
fn render_settings_content(_window: &Window, surface: &mut Surface, rect: Rect) {
    let padding = 14i32;
    let mut cur_y = rect.y + padding;
    let text_x = rect.x + padding;

    // Header
    draw_string(surface, text_x, cur_y, "Desktop Preferences", Color::WHITE, None, 1);
    cur_y += 16;

    // Divider
    surface.draw_hline(
        text_x,
        cur_y,
        rect.width.saturating_sub((padding * 2) as u32),
        Color::BORDER_SLATE,
    );
    cur_y += 12;

    let items = [
        ("[ Display ]", "1280x800 @ 32bpp BGR"),
        ("[ Theme   ]", "Cosmic Slate Dark"),
        ("[ Accent  ]", "Obsidian Azure / Cyan"),
        ("[ Stacking]", "Z-Order Occlusion Active"),
        ("[ Input   ]", "PS/2 Driver Ready (P4D)"),
        ("[ Network ]", "Ethernet Stack (Planned)"),
    ];

    for (tag, desc) in items {
        draw_string(surface, text_x, cur_y, tag, Color::ACCENT_CYAN, None, 1);
        draw_string(surface, text_x + 105, cur_y, desc, Color::WHITE, None, 1);
        cur_y += 16;
    }

    cur_y += 10;
    // Active state indicator
    let status_rect = Rect::new(text_x, cur_y, rect.width.saturating_sub((padding * 2) as u32), 22);
    surface.fill_rounded_rect(status_rect, 6, Color::new(0x1a, 0x24, 0x3b));
    surface.stroke_rounded_rect(status_rect, 6, 1, Color::ACCENT_CYAN);
    draw_string(
        surface,
        text_x + 14,
        cur_y + 7,
        "* ACTIVE FOCUSED WINDOW *",
        Color::ACCENT_CYAN,
        None,
        1,
    );
}
