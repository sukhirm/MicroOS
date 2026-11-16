//! Reusable Drawing Surface Abstraction and 2D Rendering Engine.
//!
//! Provides a safe, clip-aware drawing target that supports linear framebuffers,
//! off-screen double buffers, hardware format adaptation (RGB/BGR), and primitive rasterization.

use bootloader_api::info::{FrameBuffer, PixelFormat};
use core::marker::PhantomData;

use super::color::Color;
use super::geometry::{Point, Rect};

/// A 2D linear pixel drawing surface.
pub struct Surface<'a> {
    buffer: *mut u8,
    buffer_len: usize,
    width: usize,
    height: usize,
    stride: usize,
    bytes_per_pixel: usize,
    pixel_format: PixelFormat,
    clip_rect: Rect,
    _marker: PhantomData<&'a mut [u8]>,
}

// SAFETY: Surfaces can be shared across kernel threads when wrapped in SpinMutex.
unsafe impl Send for Surface<'_> {}
unsafe impl Sync for Surface<'_> {}

#[allow(dead_code)]
impl<'a> Surface<'a> {
    /// Constructs a Surface from a raw buffer slice and pixel geometry.
    pub fn from_raw_parts(
        buffer: &'a mut [u8],
        width: usize,
        height: usize,
        stride: usize,
        bytes_per_pixel: usize,
        pixel_format: PixelFormat,
    ) -> Self {
        let clip_rect = Rect::new(0, 0, width as u32, height as u32);
        let buffer_len = buffer.len();
        Self {
            buffer: buffer.as_mut_ptr(),
            buffer_len,
            width,
            height,
            stride,
            bytes_per_pixel,
            pixel_format,
            clip_rect,
            _marker: PhantomData,
        }
    }

    /// Constructs a Surface wrapping the bootloader's UEFI GOP framebuffer.
    pub fn from_framebuffer(fb: &'a mut FrameBuffer) -> Self {
        let info = fb.info();
        let width = info.width;
        let height = info.height;
        let stride = info.stride;
        let bytes_per_pixel = info.bytes_per_pixel;
        let pixel_format = info.pixel_format;
        let buffer = fb.buffer_mut();
        let buffer_len = buffer.len();
        let clip_rect = Rect::new(0, 0, width as u32, height as u32);

        Self {
            buffer: buffer.as_mut_ptr(),
            buffer_len,
            width,
            height,
            stride,
            bytes_per_pixel,
            pixel_format,
            clip_rect,
            _marker: PhantomData,
        }
    }

    /// Surface width in pixels.
    #[inline(always)]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Surface height in pixels.
    #[inline(always)]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Framebuffer stride (pixels per line).
    #[inline(always)]
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Bytes per pixel.
    #[inline(always)]
    pub fn bytes_per_pixel(&self) -> usize {
        self.bytes_per_pixel
    }

