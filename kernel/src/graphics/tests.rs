//! Milestone 4A Graphics Engine Self-Tests.
//!
//! Validates coordinate boundaries, pixel rasterization, clipping invariants,
//! geometric primitives, typography, icon transparency, and double-buffer blitting.

use alloc::vec;
use bootloader_api::info::PixelFormat;

use super::color::Color;
use super::double_buffer;
use super::font::{draw_char, draw_string, measure_string};
use super::geometry::{Point, Rect};
use super::icon::{draw_icon, ICON_CHECK};
use super::surface::Surface;
use crate::memory::{BACKBUFFER_SIZE, BACKBUFFER_START};

/// Runs the complete Milestone 4A graphics subsystem self-test suite.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("        Running Micro-OS Phase 4A Graphics Engine Self-Tests    ");
    log::info!("================================================================");

    test_surface_geometry_and_bounds();
    test_pixel_rendering_and_formats();
    test_clipping_enforcement();
    test_rectangle_rendering();
    test_line_drawing();
    test_rounded_rectangles();
    test_text_rendering_and_measurement();
    test_icon_transparency();
    test_double_buffer_mapping_and_blit();

    log::info!("================================================================");
    log::info!("   [OK] All Phase 4A Graphics Engine Self-Tests Passed!         ");
    log::info!("================================================================");
}

/// 1. Verifies surface dimensions, stride, bytes per pixel, and boundary detection.
fn test_surface_geometry_and_bounds() {
    let w = 120;
    let h = 80;
    let stride = 128;
    let bpp = 4;
    let mut buf = vec![0u8; stride * h * bpp];

    let surface = Surface::from_raw_parts(&mut buf, w, h, stride, bpp, PixelFormat::Bgr);
    assert_eq!(surface.width(), w);
    assert_eq!(surface.height(), h);
    assert_eq!(surface.stride(), stride);
    assert_eq!(surface.bytes_per_pixel(), bpp);
    assert_eq!(surface.bounds(), Rect::new(0, 0, w as u32, h as u32));
    assert_eq!(surface.clip_rect(), Rect::new(0, 0, w as u32, h as u32));

    log::info!("  [PASS] 1. Surface geometry, stride, and boundary detection test");
}

/// 2. Verifies pixel rendering, RGB/BGR translation, and safe out-of-bounds handling.
fn test_pixel_rendering_and_formats() {
    let w = 64;
    let h = 64;
    let mut buf_bgr = vec![0u8; w * h * 4];
    let mut surface_bgr = Surface::from_raw_parts(&mut buf_bgr, w, h, w, 4, PixelFormat::Bgr);

    let test_color = Color::new(0x12, 0x34, 0x56);
    surface_bgr.set_pixel(10, 10, test_color);

    let read_back = surface_bgr.get_pixel(10, 10).expect("Pixel read failed");
    assert_eq!(read_back, test_color, "Pixel readback mismatch for BGR");

    // Out-of-bounds handling: must not panic and must return None
    surface_bgr.set_pixel(-5, 10, test_color);
    surface_bgr.set_pixel(10, -5, test_color);
    surface_bgr.set_pixel(100, 10, test_color);
    surface_bgr.set_pixel(10, 100, test_color);
    assert!(surface_bgr.get_pixel(-5, 10).is_none());
    assert!(surface_bgr.get_pixel(100, 10).is_none());

    // RGB Format check
    let mut buf_rgb = vec![0u8; w * h * 4];
    let mut surface_rgb = Surface::from_raw_parts(&mut buf_rgb, w, h, w, 4, PixelFormat::Rgb);
    surface_rgb.set_pixel(20, 20, test_color);
    let read_rgb = surface_rgb.get_pixel(20, 20).expect("Pixel read failed for RGB");
    assert_eq!(read_rgb, test_color, "Pixel readback mismatch for RGB");

    log::info!("  [PASS] 2. Pixel rendering, format translation (RGB/BGR), and boundary safety test");
}

