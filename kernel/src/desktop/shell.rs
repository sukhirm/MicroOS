//! Desktop Environment Shell Coordinator.
//!
//! Integrates the wallpaper, top status bar, floating application dock,
//! and software mouse cursor into a cohesive graphical desktop environment.

use super::cursor::Cursor;
use super::dock::render_dock;
use super::status_bar::{render_status_bar, StatusMetrics};
use super::wallpaper::render_wallpaper;
use crate::graphics::surface::Surface;

/// Primary Desktop Environment Shell.
pub struct DesktopShell {
    metrics: StatusMetrics,
    cursor: Cursor,
}

impl DesktopShell {
    /// Constructs a new DesktopShell with the cursor positioned at the screen center.
    pub fn new(screen_w: u32, screen_h: u32) -> Self {
        let center_x = (screen_w as i32) / 2;
        let center_y = (screen_h as i32) / 2;

        Self {
            metrics: StatusMetrics::default(),
            cursor: Cursor::new(center_x, center_y),
        }
    }

    /// Returns a mutable reference to the mouse cursor.
    #[allow(dead_code)]
    pub fn cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursor
    }

    /// Returns an immutable reference to the mouse cursor.
    #[allow(dead_code)]
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Renders the complete desktop shell pipeline onto `surface`:
    /// 1. Wallpaper background
    /// 2. Top status bar
    /// 3. Bottom floating dock
    /// 4. Top-layer software cursor
    pub fn render(&mut self, surface: &mut Surface) {
        // 1. Desktop Wallpaper
        render_wallpaper(surface);

        // 2. Top Status Bar
        render_status_bar(surface, &self.metrics);

        // 3. Floating Application Dock
        render_dock(surface);

        // 4. Software Mouse Cursor (drawn last, atop all desktop layers)
        self.cursor.render(surface);
    }
}

/// Convenience function rendering the complete desktop environment onto `surface`.
pub fn render_desktop(surface: &mut Surface) {
    let mut shell = DesktopShell::new(surface.width() as u32, surface.height() as u32);
    shell.render(surface);
}
