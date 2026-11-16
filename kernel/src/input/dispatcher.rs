//! Desktop Event Dispatcher and Window Manager Interaction Controller.
//!
//! Routes keyboard shortcuts, mouse movement, window activation, title bar dragging,
//! window chrome control clicks (Min/Max/Close), and application dock interactions.

use super::event::{ButtonAction, InputEvent, KeyAction, KeyCode, MouseButton};
use crate::desktop::dock::hit_test_dock;
use crate::desktop::shell::DesktopShell;
use crate::graphics::geometry::Point;
use crate::window::state::{WindowControl, WindowId, WindowState};
use crate::window::WindowManager;

/// Represents an ongoing window drag interaction.
#[derive(Debug, Clone, Copy)]
pub struct DragState {
    pub window_id: WindowId,
    pub grab_offset_x: i32,
    pub grab_offset_y: i32,
}

/// Coordinates desktop interaction by routing input events to windows, shell, and dock.
#[allow(dead_code)]
pub struct EventDispatcher {
    drag_state: Option<DragState>,
    screen_width: u32,
    screen_height: u32,
}

#[allow(dead_code)]
impl EventDispatcher {
    /// Creates a new EventDispatcher for the given screen dimensions.
    pub const fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            drag_state: None,
            screen_width,
            screen_height,
        }
    }

    /// Returns the currently dragged window ID, if any.
    #[inline(always)]
    pub fn dragged_window(&self) -> Option<WindowId> {
        self.drag_state.map(|d| d.window_id)
    }

    /// Cancels any active window drag operation.
    pub fn cancel_drag(&mut self) {
        self.drag_state = None;
    }

    /// Dispatches a single input event across desktop components.
    ///
    /// Returns `true` if a visual change occurred and the desktop requires redrawing.
    pub fn dispatch(
        &mut self,
        event: InputEvent,
        wm: &mut WindowManager,
        shell: &mut DesktopShell,
    ) -> bool {
        match event {
            InputEvent::Mouse(mouse) => self.handle_mouse(mouse, wm, shell),
            InputEvent::Key(key) => self.handle_keyboard(key, wm),
        }
    }

    /// Handles mouse events: cursor tracking, title bar dragging, clicks, and dock hits.
    fn handle_mouse(
        &mut self,
        mouse: super::event::MouseEvent,
        wm: &mut WindowManager,
        shell: &mut DesktopShell,
    ) -> bool {
        let cursor_pt = Point::new(mouse.x, mouse.y);
        shell.cursor_mut().set_position(cursor_pt);
        let redraw_needed = true;

        // 1. Update window dragging if active
        if let Some(drag) = self.drag_state {
            if mouse.buttons.left {
                let new_x = mouse.x - drag.grab_offset_x;
                let new_y = mouse.y - drag.grab_offset_y;
                wm.move_window(drag.window_id, new_x, new_y);
            } else {
                // Left button released: end drag
                self.drag_state = None;
            }
        }

        // 2. Handle button transition events
        if let Some((button, action)) = mouse.button_change {
            match (button, action) {
                (MouseButton::Left, ButtonAction::Pressed) => {
                    // Check dock hit first
                    if let Some(dock_idx) = hit_test_dock(self.screen_width, self.screen_height, cursor_pt) {
                        log::info!("[Desktop] Dock item clicked at index: {}", dock_idx);
                        return redraw_needed;
                    }

                    // Check window hit
                    if let Some(win_id) = wm.hit_test(cursor_pt) {
                        // Check if a window chrome control button was clicked
                        if let Some(ctrl) = wm.hit_test_controls(win_id, cursor_pt) {
                            match ctrl {
                                WindowControl::Close => {
                                    wm.close_window(win_id);
                                }
                                WindowControl::Maximize => {
                                    if let Some(w) = wm.get_window(win_id) {
                                        if w.state() == WindowState::Maximized {
                                            wm.restore_window(win_id);
                                        } else {
                                            wm.maximize_window(win_id);
                                        }
                                    }
                                }
                                WindowControl::Minimize => {
                                    wm.minimize_window(win_id);
                                }
                            }
                        } else if wm.hit_test_title_bar(win_id, cursor_pt) {
                            // Clicked in title bar (not on a button): initiate window drag
                            wm.bring_to_front(win_id);
                            if let Some(w) = wm.get_window(win_id) {
                                self.drag_state = Some(DragState {
                                    window_id: win_id,
                                    grab_offset_x: mouse.x - w.bounds().x,
                                    grab_offset_y: mouse.y - w.bounds().y,
                                });
                            }
                        } else {
                            // Clicked inside window body: focus and bring to front
                            wm.bring_to_front(win_id);
                        }
                    }
                }
                (MouseButton::Left, ButtonAction::Released) => {
                    self.drag_state = None;
                }
                _ => {}
            }
        }

        redraw_needed
    }

    /// Handles keyboard events: desktop navigation shortcuts and escape cancels.
    fn handle_keyboard(
        &mut self,
        key: super::event::KeyEvent,
        wm: &mut WindowManager,
    ) -> bool {
        if key.action != KeyAction::Pressed {
            return false;
        }

        // 1. Escape cancels any in-progress window drag
        if key.code == KeyCode::Escape && self.drag_state.is_some() {
            self.drag_state = None;
            return true;
        }

        // 2. Alt+Tab cycles through active windows in Z-order
        if key.modifiers.alt && key.code == KeyCode::Tab {
            wm.cycle_active_window();
            return true;
        }

        // 3. Super+M minimizes the currently active window
        if key.modifiers.super_key && key.code == KeyCode::M {
            if let Some(active_id) = wm.active_window_id() {
                wm.minimize_window(active_id);
                return true;
            }
        }

        false
    }
}