/// 3. Verifies clipping rectangle enforcement and zero bleed outside clip bounds.
fn test_clipping_enforcement() {
    let w = 80;
    let h = 80;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    let bg_color = Color::new(0x10, 0x10, 0x10);
    surface.clear(bg_color);

    // Set constrained clip window: (20, 20, 40, 40)
    surface.set_clip_rect(Rect::new(20, 20, 40, 40));

    let fg_color = Color::new(0xFF, 0x00, 0x00);
    // Draw across the entire screen
    surface.fill_rect(Rect::new(0, 0, w as u32, h as u32), fg_color);

    // Pixels inside clip rect MUST be fg_color
    assert_eq!(surface.get_pixel(25, 25), Some(fg_color));
    assert_eq!(surface.get_pixel(55, 55), Some(fg_color));

    // Pixels outside clip rect MUST remain bg_color (zero bleed)
    assert_eq!(surface.get_pixel(10, 10), Some(bg_color));
    assert_eq!(surface.get_pixel(15, 25), Some(bg_color));
    assert_eq!(surface.get_pixel(65, 25), Some(bg_color));
    assert_eq!(surface.get_pixel(25, 65), Some(bg_color));

    surface.reset_clip_rect();
    assert_eq!(surface.clip_rect(), Rect::new(0, 0, w as u32, h as u32));

    log::info!("  [PASS] 3. Clipping rectangle enforcement and zero-bleed validation test");
}

/// 4. Verifies solid filled and outlined rectangles.
fn test_rectangle_rendering() {
    let w = 60;
    let h = 60;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    let bg = Color::BLACK;
    let fill = Color::new(0x38, 0xBD, 0xF8); // Azure
    let stroke = Color::new(0x00, 0xD2, 0xFF); // Cyan

    surface.clear(bg);
    surface.fill_rect(Rect::new(10, 10, 30, 20), fill);

    // Interior points
    assert_eq!(surface.get_pixel(10, 10), Some(fill));
    assert_eq!(surface.get_pixel(25, 20), Some(fill));
    assert_eq!(surface.get_pixel(39, 29), Some(fill));

    // Exterior points
    assert_eq!(surface.get_pixel(9, 10), Some(bg));
    assert_eq!(surface.get_pixel(40, 10), Some(bg));
    assert_eq!(surface.get_pixel(10, 30), Some(bg));

    // Stroke rect outline
    surface.stroke_rect(Rect::new(10, 10, 30, 20), 1, stroke);
    assert_eq!(surface.get_pixel(10, 10), Some(stroke));
    assert_eq!(surface.get_pixel(39, 10), Some(stroke));
    // Center remains fill
    assert_eq!(surface.get_pixel(25, 20), Some(fill));

    log::info!("  [PASS] 4. Solid rectangle filling and stroked outline rendering test");
}

/// 5. Verifies horizontal, vertical, and diagonal Bresenham lines.
fn test_line_drawing() {
    let w = 50;
    let h = 50;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    let color = Color::new(0x4A, 0xDE, 0x80); // Green
    surface.clear(Color::BLACK);

    // Horizontal line
    surface.draw_hline(5, 10, 20, color);
    for x in 5..25 {
        assert_eq!(surface.get_pixel(x, 10), Some(color));
    }
    assert_eq!(surface.get_pixel(4, 10), Some(Color::BLACK));
    assert_eq!(surface.get_pixel(25, 10), Some(Color::BLACK));

    // Vertical line
    surface.draw_vline(15, 20, 15, color);
    for y in 20..35 {
        assert_eq!(surface.get_pixel(15, y), Some(color));
    }
    assert_eq!(surface.get_pixel(15, 19), Some(Color::BLACK));
    assert_eq!(surface.get_pixel(15, 35), Some(Color::BLACK));

    // Diagonal Bresenham line: (0, 0) to (10, 10)
    surface.draw_line(Point::new(0, 0), Point::new(10, 10), color);
    for i in 0..=10 {
        assert_eq!(surface.get_pixel(i, i), Some(color));
    }

    log::info!("  [PASS] 5. Line drawing (horizontal, vertical, Bresenham diagonal) test");
}

