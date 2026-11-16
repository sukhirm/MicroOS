//! Software Mouse Cursor Component.
//!
//! Provides a high-contrast 12x18 arrow pointer sprite with a dark border,
//! crisp white interior, cyan tip accent, and modular background save/restore logic.

use crate::graphics::color::Color;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;

/// Cursor sprite width in pixels.
pub const CURSOR_WIDTH: u32 = 12;

/// Cursor sprite height in pixels.
pub const CURSOR_HEIGHT: u32 = 18;

/// 12x18 raster cursor sprite:
/// 0: Transparent
/// 1: Dark Slate / Black outline (for visibility on light surfaces)
/// 2: Pure White fill
/// 3: Accent Cyan tip
#[rustfmt::skip]
pub static CURSOR_SPRITE: [u8; (CURSOR_WIDTH * CURSOR_HEIGHT) as usize] = [
    1,0,0,0,0,0,0,0,0,0,0,0,
    1,1,0,0,0,0,0,0,0,0,0,0,
    1,3,1,0,0,0,0,0,0,0,0,0,
    1,2,3,1,0,0,0,0,0,0,0,0,
    1,2,2,3,1,0,0,0,0,0,0,0,
    1,2,2,2,3,1,0,0,0,0,0,0,
    1,2,2,2,2,3,1,0,0,0,0,0,
    1,2,2,2,2,2,3,1,0,0,0,0,
    1,2,2,2,2,2,2,3,1,0,0,0,
    1,2,2,2,2,2,2,2,3,1,0,0,
    1,2,2,2,2,2,1,1,1,1,1,0,
    1,2,2,1,2,2,1,0,0,0,0,0,
    1,2,1,0,1,2,2,1,0,0,0,0,
    1,1,0,0,1,2,2,1,0,0,0,0,
    1,0,0,0,0,1,2,2,1,0,0,0,
    0,0,0,0,0,1,2,2,1,0,0,0,
    0,0,0,0,0,0,1,1,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,
];

/// Encapsulates the software mouse cursor state.
pub struct Cursor {
    pub position: Point,
    pub visible: bool,
    saved_pixels: [Color; (CURSOR_WIDTH * CURSOR_HEIGHT) as usize],
    saved_rect: Rect,
}

impl Cursor {
    /// Creates a new cursor initialized at the specified screen coordinate.
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            position: Point::new(x, y),
            visible: true,
            saved_pixels: [Color::BLACK; (CURSOR_WIDTH * CURSOR_HEIGHT) as usize],
            saved_rect: Rect::new(x, y, CURSOR_WIDTH, CURSOR_HEIGHT),
        }
    }

    /// Returns the current cursor hotspot position.
    #[inline(always)]
    pub fn position(&self) -> Point {
        self.position
    }

    /// Updates the cursor position.
    pub fn set_position(&mut self, pos: Point) {
        self.position = pos;
    }

    /// Returns the bounding rectangle of the cursor sprite.
    #[inline(always)]
    pub fn bounds(&self) -> Rect {
        Rect::new(self.position.x, self.position.y, CURSOR_WIDTH, CURSOR_HEIGHT)
    }

    /// Backs up the surface pixels currently beneath the cursor.
    pub fn save_under(&mut self, surface: &Surface) {
        self.saved_rect = self.bounds();
        let mut idx = 0;
        for dy in 0..CURSOR_HEIGHT as i32 {
            for dx in 0..CURSOR_WIDTH as i32 {
                let px = self.position.x + dx;
                let py = self.position.y + dy;
                self.saved_pixels[idx] = surface.get_pixel(px, py).unwrap_or(Color::BLACK);
                idx += 1;
            }
        }
    }

    /// Restores the backed-up pixels to the surface, erasing the cursor.
    pub fn restore_under(&self, surface: &mut Surface) {
        let mut idx = 0;
        for dy in 0..CURSOR_HEIGHT as i32 {
            for dx in 0..CURSOR_WIDTH as i32 {
                let px = self.saved_rect.x + dx;
                let py = self.saved_rect.y + dy;
                surface.set_pixel(px, py, self.saved_pixels[idx]);
                idx += 1;
            }
        }
    }

    /// Renders the cursor sprite onto `surface`.
    pub fn render(&mut self, surface: &mut Surface) {
        if !self.visible {
            return;
        }

        self.save_under(surface);

        let w = CURSOR_WIDTH as i32;
        let h = CURSOR_HEIGHT as i32;

        for row in 0..h {
            for col in 0..w {
                let idx = (row * w + col) as usize;
                let val = CURSOR_SPRITE[idx];

                match val {
                    1 => surface.set_pixel(self.position.x + col, self.position.y + row, Color::OBSIDIAN_NAVY),
                    2 => surface.set_pixel(self.position.x + col, self.position.y + row, Color::WHITE),
                    3 => surface.set_pixel(self.position.x + col, self.position.y + row, Color::ACCENT_CYAN),
                    _ => {} // 0: Transparent
                }
            }
        }
    }
}
