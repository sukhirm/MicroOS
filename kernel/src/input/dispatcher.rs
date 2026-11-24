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
            InputEvent::Key(key) => self.handle_keyboard(key, wm, shell),
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
                        if dock_idx == 0 {
                            // "Files" application icon clicked on Dock
                            crate::apps::files::open_or_focus_files_window(wm);
                            return true;
                        } else if dock_idx == 1 {
                            // "Settings" application icon clicked on Dock
                            crate::apps::settings::open_or_focus_settings_window(wm);
                            return true;
                        } else if dock_idx == 2 {
                            // "SysMon" application icon clicked on Dock
                            crate::apps::sysmon::open_or_focus_sysmon_window(wm);
                            return true;
                        } else if dock_idx == 3 {
                            // "Network" application icon clicked on Dock
                            crate::apps::network::open_or_focus_network_window(wm);
                            return true;
                        } else if dock_idx == 4 {
                            // "Terminal" application icon clicked on Dock
                            crate::apps::terminal::open_or_focus_terminal_window(wm);
                            return true;
                        } else if dock_idx == 5 {
                            // "Store" application icon clicked on Dock
                            crate::apps::store::open_or_focus_store_window(wm);
                            return true;
                        } else if dock_idx == 6 {
                            // "Deadlock" application icon clicked on Dock
                            crate::apps::deadlock::open_or_focus_deadlock_window(wm);
                            return true;
                        }
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
                            if let Some(w) = wm.get_window(win_id) {
                                if w.title() == "Files" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::files::handle_click(cursor_pt, content_rect);
                                    }
                                } else if w.title() == "Settings" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::settings::handle_click(cursor_pt, content_rect);
                                    }
                                } else if w.title() == "System Monitor" || w.title() == "SysMon" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::sysmon::handle_click(cursor_pt, content_rect);
                                    }
                                } else if w.title() == "Network" || w.title() == "Network Manager" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::network::handle_click(cursor_pt, content_rect);
                                    }
                                } else if w.title() == "Terminal" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::terminal::handle_click(cursor_pt, content_rect);
                                    }
                                } else if w.title() == "Software Store" || w.title() == "Store" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::store::handle_click(cursor_pt, content_rect);
                                    }
                                } else if w.title() == "Deadlock Research Lab — R14" || w.title() == "Deadlock Lab" {
                                    let content_rect = w.content_rect();
                                    if content_rect.contains(cursor_pt) {
                                        crate::apps::deadlock::handle_click(cursor_pt, content_rect);
                                    }
                                }
                            }
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

    /// Handles keyboard events: desktop navigation shortcuts and pointer emulation.
    fn handle_keyboard(
        &mut self,
        key: super::event::KeyEvent,
        wm: &mut WindowManager,
        shell: &mut DesktopShell,
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

        // 3. Super or Alt + Arrow Keys / Enter: Pointer & Touchpad Navigation
        let is_pointer_nav = key.modifiers.super_key || (key.modifiers.alt && key.code != KeyCode::Tab);
        if is_pointer_nav {
            match key.code {
                KeyCode::Up => {
                    let mut pos = shell.cursor().position();
                    pos.y = (pos.y - 24).max(0);
                    shell.cursor_mut().set_position(pos);
                    return true;
                }
                KeyCode::Down => {
                    let mut pos = shell.cursor().position();
                    pos.y = (pos.y + 24).min(self.screen_height as i32 - 1);
                    shell.cursor_mut().set_position(pos);
                    return true;
                }
                KeyCode::Left => {
                    let mut pos = shell.cursor().position();
                    pos.x = (pos.x - 24).max(0);
                    shell.cursor_mut().set_position(pos);
                    return true;
                }
                KeyCode::Right => {
                    let mut pos = shell.cursor().position();
                    pos.x = (pos.x + 24).min(self.screen_width as i32 - 1);
                    shell.cursor_mut().set_position(pos);
                    return true;
                }
                KeyCode::Enter | KeyCode::Space => {
                    // Synthesize Left Click at current mouse cursor position
                    let pos = shell.cursor().position();
                    let press_ev = super::event::MouseEvent {
                        x: pos.x,
                        y: pos.y,
                        delta_x: 0,
                        delta_y: 0,
                        buttons: super::event::MouseButtons { left: true, right: false, middle: false },
                        button_change: Some((super::event::MouseButton::Left, super::event::ButtonAction::Pressed)),
                    };
                    let red1 = self.handle_mouse(press_ev, wm, shell);
                    let release_ev = super::event::MouseEvent {
                        x: pos.x,
                        y: pos.y,
                        delta_x: 0,
                        delta_y: 0,
                        buttons: super::event::MouseButtons { left: false, right: false, middle: false },
                        button_change: Some((super::event::MouseButton::Left, super::event::ButtonAction::Released)),
                    };
                    let red2 = self.handle_mouse(release_ev, wm, shell);
                    return red1 | red2;
                }
                KeyCode::M => {
                    if let Some(active_id) = wm.active_window_id() {
                        wm.minimize_window(active_id);
                        return true;
                    }
                }
                _ => {}
            }
        }

        // 4. Forward keyboard navigation to active window
        if let Some(active_id) = wm.active_window_id() {
            if let Some(w) = wm.get_window(active_id) {
                if w.title() == "Files" {
                    if crate::apps::files::handle_key(key) {
                        return true;
                    }
                } else if w.title() == "Settings" {
                    if crate::apps::settings::handle_key(key) {
                        return true;
                    }
                } else if w.title() == "System Monitor" || w.title() == "SysMon" {
                    if crate::apps::sysmon::handle_key(key) {
                        return true;
                    }
                } else if w.title() == "Network" || w.title() == "Network Manager" {
                    if crate::apps::network::handle_key(key) {
                        return true;
                    }
                } else if w.title() == "Terminal" {
                    if crate::apps::terminal::handle_key(key) {
                        return true;
                    }
                } else if w.title() == "Software Store" || w.title() == "Store" {
                    if crate::apps::store::handle_key(key) {
                        return true;
                    }
                } else if w.title() == "Deadlock Research Lab — R14" || w.title() == "Deadlock Lab" {
                    if crate::apps::deadlock::handle_key(key) {
                        return true;
                    }
                }
            }
        }

        false
    }
}