/// 6. Verifies rounded rectangle rasterization and corner bounds.
fn test_rounded_rectangles() {
    let w = 60;
    let h = 60;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    let bg = Color::BLACK;
    let fill = Color::new(0x1A, 0x24, 0x3B);
    surface.clear(bg);

    let rect = Rect::new(10, 10, 40, 40);
    let radius = 8;
    surface.fill_rounded_rect(rect, radius, fill);

    // Center point must be filled
    assert_eq!(surface.get_pixel(30, 30), Some(fill));

    // Far corner point at (10, 10) must remain background (rounded cut-off)
    assert_eq!(surface.get_pixel(10, 10), Some(bg));
    assert_eq!(surface.get_pixel(49, 10), Some(bg));
    assert_eq!(surface.get_pixel(10, 49), Some(bg));
    assert_eq!(surface.get_pixel(49, 49), Some(bg));

    // Points inside the corner arc must be filled
    assert_eq!(surface.get_pixel(18, 18), Some(fill));

    log::info!("  [PASS] 6. Rounded rectangle geometry and corner curvature test");
}

/// 7. Verifies font rendering, multi-scale typography (1x, 2x), and string measurement.
fn test_text_rendering_and_measurement() {
    let (w1, h1) = measure_string("Hello", 1);
    assert_eq!(w1, 5 * 8);
    assert_eq!(h1, 8);

    let (w2, h2) = measure_string("Hello", 2);
    assert_eq!(w2, 5 * 16);
    assert_eq!(h2, 16);

    let (mw, mh) = measure_string("Line1\nLine2\nLongLine3", 1);
    assert_eq!(mw, 9 * 8);
    assert_eq!(mh, 3 * 8 + 2 * 2);

    let mut buf = vec![0u8; 100 * 50 * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, 100, 50, 100, 4, PixelFormat::Bgr);
    surface.clear(Color::BLACK);

    let fg = Color::WHITE;
    let bg = Color::SURFACE_SLATE;
    draw_char(&mut surface, 10, 10, 'A', fg, Some(bg), 1);

    // Background bounding box check (8x8)
    assert_eq!(surface.get_pixel(10, 10), Some(bg));
    // Character glyph pixel check (A has top horizontal bar)
    let glyph_drawn = (0..8).any(|x| surface.get_pixel(10 + x, 11) == Some(fg));
    assert!(glyph_drawn, "'A' glyph pixels must be drawn");

    // Draw string at 2x scale
    let end_pt = draw_string(&mut surface, 0, 20, "OK", fg, None, 2);
    assert_eq!(end_pt.x, 2 * 16);

    log::info!("  [PASS] 7. Text rendering, integer scaling (1x/2x), and typography measurement test");
}

/// 8. Verifies embedded icon rendering and transparency preservation.
fn test_icon_transparency() {
    let mut buf = vec![0u8; 32 * 32 * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, 32, 32, 32, 4, PixelFormat::Bgr);

    let test_bg = Color::new(0x22, 0x33, 0x44);
    surface.clear(test_bg);

    draw_icon(&mut surface, 4, 4, &ICON_CHECK, None);

    // Pixel (4, 4) is top-left corner of ICON_CHECK, which is transparent (0)
    assert_eq!(
        surface.get_pixel(4, 4),
        Some(test_bg),
        "Transparent pixel must preserve background color"
    );

    // Pixel (4+13, 4+1) is part of the checkmark tip, which is colored (SUCCESS_GREEN)
    assert_eq!(
        surface.get_pixel(4 + 13, 4 + 1),
        Some(Color::SUCCESS_GREEN),
        "Icon foreground pixel must be drawn"
    );

    log::info!("  [PASS] 8. Embedded bitmap icon rendering and transparency preservation test");
}

