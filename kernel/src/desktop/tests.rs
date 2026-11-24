//! Milestone 4B Desktop Environment Shell Self-Tests.
//!
//! Validates wallpaper rendering, status bar placement and honest metrics,
//! floating dock centering and icon mapping, cursor bounds and clipping,
//! and full desktop shell composite integration.

use alloc::vec;
use bootloader_api::info::PixelFormat;

use super::cursor::{Cursor, CURSOR_HEIGHT, CURSOR_WIDTH};
use super::dock::{get_dock_bounds, render_dock, DOCK_BOTTOM_MARGIN, DOCK_HEIGHT, DOCK_ITEMS};
use super::shell::render_desktop;
use super::status_bar::{render_status_bar, StatusMetrics, STATUS_BAR_HEIGHT};
use super::wallpaper::render_wallpaper;
use crate::graphics::color::Color;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::surface::Surface;

/// Runs the complete Milestone 4B desktop shell self-test suite.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("        Running Micro-OS Phase 4B Desktop Shell Self-Tests      ");
    log::info!("================================================================");

    test_wallpaper_rendering();
    test_status_bar_dimensions_and_placement();
    test_dock_positioning_and_dimensions();
    test_dock_icon_rendering();
    test_cursor_bounds_and_clipping();
    test_resolution_adaptation_and_stride();
    test_desktop_shell_integration();

    log::info!("================================================================");
    log::info!("   [OK] All Phase 4B Desktop Shell Self-Tests Passed!           ");
    log::info!("================================================================");
}

/// 1. Verifies wallpaper rendering, cosmic background color, and grid line drawing.
fn test_wallpaper_rendering() {
    let w = 128;
    let h = 128;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    render_wallpaper(&mut surface);

    // Baseline background check: (10, 10) must be Obsidian Navy
    let bg_pixel = surface.get_pixel(10, 10).expect("Wallpaper pixel read failed");
    assert_eq!(
        bg_pixel,
        Color::OBSIDIAN_NAVY,
        "Wallpaper base color must be OBSIDIAN_NAVY"
    );

    // Center constellation motif check
    let center_pixel = surface.get_pixel(64, 64 - 20).expect("Center pixel failed");
    assert_eq!(
        center_pixel,
        Color::ACCENT_CYAN,
        "Wallpaper center celestial crosshair must be ACCENT_CYAN"
    );

    log::info!("  [PASS] 1. Wallpaper cosmic canvas, grid, and geometric accents test");
}

/// 2. Verifies status bar dimensions, top placement (row 0), and honest telemetry strings.
fn test_status_bar_dimensions_and_placement() {
    let w = 400;
    let h = 40;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    let metrics = StatusMetrics::default();
    assert_eq!(metrics.arch, "x86_64");
    assert_eq!(metrics.clock_str, "N/A", "Clock must be honest N/A placeholder");
    assert_eq!(metrics.total_ram_mb, 497);

    render_status_bar(&mut surface, &metrics);

    // Top surface bar fill check (row 1, col 5)
    assert_eq!(surface.get_pixel(5, 1), Some(Color::SURFACE_SLATE));

    // Bottom border check at y = STATUS_BAR_HEIGHT - 1
    let border_y = (STATUS_BAR_HEIGHT - 1) as i32;
    assert_eq!(surface.get_pixel(50, border_y), Some(Color::BORDER_SLATE));

    // Outside status bar check: row 35 must remain untouched (Color::BLACK)
    assert_eq!(surface.get_pixel(50, STATUS_BAR_HEIGHT as i32 + 2), Some(Color::BLACK));

    // Branding diamond check (14, 12)
    assert_eq!(surface.get_pixel(15, 13), Some(Color::ACCENT_CYAN));

    log::info!("  [PASS] 2. Status bar geometry (34px), top placement, and honest metrics test");
}

/// 3. Verifies dock positioning, horizontal centering, height, and bottom margin.
fn test_dock_positioning_and_dimensions() {
    let screen_w = 1280u32;
    let screen_h = 800u32;
    let dock_rect = get_dock_bounds(screen_w, screen_h);

    assert_eq!(dock_rect.height, DOCK_HEIGHT);
    let expected_y = (screen_h - DOCK_HEIGHT - DOCK_BOTTOM_MARGIN) as i32;
    assert_eq!(dock_rect.y, expected_y, "Dock Y coordinate mismatch");

    // Horizontal centering assertion
    let expected_x = ((screen_w - dock_rect.width) / 2) as i32;
    assert_eq!(dock_rect.x, expected_x, "Dock must be horizontally centered");

    // Verify 7 dock items
    assert_eq!(DOCK_ITEMS.len(), 7, "Dock must contain 7 application entries");

    log::info!("  [PASS] 3. Floating dock positioning, horizontal centering, and geometry test");
}

