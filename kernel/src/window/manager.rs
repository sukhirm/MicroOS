//! Central Window Manager and Z-Order Compositor.
//!
//! Coordinates window lifecycles, active focus selection, Z-order stacking,
//! title bar / control button hit-testing, and clip-aware multi-window rendering.

use alloc::vec::Vec;

use super::renderer::render_window;
use super::state::{WindowControl, WindowId, WindowState};
use super::window::Window;
use crate::graphics::geometry::Point;
use crate::graphics::surface::Surface;

/// Default height allocated to the top status bar.
pub const DEFAULT_STATUS_BAR_HEIGHT: u32 = 34;
/// Default bottom margin allocated to the application dock.
pub const DEFAULT_DOCK_HEIGHT_WITH_MARGIN: u32 = 78;

/// Manages desktop windows, Z-order stacking, focus, and presentation.
#[allow(dead_code)]
pub struct WindowManager {
    /// Collection of managed windows.
    windows: Vec<Window>,
    /// Z-order stack ordered back-to-front (index 0 = back-most, last = front-most/active).
    z_order: Vec<WindowId>,
    /// Sequential counter for allocating unique window IDs.
    next_id: u32,
    /// Target screen width in pixels.
    screen_width: u32,
    /// Target screen height in pixels.
    screen_height: u32,
    /// Top inset for maximized windows (status bar clearance).
    work_area_top: u32,
    /// Bottom inset for maximized windows (dock clearance).
    work_area_bottom: u32,
}

