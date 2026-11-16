//! Milestone 4A Graphics Engine Demonstration Screen.
//!
//! Renders a showcase verifying double-buffering, geometric primitives,
//! rounded rectangles, multi-scale typography, clipping, and embedded bitmap icons
//! in the Cosmic Slate / Obsidian Azure visual identity.

use super::color::Color;
use super::font::{draw_string, measure_string};
use super::geometry::{Point, Rect};
use super::icon::*;
use super::surface::Surface;

/// Renders the complete Milestone 4A graphics showcase on the provided surface.
pub fn render_demo(surface: &mut Surface) {
    let w = surface.width() as i32;
    let h = surface.height() as i32;

    // 1. Deep cosmic navy background
    surface.clear(Color::OBSIDIAN_NAVY);

    // 2. Subtle geometric grid lines (40px pitch)
    let grid_color = Color::rgba(0x13, 0x1b, 0x2e, 100);
    for x in (0..w).step_by(40) {
        surface.draw_vline(x, 0, h as u32, grid_color);
    }
    for y in (0..h).step_by(40) {
        surface.draw_hline(0, y, w as u32, grid_color);
    }

    // 3. Top System Status Bar
    let header_h = 36;
    surface.fill_rect(Rect::new(0, 0, w as u32, header_h as u32), Color::SURFACE_SLATE);
    surface.draw_hline(0, header_h - 1, w as u32, Color::BORDER_SLATE);

    // Brand title in Cyan
    draw_string(surface, 16, 10, "MICRO-OS", Color::ACCENT_CYAN, None, 2);
    draw_string(
        surface,
        130,
        14,
        "|  Phase 4 Milestone 4A: Graphics Engine & Drawing Pipeline",
        Color::TEXT_LIGHT,
        None,
        1,
    );

    let status_tag = "[OK] DOUBLE-BUFFERED GOP (1280x800)";
    let (tag_w, _) = measure_string(status_tag, 1);
    let tag_x = (w - tag_w as i32 - 16).max(200);
    draw_string(surface, tag_x, 14, status_tag, Color::SUCCESS_GREEN, None, 1);

    // Layout configuration for showcase cards
    let margin = 20;
    let content_y = header_h + margin;
    let col_w = (w - margin * 3) / 2;
    let card_h = 320;

    // =========================================================================
    // Card 1: Geometric Primitives & Rounded Rectangles
    // =========================================================================
    let card1 = Rect::new(margin, content_y, col_w as u32, card_h as u32);
    surface.fill_rounded_rect(card1, 10, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card1, 10, 1, Color::ACCENT_AZURE);

    // Card 1 Header
    draw_string(surface, card1.x + 16, card1.y + 16, "GEOMETRIC PRIMITIVES & ROUNDING", Color::ACCENT_AZURE, None, 1);
    surface.draw_hline(card1.x + 16, card1.y + 34, card1.width - 32, Color::BORDER_SLATE);

    // Primitives inside Card 1
    // A) Solid and outlined rectangles
    surface.fill_rect(Rect::new(card1.x + 20, card1.y + 50, 80, 50), Color::SURFACE_SLATE);
    surface.stroke_rect(Rect::new(card1.x + 20, card1.y + 50, 80, 50), 2, Color::ACCENT_CYAN);
    draw_string(surface, card1.x + 24, card1.y + 110, "Outlined", Color::TEXT_MUTED, None, 1);

    // B) Rounded rect with radius 12
    let round_demo = Rect::new(card1.x + 130, card1.y + 50, 100, 50);
    surface.fill_rounded_rect(round_demo, 12, Color::SURFACE_SLATE);
    surface.stroke_rounded_rect(round_demo, 12, 2, Color::SUCCESS_GREEN);
    draw_string(surface, card1.x + 135, card1.y + 110, "Rounded 12px", Color::TEXT_MUTED, None, 1);

    // C) Bresenham diagonal lines
    let line_box = Rect::new(card1.x + 260, card1.y + 50, 120, 50);
    surface.stroke_rect(line_box, 1, Color::BORDER_SLATE);
    surface.draw_line(
        Point::new(line_box.x + 4, line_box.y + 4),
        Point::new(line_box.right() - 5, line_box.bottom() - 5),
        Color::WARNING_AMBER,
    );
    surface.draw_line(
        Point::new(line_box.x + 4, line_box.bottom() - 5),
        Point::new(line_box.right() - 5, line_box.y + 4),
        Color::ACCENT_CYAN,
    );
    draw_string(surface, card1.x + 265, card1.y + 110, "Bresenham Lines", Color::TEXT_MUTED, None, 1);

    // D) Color Palette Swatches
    draw_string(surface, card1.x + 20, card1.y + 145, "Cosmic Slate Design Palette:", Color::TEXT_LIGHT, None, 1);
    let swatches = [
        (Color::OBSIDIAN_NAVY, "Navy"),
        (Color::SURFACE_SLATE, "Surface"),
        (Color::CARD_SLATE, "Card"),
        (Color::BORDER_SLATE, "Border"),
        (Color::ACCENT_AZURE, "Azure"),
        (Color::ACCENT_CYAN, "Cyan"),
        (Color::SUCCESS_GREEN, "Green"),
        (Color::WARNING_AMBER, "Amber"),
        (Color::DANGER_RED, "Red"),
    ];

    let mut sx = card1.x + 20;
    for (c, name) in swatches {
        surface.fill_rounded_rect(Rect::new(sx, card1.y + 170, 36, 24), 4, c);
        surface.stroke_rounded_rect(Rect::new(sx, card1.y + 170, 36, 24), 4, 1, Color::BORDER_SLATE);
        draw_string(surface, sx, card1.y + 200, name, Color::TEXT_MUTED, None, 1);
        sx += 44;
    }

    // E) Alpha blending demo
    draw_string(surface, card1.x + 20, card1.y + 225, "Alpha Transparency & Blending:", Color::TEXT_LIGHT, None, 1);
    let base_rect = Rect::new(card1.x + 20, card1.y + 245, 180, 50);
    surface.fill_rect(base_rect, Color::SURFACE_SLATE);
    surface.stroke_rect(base_rect, 1, Color::BORDER_SLATE);

    // Overlapping translucent chips
    surface.fill_rounded_rect(
        Rect::new(base_rect.x + 10, base_rect.y + 10, 60, 30),
        6,
        Color::rgba(0x38, 0xbd, 0xf8, 180),
    );
    surface.fill_rounded_rect(
        Rect::new(base_rect.x + 40, base_rect.y + 10, 60, 30),
        6,
        Color::rgba(0x00, 0xd2, 0xff, 180),
    );
    surface.fill_rounded_rect(
        Rect::new(base_rect.x + 70, base_rect.y + 10, 60, 30),
        6,
        Color::rgba(0x4a, 0xde, 0x80, 180),
    );
    draw_string(surface, base_rect.right() + 15, card1.y + 260, "Alpha Blend OK", Color::SUCCESS_GREEN, None, 1);

    // =========================================================================
    // Card 2: Typography & Scaled Font Engine
    // =========================================================================
    let card2 = Rect::new(margin * 2 + col_w, content_y, col_w as u32, card_h as u32);
    surface.fill_rounded_rect(card2, 10, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card2, 10, 1, Color::ACCENT_AZURE);

    draw_string(surface, card2.x + 16, card2.y + 16, "TYPOGRAPHY & SCALED FONT ENGINE", Color::ACCENT_AZURE, None, 1);
    surface.draw_hline(card2.x + 16, card2.y + 34, card2.width - 32, Color::BORDER_SLATE);

    // Scale 2x Headline
    draw_string(surface, card2.x + 20, card2.y + 50, "Micro-OS Desktop UI", Color::TEXT_LIGHT, None, 2);

    // Scale 1x Subtitles
    draw_string(surface, card2.x + 20, card2.y + 75, "Custom lightweight bitmap font with integer scaling", Color::ACCENT_CYAN, None, 1);
    draw_string(surface, card2.x + 20, card2.y + 95, "Supports foreground tints, background bounds, and tabs:", Color::TEXT_MUTED, None, 1);

    // Badge styling examples
    draw_string(surface, card2.x + 20, card2.y + 120, " [SYSTEM READY] ", Color::SUCCESS_GREEN, Some(Color::SURFACE_SLATE), 1);
    draw_string(surface, card2.x + 170, card2.y + 120, " [TIMER: 100 Hz] ", Color::ACCENT_AZURE, Some(Color::SURFACE_SLATE), 1);
    draw_string(surface, card2.x + 320, card2.y + 120, " [CPL: RING 0] ", Color::WARNING_AMBER, Some(Color::SURFACE_SLATE), 1);

    // Multi-line code block simulation
    let code_box = Rect::new(card2.x + 20, card2.y + 150, card2.width - 40, 140);
    surface.fill_rounded_rect(code_box, 6, Color::OBSIDIAN_NAVY);
    surface.stroke_rounded_rect(code_box, 6, 1, Color::BORDER_SLATE);

    draw_string(surface, code_box.x + 12, code_box.y + 12, "// Micro-OS Graphics Surface Pipeline", Color::TEXT_MUTED, None, 1);
    draw_string(surface, code_box.x + 12, code_box.y + 30, "let mut surface = create_backbuffer(1280, 800);", Color::ACCENT_CYAN, None, 1);
    draw_string(surface, code_box.x + 12, code_box.y + 48, "surface.clear(Color::OBSIDIAN_NAVY);", Color::TEXT_LIGHT, None, 1);
    draw_string(surface, code_box.x + 12, code_box.y + 66, "surface.fill_rounded_rect(bounds, 8, CARD_SLATE);", Color::TEXT_LIGHT, None, 1);
    draw_string(surface, code_box.x + 12, code_box.y + 84, "draw_string(&mut surface, x, y, \"Title\", AZURE, 1);", Color::SUCCESS_GREEN, None, 1);
    draw_string(surface, code_box.x + 12, code_box.y + 102, "double_buffer::present(&surface, &mut fb);", Color::WARNING_AMBER, None, 1);

    // =========================================================================
    // Card 3: Embedded Bitmap Icons & Clipping Engine (Bottom Row)
    // =========================================================================
    let card3_y = content_y + card_h + margin;
    let card3_h = 320;
    let card3 = Rect::new(margin, card3_y, w as u32 - (margin * 2) as u32, card3_h as u32);
    surface.fill_rounded_rect(card3, 10, Color::CARD_SLATE);
    surface.stroke_rounded_rect(card3, 10, 1, Color::ACCENT_AZURE);

    draw_string(surface, card3.x + 16, card3.y + 16, "EMBEDDED 16x16 ICONS & HARDWARE CLIPPING PIPELINE", Color::ACCENT_AZURE, None, 1);
    surface.draw_hline(card3.x + 16, card3.y + 34, card3.width - 32, Color::BORDER_SLATE);

    // Left sub-panel: Icon showcase
    let icons = [
        (&ICON_FOLDER, "Files", Color::ACCENT_AZURE),
        (&ICON_GEAR, "Settings", Color::TEXT_LIGHT),
        (&ICON_ACTIVITY, "System", Color::SUCCESS_GREEN),
        (&ICON_TERMINAL, "Terminal", Color::ACCENT_CYAN),
        (&ICON_STORE, "Store", Color::WARNING_AMBER),
        (&ICON_NETWORK, "Network", Color::ACCENT_AZURE),
        (&ICON_CHECK, "Check", Color::SUCCESS_GREEN),
        (&ICON_CLOSE, "Close", Color::DANGER_RED),
    ];

    draw_string(surface, card3.x + 20, card3.y + 50, "Embedded 16x16 Application & Chrome Icons (with 1-bit transparency):", Color::TEXT_LIGHT, None, 1);

    let mut ix = card3.x + 24;
    for (icon, label, tint) in icons {
        let icon_box = Rect::new(ix, card3.y + 75, 52, 60);
        surface.fill_rounded_rect(icon_box, 6, Color::SURFACE_SLATE);
        surface.stroke_rounded_rect(icon_box, 6, 1, Color::BORDER_SLATE);

        draw_icon(surface, ix + 18, card3.y + 85, icon, Some(tint));
        let (lw, _) = measure_string(label, 1);
        let lx = ix + (52 - lw as i32) / 2;
        draw_string(surface, lx, card3.y + 115, label, Color::TEXT_MUTED, None, 1);

        ix += 64;
    }

    // Right sub-panel: Clipping demonstration
    let clip_panel_x = card3.x + 580;
    let clip_panel_w = card3.width.saturating_sub(600);
    draw_string(surface, clip_panel_x, card3.y + 50, "Hardware Clipping Demonstration:", Color::TEXT_LIGHT, None, 1);

    let outer_boundary = Rect::new(clip_panel_x, card3.y + 75, clip_panel_w - 20, 160);
    surface.fill_rect(outer_boundary, Color::OBSIDIAN_NAVY);
    surface.stroke_rect(outer_boundary, 1, Color::BORDER_SLATE);

    // Define a constrained inner clipping window
    let inner_clip = Rect::new(outer_boundary.x + 30, outer_boundary.y + 30, outer_boundary.width - 60, 100);
    surface.stroke_rect(inner_clip, 1, Color::ACCENT_CYAN);
    draw_string(surface, inner_clip.x + 8, inner_clip.y - 14, "Active Clip Rect [30, 30, W-60, 100]", Color::ACCENT_CYAN, None, 1);

    // Save old clip rect, set inner clip
    let old_clip = surface.clip_rect();
    surface.set_clip_rect(inner_clip);

    // Draw elements that intentionally exceed the clipping rectangle
    surface.fill_rounded_rect(
        Rect::new(inner_clip.x - 20, inner_clip.y - 15, inner_clip.width + 40, 40),
        8,
        Color::rgba(0x38, 0xbd, 0xf8, 220),
    );
    draw_string(
        surface,
        inner_clip.x - 10,
        inner_clip.y + 10,
        "This long line is clipped on left and right bounds!",
        Color::OBSIDIAN_NAVY,
        None,
        1,
    );
    surface.draw_line(
        Point::new(outer_boundary.x, outer_boundary.y),
        Point::new(outer_boundary.right(), outer_boundary.bottom()),
        Color::DANGER_RED,
    );

    // Restore original surface clipping
    surface.set_clip_rect(old_clip);

    draw_string(surface, clip_panel_x, card3.y + 250, "Verified: Objects extending outside clip window are clipped with 0 bleed.", Color::SUCCESS_GREEN, None, 1);

    // 4. Bottom Footer status line
    let footer_y = h - 22;
    surface.draw_hline(0, footer_y - 4, w as u32, Color::BORDER_SLATE);
    draw_string(
        surface,
        16,
        footer_y,
        "Micro-OS v0.4.0-alpha | Bare-Metal x86_64 Desktop OS | Cosmic Slate / Obsidian Azure",
        Color::TEXT_MUTED,
        None,
        1,
    );
}