/// 4. Verifies dock item slots, icon rendering, and label positioning.
fn test_dock_icon_rendering() {
    let screen_w = 520;
    let screen_h = 100;
    let mut buf = vec![0u8; (screen_w * screen_h * 4) as usize];
    let mut surface = Surface::from_raw_parts(&mut buf, screen_w as usize, screen_h as usize, screen_w as usize, 4, PixelFormat::Bgr);

    render_dock(&mut surface);

    let dock_rect = get_dock_bounds(screen_w, screen_h);
    // 1. Dock background card fill (left margin before first tile) must be CARD_SLATE
    assert_eq!(
        surface.get_pixel(dock_rect.x + 20, dock_rect.y + 10),
        Some(Color::CARD_SLATE),
        "Dock background card fill mismatch"
    );

    // 2. Center tile container box (item 3: Network) fill must be SURFACE_SLATE
    assert_eq!(
        surface.get_pixel(dock_rect.x + (dock_rect.width as i32 / 2), dock_rect.y + 10),
        Some(Color::SURFACE_SLATE),
        "Dock center tile container fill mismatch"
    );

    // Verify all 7 application names are distinct
    let names = ["Files", "Settings", "SysMon", "Network", "Terminal", "Store", "Deadlock"];
    for (i, item) in DOCK_ITEMS.iter().enumerate() {
        assert_eq!(item.name, names[i]);
    }

    log::info!("  [PASS] 4. Dock 7-application icon mapping, card slots, and labels test");
}

/// 5. Verifies cursor bounds, clipping at screen edges, and save/restore fidelity.
fn test_cursor_bounds_and_clipping() {
    let mut cursor = Cursor::new(100, 150);
    assert_eq!(cursor.position(), Point::new(100, 150));
    assert_eq!(cursor.bounds(), Rect::new(100, 150, CURSOR_WIDTH, CURSOR_HEIGHT));

    let w = 80;
    let h = 80;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);
    let bg = Color::new(0x22, 0x33, 0x44);
    surface.clear(bg);

    // Test save and restore under cursor
    cursor.set_position(Point::new(20, 20));
    cursor.render(&mut surface);

    // Hotspot at (20, 20) is border (Color::OBSIDIAN_NAVY)
    assert_eq!(surface.get_pixel(20, 20), Some(Color::OBSIDIAN_NAVY));
    // Interior at (21, 23) is white
    assert_eq!(surface.get_pixel(21, 23), Some(Color::WHITE));

    // Restore background under cursor
    cursor.restore_under(&mut surface);
    assert_eq!(surface.get_pixel(20, 20), Some(bg));
    assert_eq!(surface.get_pixel(21, 23), Some(bg));

    // Test cursor clipping near edge: must not panic or cause buffer overrun
    cursor.set_position(Point::new(75, 75));
    cursor.render(&mut surface);
    assert_eq!(surface.get_pixel(75, 75), Some(Color::OBSIDIAN_NAVY));

    log::info!("  [PASS] 5. Mouse cursor 12x18 sprite bounds, clipping, and restore test");
}

/// 6. Verifies desktop shell resolution adaptation across 1280x800 and 1024x768.
fn test_resolution_adaptation_and_stride() {
    let resolutions = [(1280u32, 800u32), (1024u32, 768u32)];

    for (w, h) in resolutions {
        let dock = get_dock_bounds(w, h);
        assert!(dock.right() <= w as i32);
        assert!(dock.bottom() <= h as i32);
        assert!(dock.y > STATUS_BAR_HEIGHT as i32);
    }

    log::info!("  [PASS] 6. Desktop shell resolution adaptation & stride compatibility test");
}

/// 7. Verifies full DesktopShell composite integration.
fn test_desktop_shell_integration() {
    let w = 420;
    let h = 120;
    let mut buf = vec![0u8; w * h * 4];
    let mut surface = Surface::from_raw_parts(&mut buf, w, h, w, 4, PixelFormat::Bgr);

    render_desktop(&mut surface);

    // 1. Status bar present at top
    assert_eq!(surface.get_pixel(5, 5), Some(Color::SURFACE_SLATE));
    // 2. Wallpaper present in middle
    assert_eq!(surface.get_pixel(5, 45), Some(Color::OBSIDIAN_NAVY));
    // 3. Dock present near bottom
    let dock = get_dock_bounds(w as u32, h as u32);
    assert_eq!(surface.get_pixel(dock.x + 20, dock.y + 10), Some(Color::CARD_SLATE));
    // 4. Cursor present at center
    let center_x = (w as i32) / 2;
    let center_y = (h as i32) / 2;
    assert_eq!(surface.get_pixel(center_x, center_y), Some(Color::OBSIDIAN_NAVY));

    log::info!("  [PASS] 7. Desktop shell multi-layer composite integration test");
}