/// 9. Verifies dedicated 16 MiB backbuffer memory mapping, multi-resolution support (1280x800, 1920x1080),
/// capacity boundary checks, and surface blitting fidelity.
fn test_double_buffer_mapping_and_blit() {
    // 1. Verify that BACKBUFFER_START virtual memory was mapped during memory::init()
    let backbuffer_ptr = BACKBUFFER_START as *mut u8;
    assert!(!backbuffer_ptr.is_null());
    assert_eq!(BACKBUFFER_SIZE, 16 * 1024 * 1024, "BACKBUFFER_SIZE must be exactly 16 MiB");

    // 2. Validate Existing 1280x800 Framebuffer Geometry (QEMU / Default)
    // 1280 * 800 * 4 = 4,096,000 bytes (~3.91 MiB <= 16 MiB)
    let res_1280_res = unsafe {
        double_buffer::try_create_backbuffer_surface(1280, 800, 1280, PixelFormat::Bgr)
    };
    assert!(res_1280_res.is_ok(), "1280x800 backbuffer creation must succeed");
    let backbuffer_1280 = res_1280_res.unwrap();
    assert_eq!(backbuffer_1280.width(), 1280);
    assert_eq!(backbuffer_1280.height(), 800);
    assert_eq!(backbuffer_1280.stride(), 1280);
    assert_eq!(backbuffer_1280.bytes_per_pixel(), 4);

    // 3. Validate Physical HP Victus 1920x1080 Framebuffer Geometry (FHD Native)
    // 1920 * 1080 * 4 = 8,294,400 bytes (~7.91 MiB <= 16 MiB)
    let res_1080_res = unsafe {
        double_buffer::try_create_backbuffer_surface(1920, 1080, 1920, PixelFormat::Bgr)
    };
    assert!(res_1080_res.is_ok(), "1920x1080 FHD backbuffer creation must succeed");
    let backbuffer_1080 = res_1080_res.unwrap();
    assert_eq!(backbuffer_1080.width(), 1920);
    assert_eq!(backbuffer_1080.height(), 1080);
    assert_eq!(backbuffer_1080.stride(), 1920);
    assert_eq!(backbuffer_1080.bytes_per_pixel(), 4);

    // 4. Validate QHD 2560x1440 Framebuffer Geometry
    // 2560 * 1440 * 4 = 14,745,600 bytes (~14.06 MiB <= 16 MiB)
    let res_1440_res = unsafe {
        double_buffer::try_create_backbuffer_surface(2560, 1440, 2560, PixelFormat::Bgr)
    };
    assert!(res_1440_res.is_ok(), "2560x1440 QHD backbuffer creation must succeed");

    // 5. Validate Backbuffer Capacity Boundaries (Excessively Large Framebuffers Fail Safely)
    // 4K UHD: 3840 * 2160 * 4 = 33,177,600 bytes (~31.6 MiB > 16 MiB)
    let res_4k = unsafe {
        double_buffer::try_create_backbuffer_surface(3840, 2160, 3840, PixelFormat::Bgr)
    };
    match res_4k {
        Err(double_buffer::BackbufferError::CapacityExceeded { required, max }) => {
            assert_eq!(required, 33_177_600);
            assert_eq!(max, 16 * 1024 * 1024);
        }
        _ => panic!("Expected CapacityExceeded for 4K UHD"),
    }

    // Geometry arithmetic overflow protection
    let res_overflow = unsafe {
        double_buffer::try_create_backbuffer_surface(usize::MAX, 100, usize::MAX, PixelFormat::Bgr)
    };
    match res_overflow {
        Err(double_buffer::BackbufferError::GeometryOverflow) => {}
        _ => panic!("Overflowing geometry must safely return GeometryOverflow"),
    }

    // 6. Instantiate test surface and verify drawing & off-screen blitting fidelity
    let mut backbuffer = unsafe {
        double_buffer::create_backbuffer_surface(320, 240, 320, PixelFormat::Bgr)
    };
    assert_eq!(backbuffer.width(), 320);
    assert_eq!(backbuffer.height(), 240);

    let fill_color = Color::new(0x13, 0x1B, 0x2E);
    let accent_color = Color::new(0x38, 0xBD, 0xF8);
    backbuffer.clear(fill_color);
    backbuffer.fill_rect(Rect::new(20, 20, 100, 50), accent_color);

    assert_eq!(backbuffer.get_pixel(10, 10), Some(fill_color));
    assert_eq!(backbuffer.get_pixel(25, 25), Some(accent_color));

    // Blit from backbuffer to a destination target surface
    let mut target_buf = vec![0u8; 320 * 240 * 4];
    let mut target_surface = Surface::from_raw_parts(&mut target_buf, 320, 240, 320, 4, PixelFormat::Bgr);
    target_surface.clear(Color::BLACK);

    backbuffer.copy_to(&mut target_surface);
    assert_eq!(target_surface.get_pixel(10, 10), Some(fill_color));
    assert_eq!(target_surface.get_pixel(25, 25), Some(accent_color));

    log::info!("  [PASS] 9. Dedicated 16 MiB backbuffer mapping, multi-resolution (1280x800, 1920x1080) and bounds test");
}
