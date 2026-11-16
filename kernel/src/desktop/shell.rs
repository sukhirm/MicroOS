//! Desktop Environment Shell Coordinator.
//!
//! Integrates the wallpaper, window manager, top status bar, floating application dock,
//! and software mouse cursor into a cohesive graphical desktop environment.

use super::cursor::Cursor;
use super::dock::render_dock;
use super::status_bar::{render_status_bar, StatusMetrics};
use super::wallpaper::render_wallpaper;
use crate::graphics::surface::Surface;
use crate::window::WindowManager;

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

    /// Renders the desktop shell pipeline without windows:
    /// 1. Wallpaper background
    /// 2. Top status bar
    /// 3. Bottom floating dock
    /// 4. Top-layer software cursor
    pub fn render(&mut self, surface: &mut Surface) {
        self.render_with_wm(surface, None);
    }

    /// Renders the complete multi-layer desktop environment including managed windows:
    /// 1. Desktop Wallpaper
    /// 2. Managed Desktop Windows (in back-to-front Z-order)
    /// 3. Top Status Bar (stays atop windows)
    /// 4. Floating Application Dock (stays atop windows)
    /// 5. Software Mouse Cursor (drawn last, atop all desktop layers)
    pub fn render_with_wm(&mut self, surface: &mut Surface, wm: Option<&WindowManager>) {
        // 1. Desktop Wallpaper
        render_wallpaper(surface);

        // 2. Desktop Windows (managed in Z-order)
        if let Some(wm) = wm {
            wm.render(surface);
        }

        // 3. Top Status Bar
        render_status_bar(surface, &self.metrics);

        // 4. Floating Application Dock
        render_dock(surface);

        // 5. Software Mouse Cursor (drawn last, atop all desktop layers)
        self.cursor.render(surface);
    }
}

/// Convenience function rendering the desktop environment without windows onto `surface`.
pub fn render_desktop(surface: &mut Surface) {
    let mut shell = DesktopShell::new(surface.width() as u32, surface.height() as u32);
    shell.render(surface);
}

/// Convenience function rendering the complete desktop environment with windows onto `surface`.
pub fn render_desktop_with_wm(surface: &mut Surface, wm: &WindowManager) {
    let mut shell = DesktopShell::new(surface.width() as u32, surface.height() as u32);
    shell.render_with_wm(surface, Some(wm));
}
