//! Window Chrome and Content Renderer.
//!
//! Renders visually polished windows in the Obsidian Azure design language,
//! featuring rounded corners, distinct active/inactive title bar elevation,
//! crisp vector control buttons, application icons, and clipped client content areas.

use super::state::{WindowControl, WindowTheme};
use super::window::Window;
use crate::graphics::color::Color;
use crate::graphics::font::draw_string;
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::draw_icon;
use crate::graphics::surface::Surface;

/// Renders a single window onto `surface`.
///
/// Skips rendering if the window is hidden, minimized, or closed.
pub fn render_window(window: &Window, surface: &mut Surface) {
    if !window.is_visible() || window.state() == super::state::WindowState::Minimized || window.state() == super::state::WindowState::Closed {
        return;
    }

    let saved_clip = surface.clip_rect();
    let bounds = window.bounds();

    // 1. Clip rendering to the intersection of the current clip rect and window bounds
    if let Some(clipped_window) = saved_clip.intersect(&bounds) {
        surface.set_clip_rect(clipped_window);
    } else {
        return; // Window is completely outside surface/clipping bounds
    }

    let theme = window.theme();
    let is_active = window.is_active();

    // 2. Select visual attributes based on active/inactive focus
    let (title_bg, title_fg, border_color) = if is_active {
        (theme.title_active_bg, theme.title_active_fg, theme.border_active)
    } else {
        (theme.title_inactive_bg, theme.title_inactive_fg, theme.border_inactive)
    };

    // 3. Fill the window body background with rounded corners
    surface.fill_rounded_rect(bounds, theme.corner_radius, theme.bg_color);

    // 4. Fill the title bar area with matching top-rounded corners
    render_title_bar_background(surface, bounds, theme, title_bg);

    // 5. Title bar bottom divider line
    let title_rect = window.title_bar_rect();
    surface.draw_hline(
        title_rect.x,
        title_rect.bottom() - 1,
        title_rect.width,
        Color::BORDER_SLATE,
    );

    // 6. Draw application icon (if configured)
    let icon_offset = if let Some(icon) = window.icon() {
        let icon_x = title_rect.x + 8;
        let icon_y = title_rect.y + ((theme.title_bar_height.saturating_sub(icon.height)) / 2) as i32;
        let icon_tint = if is_active { None } else { Some(Color::TEXT_MUTED) };
        draw_icon(surface, icon_x, icon_y, icon, icon_tint);
        icon.width as i32 + 8
    } else {
        0
    };

    // 7. Draw window title string
    let title_x = title_rect.x + 8 + icon_offset;
    let title_y = title_rect.y + ((theme.title_bar_height.saturating_sub(8)) / 2) as i32;
    // Clip title string so it does not collide with control buttons
    let min_btn_rect = window.control_button_rect(WindowControl::Minimize);
    let max_title_w = (min_btn_rect.x - title_x - 8).max(0) as u32;
    let title_clip = Rect::new(title_x, title_rect.y, max_title_w, theme.title_bar_height);

    if let Some(str_clip) = surface.clip_rect().intersect(&title_clip) {
        surface.set_clip_rect(str_clip);
        draw_string(surface, title_x, title_y, window.title(), title_fg, None, 1);
        surface.set_clip_rect(bounds);
    }

    // 8. Draw window chrome control buttons (Minimize, Maximize, Close)
    render_control_buttons(surface, window, is_active);

    // 9. Draw outer rounded border stroke (highlight cyan when active, slate when inactive)
    surface.stroke_rounded_rect(bounds, theme.corner_radius, 1, border_color);

    // 10. Render client content area
    let content_rect = window.content_rect();
    if let Some(renderer) = window.content_renderer() {
        if let Some(clipped_content) = bounds.intersect(&content_rect).and_then(|r| saved_clip.intersect(&r)) {
            surface.set_clip_rect(clipped_content);
            renderer(window, surface, content_rect);
        }
    }

    // 11. Restore original surface clipping rectangle
    surface.set_clip_rect(saved_clip);
}

/// Fills the title bar with rounded top corners matching the outer window radius.
fn render_title_bar_background(
    surface: &mut Surface,
    bounds: Rect,
    theme: &WindowTheme,
    title_bg: Color,
) {
    let r = theme.corner_radius as i32;
    let r_sq = r * r;
    let title_rect = Rect::new(bounds.x, bounds.y, bounds.width, theme.title_bar_height);

    for y in title_rect.y..title_rect.bottom() {
        let dy = y - title_rect.y;
        for x in title_rect.x..title_rect.right() {
            if dy < r {
                let dx_left = x - title_rect.x;
                if dx_left < r && (r - 1 - dx_left) * (r - 1 - dx_left) + (r - 1 - dy) * (r - 1 - dy) > r_sq {
                    continue; // Skip outside top-left curve
                }
                let dx_right = title_rect.right() - 1 - x;
                if dx_right < r && (r - 1 - dx_right) * (r - 1 - dx_right) + (r - 1 - dy) * (r - 1 - dy) > r_sq {
                    continue; // Skip outside top-right curve
                }
            }
            surface.set_pixel(x, y, title_bg);
        }
    }
}

/// Renders the three window chrome control buttons.
fn render_control_buttons(surface: &mut Surface, window: &Window, is_active: bool) {
    // Colors for buttons
    let (close_color, max_color, min_color) = if is_active {
        (
            Color::new(0xf8, 0x71, 0x71), // Coral / danger red
            Color::ACCENT_CYAN,           // Obsidian cyan
            Color::ACCENT_AZURE,          // Azure blue
        )
    } else {
        let muted = Color::new(0x47, 0x55, 0x69);
        (muted, muted, muted)
    };

    // 1. Close button (x cross)
    let close_rect = window.control_button_rect(WindowControl::Close);
    let cx = close_rect.x + 3;
    let cy = close_rect.y + 3;
    surface.draw_line(Point::new(cx, cy), Point::new(cx + 7, cy + 7), close_color);
    surface.draw_line(Point::new(cx, cy + 7), Point::new(cx + 7, cy), close_color);

    // 2. Maximize button (square outline)
    let max_rect = window.control_button_rect(WindowControl::Maximize);
    let mx = max_rect.x + 3;
    let my = max_rect.y + 3;
    surface.stroke_rect(Rect::new(mx, my, 8, 8), 1, max_color);

    // 3. Minimize button (horizontal line)
    let min_rect = window.control_button_rect(WindowControl::Minimize);
    let min_x = min_rect.x + 3;
    let min_y = min_rect.y + 7;
    surface.draw_hline(min_x, min_y, 8, min_color);
}
