//! Off-Screen Double Buffering Engine.
//!
//! Manages an off-screen surface backed by a dedicated 4 MiB virtual memory region
//! (`BACKBUFFER_START`), providing tear-free rendering and fast VRAM synchronization.

use bootloader_api::info::{FrameBuffer, PixelFormat};

use super::geometry::Rect;
use super::surface::Surface;
use crate::memory::{BACKBUFFER_SIZE, BACKBUFFER_START};

/// Instantiates an off-screen Surface backed by the dedicated 4 MiB virtual memory region.
///
/// # Safety
/// The caller must ensure that `memory::init()` has mapped `[BACKBUFFER_START, BACKBUFFER_START + BACKBUFFER_SIZE)`.
pub unsafe fn create_backbuffer_surface(
    width: usize,
    height: usize,
    stride: usize,
    pixel_format: PixelFormat,
) -> Surface<'static> {
    let bytes_per_pixel = match pixel_format {
        PixelFormat::Rgb | PixelFormat::Bgr => 4,
        PixelFormat::U8 => 1,
        _ => 4,
    };

    let required_bytes = stride * height * bytes_per_pixel;
    assert!(
        required_bytes <= BACKBUFFER_SIZE,
        "Framebuffer geometry requires {} bytes, but backbuffer size is {}",
        required_bytes,
        BACKBUFFER_SIZE
    );

    let slice = core::slice::from_raw_parts_mut(BACKBUFFER_START as *mut u8, required_bytes);

    Surface::from_raw_parts(
        slice,
        width,
        height,
        stride,
        bytes_per_pixel,
        pixel_format,
    )
}

/// Flushes the entire off-screen surface to the physical hardware framebuffer.
pub fn present(backbuffer: &Surface, fb: &mut FrameBuffer) {
    let fb_info = fb.info();
    let fb_buf = fb.buffer_mut();

    let rows = backbuffer.height().min(fb_info.height);
    let row_bytes = backbuffer.width().min(fb_info.width) * backbuffer.bytes_per_pixel();

    for y in 0..rows {
        let src_off = y * backbuffer.stride() * backbuffer.bytes_per_pixel();
        let dst_off = y * fb_info.stride * fb_info.bytes_per_pixel;

        if src_off + row_bytes <= BACKBUFFER_SIZE && dst_off + row_bytes <= fb_buf.len() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    (BACKBUFFER_START as *const u8).add(src_off),
                    fb_buf.as_mut_ptr().add(dst_off),
                    row_bytes,
                );
            }
        }
    }
}

/// Flushes a specific dirty rectangle from the off-screen surface to the hardware framebuffer.
#[allow(dead_code)]
pub fn present_rect(backbuffer: &Surface, fb: &mut FrameBuffer, rect: Rect) {
    let fb_info = fb.info();
    let screen_bounds = Rect::new(0, 0, fb_info.width as u32, fb_info.height as u32);

    let clipped = match screen_bounds.intersect(&rect) {
        Some(r) => r,
        None => return,
    };

    let fb_buf = fb.buffer_mut();
    let bpp = backbuffer.bytes_per_pixel();
    let row_bytes = clipped.width as usize * bpp;

    for y in clipped.y..clipped.bottom() {
        let uy = y as usize;
        let src_off = (uy * backbuffer.stride() + clipped.x as usize) * bpp;
        let dst_off = (uy * fb_info.stride + clipped.x as usize) * fb_info.bytes_per_pixel;

        if src_off + row_bytes <= BACKBUFFER_SIZE && dst_off + row_bytes <= fb_buf.len() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    (BACKBUFFER_START as *const u8).add(src_off),
                    fb_buf.as_mut_ptr().add(dst_off),
                    row_bytes,
                );
            }
        }
    }
}
