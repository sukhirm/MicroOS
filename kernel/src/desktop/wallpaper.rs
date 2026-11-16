//! Desktop Wallpaper Renderer.
//!
//! Generates the official Cosmic Slate / Obsidian Azure desktop background,
//! featuring a deep navy cosmic canvas, subtle geometric grid lines,
//! and clean cyan/azure architectural accents.

use crate::graphics::color::Color;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;

/// Renders the complete Cosmic Slate wallpaper across the provided surface.
pub fn render_wallpaper(surface: &mut Surface) {
    let w = surface.width() as i32;
    let h = surface.height() as i32;

    // 1. Base deep cosmic navy background
    surface.clear(Color::OBSIDIAN_NAVY);

    // 2. Subtle architectural grid lines (64px pitch)
    let grid_color = Color::rgba(0x13, 0x1b, 0x2e, 90);
    for x in (0..w).step_by(64) {
        surface.draw_vline(x, 0, h as u32, grid_color);
    }
    for y in (0..h).step_by(64) {
        surface.draw_hline(0, y, w as u32, grid_color);
    }

    // 3. Ambient Geometric Accent Elements
    // Top-right corner geometric diagonal bands
    let tr_band_color = Color::rgba(0x2d, 0x3c, 0x5a, 60);
    for offset in (0..180).step_by(30) {
        surface.draw_line(
            Point::new(w - 240 + offset, 0),
            Point::new(w, 240 - offset),
            tr_band_color,
        );
    }

    // Bottom-left subtle angled geometric line
    let bl_band_color = Color::rgba(0x38, 0xbd, 0xf8, 40);
    surface.draw_line(
        Point::new(0, h - 160),
        Point::new(200, h),
        bl_band_color,
    );

    // 4. Central Subtle Celestial Motif (Concentric Slate Rings & Azure Spark)
    let center_x = w / 2;
    let center_y = h / 2 - 20;

    let ring_color1 = Color::rgba(0x1a, 0x24, 0x3b, 80);
    let ring_color2 = Color::rgba(0x2d, 0x3c, 0x5a, 50);

    // Subtle outer rounded rectangle framing
    let frame_w = 480u32.min(w as u32 - 80);
    let frame_h = 240u32.min(h as u32 - 120);
    let frame_rect = Rect::new(center_x - (frame_w as i32 / 2), center_y - (frame_h as i32 / 2), frame_w, frame_h);
    surface.stroke_rounded_rect(frame_rect, 24, 1, ring_color1);

    // Inner subtle emblem ring
    let inner_frame = frame_rect.inset(20);
    surface.stroke_rounded_rect(inner_frame, 16, 1, ring_color2);

    // Accent constellation crosshairs in center
    let cross_len = 16;
    let cross_color = Color::rgba(0x00, 0xd2, 0xff, 70);
    surface.draw_hline(center_x - cross_len, center_y, (cross_len * 2) as u32, cross_color);
    surface.draw_vline(center_x, center_y - cross_len, (cross_len * 2) as u32, cross_color);
    surface.set_pixel(center_x, center_y, Color::ACCENT_CYAN);
}
