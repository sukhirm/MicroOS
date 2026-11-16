//! Desktop Floating Application Dock Component.
//!
//! Provides a floating, rounded shelf centered near the bottom of the screen,
//! featuring embedded icons for the 6 core MicroOS system applications:
//! File Manager, Settings, System Monitor, Network Manager, Terminal, and Software Store.

use crate::graphics::color::Color;
use crate::graphics::font::{draw_string, measure_string};
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::*;
use crate::graphics::surface::Surface;

/// Height of the floating dock in pixels.
pub const DOCK_HEIGHT: u32 = 62;

/// Margin from the bottom of the screen in pixels.
pub const DOCK_BOTTOM_MARGIN: u32 = 14;

/// An individual application entry on the desktop dock.
pub struct DockItem {
    pub name: &'static str,
    pub icon: &'static Icon,
    pub accent_color: Color,
}

/// The 6 canonical application entries displayed on the MicroOS desktop dock.
pub static DOCK_ITEMS: [DockItem; 6] = [
    DockItem {
        name: "Files",
        icon: &ICON_FOLDER,
        accent_color: Color::ACCENT_AZURE,
    },
    DockItem {
        name: "Settings",
        icon: &ICON_GEAR,
        accent_color: Color::TEXT_LIGHT,
    },
    DockItem {
        name: "SysMon",
        icon: &ICON_ACTIVITY,
        accent_color: Color::SUCCESS_GREEN,
    },
    DockItem {
        name: "Network",
        icon: &ICON_NETWORK,
        accent_color: Color::ACCENT_CYAN,
    },
    DockItem {
        name: "Terminal",
        icon: &ICON_TERMINAL,
        accent_color: Color::ACCENT_AZURE,
    },
    DockItem {
        name: "Store",
        icon: &ICON_STORE,
        accent_color: Color::WARNING_AMBER,
    },
];

/// Computes the bounding rectangle of the floating dock given the screen dimensions.
pub fn get_dock_bounds(screen_w: u32, screen_h: u32) -> Rect {
    let item_slot_w = 64u32;
    let padding_h = 16u32;
    let total_w = (DOCK_ITEMS.len() as u32 * item_slot_w) + (padding_h * 2);
    let x = (screen_w.saturating_sub(total_w)) / 2;
    let y = screen_h.saturating_sub(DOCK_HEIGHT + DOCK_BOTTOM_MARGIN);

    Rect::new(x as i32, y as i32, total_w, DOCK_HEIGHT)
}

/// Renders the floating application dock onto `surface`.
pub fn render_dock(surface: &mut Surface) {
    let screen_w = surface.width() as u32;
    let screen_h = surface.height() as u32;
    let dock_rect = get_dock_bounds(screen_w, screen_h);

    // 1. Elevated Dock Background Card with rounded corners
    surface.fill_rounded_rect(dock_rect, 14, Color::CARD_SLATE);
    surface.stroke_rounded_rect(dock_rect, 14, 1, Color::BORDER_SLATE);

    // Top highlight rim (simulating subtle glass reflection)
    let rim_w = dock_rect.width - 28;
    surface.draw_hline(
        dock_rect.x + 14,
        dock_rect.y + 1,
        rim_w,
        Color::rgba(0x38, 0xbd, 0xf8, 60),
    );

    // 2. Render each dock tile
    let item_slot_w = 64i32;
    let start_x = dock_rect.x + 16;
    let slot_y = dock_rect.y + 6;

    for (i, item) in DOCK_ITEMS.iter().enumerate() {
        let cur_x = start_x + (i as i32 * item_slot_w);

        // Tile container box (36x36 with 8px rounded corners)
        let tile_rect = Rect::new(cur_x + 14, slot_y, 36, 36);
        surface.fill_rounded_rect(tile_rect, 8, Color::SURFACE_SLATE);
        surface.stroke_rounded_rect(tile_rect, 8, 1, Color::BORDER_SLATE);

        // Draw embedded 16x16 icon centered inside tile box
        let icon_x = tile_rect.x + 10;
        let icon_y = tile_rect.y + 10;
        draw_icon(surface, icon_x, icon_y, item.icon, Some(item.accent_color));

        // Draw application label underneath tile
        let (label_w, _) = measure_string(item.name, 1);
        let label_x = cur_x + (item_slot_w - label_w as i32) / 2;
        let label_y = tile_rect.bottom() + 4;
        draw_string(surface, label_x, label_y, item.name, Color::TEXT_MUTED, None, 1);
    }
}

/// Tests whether a screen coordinate intersects an application tile in the floating dock.
/// Returns `Some(index)` if a dock tile was clicked, or `None` otherwise.
pub fn hit_test_dock(screen_w: u32, screen_h: u32, pt: Point) -> Option<usize> {
    let dock_rect = get_dock_bounds(screen_w, screen_h);
    if !dock_rect.contains(pt) {
        return None;
    }

    let item_slot_w = 64i32;
    let start_x = dock_rect.x + 16;
    for (i, _) in DOCK_ITEMS.iter().enumerate() {
        let cur_x = start_x + (i as i32 * item_slot_w);
        let slot_rect = Rect::new(cur_x, dock_rect.y, item_slot_w as u32, dock_rect.height);
        if slot_rect.contains(pt) {
            return Some(i);
        }
    }

    None
}
