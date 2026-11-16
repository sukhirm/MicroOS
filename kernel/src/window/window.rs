//! Window Primitive Structure, Geometry, and Hit-Testing Logic.
//!
//! Encapsulates the visual geometry, state machine, title bar bounds,
//! interactive control buttons, and client content rectangle for a single window.

use alloc::string::String;
use alloc::borrow::ToOwned;

use super::state::{WindowControl, WindowId, WindowState, WindowTheme};
use crate::graphics::geometry::{Point, Rect};
use crate::graphics::icon::Icon;
use crate::graphics::surface::Surface;

/// Default minimum window width in pixels to guarantee title and controls visibility.
pub const DEFAULT_MIN_WINDOW_WIDTH: u32 = 180;
/// Default minimum window height in pixels to fit title bar and client content.
pub const DEFAULT_MIN_WINDOW_HEIGHT: u32 = 100;

/// Function pointer type for rendering window client area content.
pub type ContentRenderer = fn(&Window, &mut Surface, Rect);

/// Represents an independent application or tool window on the desktop surface.
#[allow(dead_code)]
pub struct Window {
    /// Unique identifier for this window.
    id: WindowId,
    /// Window title displayed in the title bar.
    title: String,
    /// Optional application icon displayed at the left of the title bar.
    icon: Option<&'static Icon>,
    /// Current on-screen bounding rectangle (position and dimensions).
    rect: Rect,
    /// Saved bounding rectangle in Normal state (used when restoring from Maximized).
    saved_normal_rect: Rect,
    /// Minimum allowed width in pixels.
    min_width: u32,
    /// Minimum allowed height in pixels.
    min_height: u32,
    /// Current window lifecycle and presentation state.
    state: WindowState,
    /// Whether this window currently has keyboard and visual focus.
    is_active: bool,
    /// Whether this window is visible on the desktop surface.
    is_visible: bool,
    /// Visual styling theme for chrome, borders, and typography.
    theme: WindowTheme,
    /// Optional content renderer callback invoked during paint pass.
    content_renderer: Option<ContentRenderer>,
}

#[allow(dead_code)]
impl Window {
    /// Constructs a new Window with enforced minimum dimensions and Obsidian Azure theme.
    pub fn new(id: WindowId, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        let clamped_w = width.max(DEFAULT_MIN_WINDOW_WIDTH);
        let clamped_h = height.max(DEFAULT_MIN_WINDOW_HEIGHT);
        let rect = Rect::new(x, y, clamped_w, clamped_h);

        Self {
            id,
            title: title.to_owned(),
            icon: None,
            rect,
            saved_normal_rect: rect,
            min_width: DEFAULT_MIN_WINDOW_WIDTH,
            min_height: DEFAULT_MIN_WINDOW_HEIGHT,
            state: WindowState::Normal,
            is_active: false,
            is_visible: true,
            theme: WindowTheme::obsidian_azure(),
            content_renderer: None,
        }
    }

    /// Builder helper to attach an application icon.
    pub fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Builder helper to configure custom minimum dimensions.
    pub fn with_min_size(mut self, min_w: u32, min_h: u32) -> Self {
        self.min_width = min_w;
        self.min_height = min_h;
        if self.rect.width < min_w || self.rect.height < min_h {
            self.rect.width = self.rect.width.max(min_w);
            self.rect.height = self.rect.height.max(min_h);
            self.saved_normal_rect = self.rect;
        }
        self
    }

    /// Builder helper to attach a custom content renderer callback.
    pub fn with_content_renderer(mut self, renderer: ContentRenderer) -> Self {
        self.content_renderer = Some(renderer);
        self
    }

    /// Returns the unique window ID.
    #[inline(always)]
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Returns the window title.
    #[inline(always)]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the window bounding rectangle.
    #[inline(always)]
    pub fn bounds(&self) -> Rect {
        self.rect
    }

    /// Returns the minimum allowed width.
    #[inline(always)]
    pub fn min_width(&self) -> u32 {
        self.min_width
    }

    /// Returns the minimum allowed height.
    #[inline(always)]
    pub fn min_height(&self) -> u32 {
        self.min_height
    }

    /// Returns the current window state.
    #[inline(always)]
    pub fn state(&self) -> WindowState {
        self.state
    }

    /// Returns true if the window currently has active visual focus.
    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Returns true if the window is currently marked visible.
    #[inline(always)]
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    /// Returns a reference to the window visual styling theme.
    #[inline(always)]
    pub fn theme(&self) -> &WindowTheme {
        &self.theme
    }

