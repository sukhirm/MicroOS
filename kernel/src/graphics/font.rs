//! Scaled Text and Bitmap Font Rendering Engine.
//!
//! Provides multi-scale typography based on the standard 8x8 bitmap font,
//! with transparent backgrounds, newline formatting, and character measurement.

use super::color::Color;
use super::geometry::{Point, Rect};
use super::surface::Surface;

/// Draws a single character glyph on a surface with custom integer scaling.
pub fn draw_char(
    surface: &mut Surface,
    start_x: i32,
    start_y: i32,
    c: char,
    fg: Color,
    bg: Option<Color>,
    scale: u32,
) -> u32 {
    let s = scale.max(1) as i32;
    let ascii_code = c as usize;

    let bitmap = if ascii_code < 128 {
        font8x8::legacy::BASIC_LEGACY[ascii_code]
    } else {
        font8x8::legacy::BASIC_LEGACY[b'?' as usize]
    };

    // If background color is specified, draw the full character bounding box
    if let Some(bg_color) = bg {
        surface.fill_rect(Rect::new(start_x, start_y, (8 * s) as u32, (8 * s) as u32), bg_color);
    }

    for (row, &byte) in bitmap.iter().enumerate() {
        for col in 0..8 {
            let pixel_set = (byte & (1 << col)) != 0;
            if pixel_set {
                let px = start_x + (col as i32) * s;
                let py = start_y + (row as i32) * s;
                if s == 1 {
                    surface.set_pixel(px, py, fg);
                } else {
                    surface.fill_rect(Rect::new(px, py, s as u32, s as u32), fg);
                }
            }
        }
    }

    (8 * s) as u32
}

/// Draws a text string on a surface, supporting newlines and integer scaling.
pub fn draw_string(
    surface: &mut Surface,
    start_x: i32,
    start_y: i32,
    text: &str,
    fg: Color,
    bg: Option<Color>,
    scale: u32,
) -> Point {
    let s = scale.max(1) as i32;
    let mut cur_x = start_x;
    let mut cur_y = start_y;
    let line_height = (8 + 2) * s;

    for c in text.chars() {
        match c {
            '\n' => {
                cur_x = start_x;
                cur_y += line_height;
            }
            '\r' => {
                cur_x = start_x;
            }
            '\t' => {
                cur_x += 4 * 8 * s;
            }
            _ => {
                let char_width = draw_char(surface, cur_x, cur_y, c, fg, bg, scale);
                cur_x += char_width as i32;
            }
        }
    }

    Point::new(cur_x, cur_y)
}

/// Computes the pixel dimensions `(width, height)` required to render `text` at `scale`.
pub fn measure_string(text: &str, scale: u32) -> (u32, u32) {
    let s = scale.max(1);
    let mut max_width = 0u32;
    let mut current_width = 0u32;
    let mut lines = 1u32;

    for c in text.chars() {
        match c {
            '\n' => {
                max_width = max_width.max(current_width);
                current_width = 0;
                lines += 1;
            }
            '\r' => {
                current_width = 0;
            }
            '\t' => {
                current_width += 4 * 8 * s;
            }
            _ => {
                current_width += 8 * s;
            }
        }
    }

    max_width = max_width.max(current_width);
    let total_height = lines * 8 * s + (lines.saturating_sub(1)) * 2 * s;
    (max_width, total_height)
}