#[allow(dead_code)]
impl WindowManager {
    /// Creates a new WindowManager for the specified screen dimensions.
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            windows: Vec::new(),
            z_order: Vec::new(),
            next_id: 1,
            screen_width,
            screen_height,
            work_area_top: DEFAULT_STATUS_BAR_HEIGHT,
            work_area_bottom: DEFAULT_DOCK_HEIGHT_WITH_MARGIN,
        }
    }

    /// Sets the top and bottom work area insets (e.g. for status bar and dock).
    pub fn set_work_area(&mut self, top: u32, bottom: u32) {
        self.work_area_top = top;
        self.work_area_bottom = bottom;
    }

    /// Creates and registers a new window with default styling, placing it at the front of the Z-stack.
    pub fn create_window(
        &mut self,
        title: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> WindowId {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;

        let window = Window::new(id, title, x, y, width, height);
        self.add_window(window);
        id
    }

    /// Registers a pre-configured Window instance, bringing it to the front and activating it.
    pub fn add_window(&mut self, window: Window) -> WindowId {
        let id = window.id();
        // Update next_id if this window has a larger ID
        if window.id().as_u32() >= self.next_id {
            self.next_id = window.id().as_u32() + 1;
        }

        self.windows.push(window);
        self.z_order.push(id);
        self.set_active(id);
        id
    }

    /// Returns an immutable reference to the window with ID `id`.
    pub fn get_window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id() == id)
    }

    /// Returns a mutable reference to the window with ID `id`.
    pub fn get_window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id() == id)
    }

    /// Returns an immutable slice of all registered windows.
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Returns the current back-to-front Z-order list of window IDs.
    pub fn z_order(&self) -> &[WindowId] {
        &self.z_order
    }

    /// Returns the ID of the currently active/focused window, if any.
    pub fn active_window_id(&self) -> Option<WindowId> {
        self.z_order
            .iter()
            .rev()
            .copied()
            .find(|&id| self.get_window(id).map_or(false, |w| w.is_active()))
    }

    /// Focuses and activates the window with ID `id`, bringing it to the front of the Z-stack.
    ///
    /// Deactivates all other windows.
    pub fn set_active(&mut self, id: WindowId) {
        // Verify window exists and can be activated
        let exists = self.get_window(id).map_or(false, |w| {
            w.state() != WindowState::Closed && w.state() != WindowState::Minimized
        });
        if !exists {
            return;
        }

        // Deactivate all windows
        for w in &mut self.windows {
            w.set_active(false);
        }

        // Activate target window
        if let Some(w) = self.get_window_mut(id) {
            w.set_active(true);
        }

        // Move to the top (end) of the Z-order
        if let Some(pos) = self.z_order.iter().position(|&item| item == id) {
            self.z_order.remove(pos);
            self.z_order.push(id);
        }
    }

    /// Brings the specified window to the front of the Z-order and gives it active focus.
    pub fn bring_to_front(&mut self, id: WindowId) {
        self.set_active(id);
    }

    /// Closes the specified window, flags it as Closed, and updates the active window.
    pub fn close_window(&mut self, id: WindowId) -> bool {
        let was_active = if let Some(w) = self.get_window_mut(id) {
            let active = w.is_active();
            w.close();
            active
        } else {
            return false;
        };

        // Remove from Z-order
        if let Some(pos) = self.z_order.iter().position(|&item| item == id) {
            self.z_order.remove(pos);
        }

        // If the closed window was active, transfer focus to the next topmost visible window
        if was_active {
            self.activate_topmost();
        }

        true
    }

    /// Permanently removes all closed windows from the internal collections.
    pub fn remove_closed(&mut self) {
        self.windows.retain(|w| w.state() != WindowState::Closed);
        self.z_order
            .retain(|&id| self.windows.iter().any(|w| w.id() == id));
    }

    /// Minimizes the window, hiding it from the desktop, and transfers focus.
    pub fn minimize_window(&mut self, id: WindowId) {
        let was_active = if let Some(w) = self.get_window_mut(id) {
            let active = w.is_active();
            w.minimize();
            active
        } else {
            false
        };

        if was_active {
            self.activate_topmost();
        }
    }

    /// Maximizes the window across the usable workspace and brings it to the front.
    pub fn maximize_window(&mut self, id: WindowId) {
        let (w, h, top, bot) = (
            self.screen_width,
            self.screen_height,
            self.work_area_top,
            self.work_area_bottom,
        );
        if let Some(win) = self.get_window_mut(id) {
            win.maximize(w, h, top, bot);
        }
        self.set_active(id);
    }

    /// Restores the window from Minimized or Maximized to Normal dimensions and activates it.
    pub fn restore_window(&mut self, id: WindowId) {
        if let Some(win) = self.get_window_mut(id) {
            win.restore();
        }
        self.set_active(id);
    }

    /// Moves the window to `(new_x, new_y)`.
    pub fn move_window(&mut self, id: WindowId, new_x: i32, new_y: i32) {
        if let Some(win) = self.get_window_mut(id) {
            win.set_position(new_x, new_y);
        }
    }

    /// Resizes the window, strictly enforcing its configured minimum dimensions.
    pub fn resize_window(&mut self, id: WindowId, new_w: u32, new_h: u32) {
        if let Some(win) = self.get_window_mut(id) {
            win.set_size(new_w, new_h);
        }
    }

    /// Identifies the topmost visible, non-minimized window intersecting coordinate `pt`.
    ///
    /// Searches in reverse Z-order (from front-most to back-most).
    pub fn hit_test(&self, pt: Point) -> Option<WindowId> {
        for &id in self.z_order.iter().rev() {
            if let Some(win) = self.get_window(id) {
                if win.hit_test(pt) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Hit-tests the title bar of a specific window.
    pub fn hit_test_title_bar(&self, id: WindowId, pt: Point) -> bool {
        self.get_window(id).map_or(false, |w| w.hit_test_title_bar(pt))
    }

    /// Hit-tests the control buttons (Minimize, Maximize, Close) of a specific window.
    pub fn hit_test_controls(&self, id: WindowId, pt: Point) -> Option<WindowControl> {
        self.get_window(id).and_then(|w| w.hit_test_controls(pt))
    }

    /// Renders all visible windows in back-to-front Z-order onto `surface`.
    pub fn render(&self, surface: &mut Surface) {
        for &id in &self.z_order {
            if let Some(win) = self.get_window(id) {
                render_window(win, surface);
            }
        }
    }

    /// Activates the highest visible, non-minimized window in the Z-order.
    fn activate_topmost(&mut self) {
        let topmost_id = self
            .z_order
            .iter()
            .rev()
            .copied()
            .find(|&id| {
                self.get_window(id).map_or(false, |w| {
                    w.is_visible()
                        && w.state() != WindowState::Minimized
                        && w.state() != WindowState::Closed
                })
            });

        for w in &mut self.windows {
            w.set_active(false);
        }

        if let Some(top_id) = topmost_id {
            if let Some(w) = self.get_window_mut(top_id) {
                w.set_active(true);
            }
        }
    }
}