    /// Returns the attached application icon, if any.
    #[inline(always)]
    pub fn icon(&self) -> Option<&'static Icon> {
        self.icon
    }

    /// Returns the attached client content renderer, if any.
    #[inline(always)]
    pub fn content_renderer(&self) -> Option<ContentRenderer> {
        self.content_renderer
    }

    /// Computes the bounding rectangle of the window title bar.
    pub fn title_bar_rect(&self) -> Rect {
        Rect::new(
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.theme.title_bar_height,
        )
    }

    /// Computes the client content area enclosed within borders and beneath the title bar.
    pub fn content_rect(&self) -> Rect {
        let content_y = self.rect.y + self.theme.title_bar_height as i32;
        let content_h = self
            .rect
            .height
            .saturating_sub(self.theme.title_bar_height + 1);
        let content_w = self.rect.width.saturating_sub(2);

        Rect::new(self.rect.x + 1, content_y, content_w, content_h)
    }

    /// Computes the bounding rectangle for a specific window chrome control button.
    pub fn control_button_rect(&self, control: WindowControl) -> Rect {
        let btn_size = self.theme.button_size;
        let spacing = self.theme.button_spacing;
        let btn_y = self.rect.y
            + ((self.theme.title_bar_height.saturating_sub(btn_size)) / 2) as i32;

        let right_margin = 8i32;
        let close_x = self.rect.right() - right_margin - btn_size as i32;

        match control {
            WindowControl::Close => Rect::new(close_x, btn_y, btn_size, btn_size),
            WindowControl::Maximize => {
                let max_x = close_x - (btn_size + spacing) as i32;
                Rect::new(max_x, btn_y, btn_size, btn_size)
            }
            WindowControl::Minimize => {
                let min_x = close_x - ((btn_size + spacing) * 2) as i32;
                Rect::new(min_x, btn_y, btn_size, btn_size)
            }
        }
    }

    /// Tests whether a screen coordinate falls inside this window's bounds.
    ///
    /// Returns false if the window is hidden, minimized, or closed.
    pub fn hit_test(&self, pt: Point) -> bool {
        if !self.is_visible || self.state == WindowState::Minimized || self.state == WindowState::Closed {
            return false;
        }
        self.rect.contains(pt)
    }

    /// Hit-tests window control buttons (Close, Maximize, Minimize).
    ///
    /// Returns `Some(control)` if the point intersects a button, or `None` otherwise.
    pub fn hit_test_controls(&self, pt: Point) -> Option<WindowControl> {
        if !self.is_visible || self.state == WindowState::Minimized || self.state == WindowState::Closed {
            return None;
        }

        if self.control_button_rect(WindowControl::Close).contains(pt) {
            return Some(WindowControl::Close);
        }
        if self.control_button_rect(WindowControl::Maximize).contains(pt) {
            return Some(WindowControl::Maximize);
        }
        if self.control_button_rect(WindowControl::Minimize).contains(pt) {
            return Some(WindowControl::Minimize);
        }

        None
    }

    /// Hit-tests the window title bar for dragging or window activation.
    ///
    /// Returns true if `pt` is within the title bar AND NOT inside any control button.
    pub fn hit_test_title_bar(&self, pt: Point) -> bool {
        if !self.is_visible || self.state == WindowState::Minimized || self.state == WindowState::Closed {
            return false;
        }

        self.title_bar_rect().contains(pt) && self.hit_test_controls(pt).is_none()
    }

    /// Tests whether a screen coordinate falls within the client content area.
    pub fn hit_test_content(&self, pt: Point) -> bool {
        if !self.is_visible || self.state == WindowState::Minimized || self.state == WindowState::Closed {
            return false;
        }

        self.content_rect().contains(pt)
    }

    /// Updates the window on-screen position.
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.rect.x = x;
        self.rect.y = y;
        if self.state == WindowState::Normal {
            self.saved_normal_rect.x = x;
            self.saved_normal_rect.y = y;
        }
    }

    /// Resizes the window, strictly enforcing minimum width and height limits.
    pub fn set_size(&mut self, width: u32, height: u32) {
        let clamped_w = width.max(self.min_width);
        let clamped_h = height.max(self.min_height);
        self.rect.width = clamped_w;
        self.rect.height = clamped_h;
        if self.state == WindowState::Normal {
            self.saved_normal_rect.width = clamped_w;
            self.saved_normal_rect.height = clamped_h;
        }
    }

    /// Sets the active visual and focus state of this window.
    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }

    /// Sets the visibility state of this window.
    pub fn set_visible(&mut self, visible: bool) {
        self.is_visible = visible;
    }

    /// Sets the window title.
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_owned();
    }

    /// Minimizes the window, hiding it from the desktop view.
    pub fn minimize(&mut self) {
        self.state = WindowState::Minimized;
        self.is_active = false;
    }

    /// Maximizes the window across the usable desktop work area.
    pub fn maximize(&mut self, screen_w: u32, screen_h: u32, top_inset: u32, bottom_inset: u32) {
        if self.state == WindowState::Normal {
            self.saved_normal_rect = self.rect;
        }
        let usable_h = screen_h.saturating_sub(top_inset + bottom_inset);
        self.rect = Rect::new(0, top_inset as i32, screen_w, usable_h);
        self.state = WindowState::Maximized;
    }

    /// Restores the window from Minimized or Maximized back to Normal state.
    pub fn restore(&mut self) {
        self.rect = self.saved_normal_rect;
        self.state = WindowState::Normal;
    }

    /// Closes the window, flagging it for removal.
    pub fn close(&mut self) {
        self.state = WindowState::Closed;
        self.is_visible = false;
        self.is_active = false;
    }
}