    /// Pixel format (RGB, BGR, U8).
    #[inline(always)]
    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// Full screen bounding rectangle of this surface.
    #[inline(always)]
    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width as u32, self.height as u32)
    }

    /// Current active clipping rectangle.
    #[inline(always)]
    pub fn clip_rect(&self) -> Rect {
        self.clip_rect
    }

    /// Sets the active clipping rectangle, intersected with the surface boundaries.
    pub fn set_clip_rect(&mut self, rect: Rect) {
        if let Some(clipped) = self.bounds().intersect(&rect) {
            self.clip_rect = clipped;
        } else {
            self.clip_rect = Rect::new(0, 0, 0, 0);
        }
    }

    /// Resets the clipping rectangle to the full surface dimensions.
    pub fn reset_clip_rect(&mut self) {
        self.clip_rect = self.bounds();
    }

    /// Writes a single pixel with clipping and format translation.
    #[inline]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if !self.clip_rect.contains(Point::new(x, y)) {
            return;
        }

        let ux = x as usize;
        let uy = y as usize;
        let offset = (uy * self.stride + ux) * self.bytes_per_pixel;

        if offset + self.bytes_per_pixel > self.buffer_len {
            return;
        }

        let final_color = if color.a < 255 {
            if let Some(existing) = self.get_pixel(x, y) {
                color.blend_over(existing)
            } else {
                color
            }
        } else {
            color
        };

        unsafe {
            let ptr = self.buffer.add(offset);
            match self.pixel_format {
                PixelFormat::Rgb => {
                    *ptr = final_color.r;
                    *ptr.add(1) = final_color.g;
                    *ptr.add(2) = final_color.b;
                }
                PixelFormat::Bgr => {
                    *ptr = final_color.b;
                    *ptr.add(1) = final_color.g;
                    *ptr.add(2) = final_color.r;
                }
                PixelFormat::U8 => {
                    let gray = ((final_color.r as u16 * 77
                        + final_color.g as u16 * 150
                        + final_color.b as u16 * 29)
                        >> 8) as u8;
                    *ptr = gray;
                }
                _ => {
                    *ptr = final_color.b;
                    *ptr.add(1) = final_color.g;
                    *ptr.add(2) = final_color.r;
                }
            }
        }
    }

    /// Reads a single pixel from the surface, if within bounds.
    pub fn get_pixel(&self, x: i32, y: i32) -> Option<Color> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }

        let ux = x as usize;
        let uy = y as usize;
        let offset = (uy * self.stride + ux) * self.bytes_per_pixel;

        if offset + self.bytes_per_pixel > self.buffer_len {
            return None;
        }

        unsafe {
            let ptr = self.buffer.add(offset);
            match self.pixel_format {
                PixelFormat::Rgb => Some(Color::new(*ptr, *ptr.add(1), *ptr.add(2))),
                PixelFormat::Bgr => Some(Color::new(*ptr.add(2), *ptr.add(1), *ptr)),
                PixelFormat::U8 => {
                    let g = *ptr;
                    Some(Color::new(g, g, g))
                }
                _ => Some(Color::new(*ptr.add(2), *ptr.add(1), *ptr)),
            }
        }
    }

    /// Clears the entire surface with a solid color.
    pub fn clear(&mut self, color: Color) {
        let b = self.bounds();
        self.fill_rect(b, color);
    }

    /// Fills a solid rectangle with clipping.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if let Some(clipped) = self.clip_rect.intersect(&rect) {
            for y in clipped.y..clipped.bottom() {
                for x in clipped.x..clipped.right() {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    /// Outlines a rectangle with a specified stroke thickness.
    pub fn stroke_rect(&mut self, rect: Rect, thickness: u32, color: Color) {
        if rect.is_empty() || thickness == 0 {
            return;
        }

        let t = thickness as i32;
        // Top edge
        self.fill_rect(Rect::new(rect.x, rect.y, rect.width, thickness), color);
        // Bottom edge
        self.fill_rect(
            Rect::new(rect.x, rect.bottom() - t, rect.width, thickness),
            color,
        );
        // Left edge
        self.fill_rect(Rect::new(rect.x, rect.y + t, thickness, rect.height.saturating_sub(thickness * 2)), color);
        // Right edge
        self.fill_rect(
            Rect::new(rect.right() - t, rect.y + t, thickness, rect.height.saturating_sub(thickness * 2)),
            color,
        );
    }

    /// Draws a fast horizontal line.
    pub fn draw_hline(&mut self, x: i32, y: i32, length: u32, color: Color) {
        self.fill_rect(Rect::new(x, y, length, 1), color);
    }

    /// Draws a fast vertical line.
    pub fn draw_vline(&mut self, x: i32, y: i32, length: u32, color: Color) {
        self.fill_rect(Rect::new(x, y, 1, length), color);
    }

    /// Draws an arbitrary 2D line using Bresenham's line algorithm.
    pub fn draw_line(&mut self, p1: Point, p2: Point, color: Color) {
        let mut x0 = p1.x;
        let mut y0 = p1.y;
        let x1 = p2.x;
        let y1 = p2.y;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.set_pixel(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Fills a rounded rectangle with corner radius `radius`.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: u32, color: Color) {
        if rect.is_empty() {
            return;
        }

        let max_r = (rect.width.min(rect.height) / 2) as u32;
        let r = radius.min(max_r) as i32;

        if r <= 0 {
            self.fill_rect(rect, color);
            return;
        }

        let r_u32 = r as u32;

        // 1. Central horizontal body: (x, y + r, w, h - 2*r)
        self.fill_rect(
            Rect::new(
                rect.x,
                rect.y + r,
                rect.width,
                rect.height.saturating_sub(r_u32 * 2),
            ),
            color,
        );

        // 2. Top and bottom central blocks: (x + r, y, w - 2*r, r)
        let middle_w = rect.width.saturating_sub(r_u32 * 2);
        self.fill_rect(Rect::new(rect.x + r, rect.y, middle_w, r_u32), color);
        self.fill_rect(
            Rect::new(rect.x + r, rect.bottom() - r, middle_w, r_u32),
            color,
        );

        // 3. Four corner quarter-circles
        let r_sq = r * r;
        for dy in 0..r {
            for dx in 0..r {
                let dist_sq = (r - 1 - dx) * (r - 1 - dx) + (r - 1 - dy) * (r - 1 - dy);
                if dist_sq <= r_sq {
                    // Top-Left
                    self.set_pixel(rect.x + dx, rect.y + dy, color);
                    // Top-Right
                    self.set_pixel(rect.right() - 1 - dx, rect.y + dy, color);
                    // Bottom-Left
                    self.set_pixel(rect.x + dx, rect.bottom() - 1 - dy, color);
                    // Bottom-Right
                    self.set_pixel(rect.right() - 1 - dx, rect.bottom() - 1 - dy, color);
                }
            }
        }
    }

    /// Outlines a rounded rectangle with stroke `thickness`.
    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        radius: u32,
        thickness: u32,
        color: Color,
    ) {
        if rect.is_empty() || thickness == 0 {
            return;
        }

        let max_r = (rect.width.min(rect.height) / 2) as u32;
        let r = radius.min(max_r) as i32;

        if r <= 0 {
            self.stroke_rect(rect, thickness, color);
            return;
        }

        let r_u32 = r as u32;
        let t = thickness as i32;

        // Top line
        self.fill_rect(
            Rect::new(rect.x + r, rect.y, rect.width.saturating_sub(r_u32 * 2), thickness),
            color,
        );
        // Bottom line
        self.fill_rect(
            Rect::new(rect.x + r, rect.bottom() - t, rect.width.saturating_sub(r_u32 * 2), thickness),
            color,
        );
        // Left line
        self.fill_rect(
            Rect::new(rect.x, rect.y + r, thickness, rect.height.saturating_sub(r_u32 * 2)),
            color,
        );
        // Right line
        self.fill_rect(
            Rect::new(rect.right() - t, rect.y + r, thickness, rect.height.saturating_sub(r_u32 * 2)),
            color,
        );

        // Corner arcs
        let outer_sq = r * r;
        let inner_r = (r - t).max(0);
        let inner_sq = inner_r * inner_r;

        for dy in 0..r {
            for dx in 0..r {
                let dist_sq = (r - 1 - dx) * (r - 1 - dx) + (r - 1 - dy) * (r - 1 - dy);
                if dist_sq <= outer_sq && dist_sq >= inner_sq {
                    // Top-Left
                    self.set_pixel(rect.x + dx, rect.y + dy, color);
                    // Top-Right
                    self.set_pixel(rect.right() - 1 - dx, rect.y + dy, color);
                    // Bottom-Left
                    self.set_pixel(rect.x + dx, rect.bottom() - 1 - dy, color);
                    // Bottom-Right
                    self.set_pixel(rect.right() - 1 - dx, rect.bottom() - 1 - dy, color);
                }
            }
        }
    }

    /// Blits a rectangular region from another surface onto this surface.
    pub fn blit(&mut self, src: &Surface, src_rect: Rect, dest_pos: Point) {
        let clipped_src = match src.bounds().intersect(&src_rect) {
            Some(r) => r,
            None => return,
        };

        for row in 0..clipped_src.height as i32 {
            for col in 0..clipped_src.width as i32 {
                if let Some(color) = src.get_pixel(clipped_src.x + col, clipped_src.y + row) {
                    self.set_pixel(dest_pos.x + col, dest_pos.y + row, color);
                }
            }
        }
    }

    /// Fast row-by-row memory copy to another surface with identical geometry.
    pub fn copy_to(&self, dest: &mut Surface) {
        let rows = self.height.min(dest.height);
        let row_bytes = self.width.min(dest.width) * self.bytes_per_pixel;

        for y in 0..rows {
            let src_offset = y * self.stride * self.bytes_per_pixel;
            let dest_offset = y * dest.stride * dest.bytes_per_pixel;

            if src_offset + row_bytes <= self.buffer_len && dest_offset + row_bytes <= dest.buffer_len {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        self.buffer.add(src_offset),
                        dest.buffer.add(dest_offset),
                        row_bytes,
                    );
                }
            }
        }
    }
}
