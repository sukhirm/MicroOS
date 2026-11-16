//! Window State, Identifiers, Controls, and Visual Styling Themes.
//!
//! Provides the core type definitions for window states, unique IDs,
//! interactive control buttons, and the Obsidian Azure design system theme.

use crate::graphics::color::Color;

/// Unique identifier for a managed window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u32);

impl WindowId {
    /// Creates a new WindowId.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw integer value of this window ID.
    #[inline(always)]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Lifecycle and presentation state of a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    /// Normal window occupying user-specified dimensions.
    Normal,
    /// Minimized window hidden from the desktop surface.
    Minimized,
    /// Maximized window expanded across the usable desktop workspace.
    Maximized,
    /// Closed window flagged for destruction and removal.
    Closed,
}

/// Interactive window chrome control buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControl {
    /// Minimize button (collapses window to dock/taskbar).
    Minimize,
    /// Maximize / Restore toggle button.
    Maximize,
    /// Close button (terminates window).
    Close,
}

/// Visual theme and styling parameters for window chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowTheme {
    /// Window content body background fill color.
    pub bg_color: Color,
    /// Title bar background fill when the window is focused/active.
    pub title_active_bg: Color,
    /// Title bar background fill when the window is unfocused/inactive.
    pub title_inactive_bg: Color,
    /// Title bar text and icon color when focused.
    pub title_active_fg: Color,
    /// Title bar text and icon color when unfocused.
    pub title_inactive_fg: Color,
    /// 1px outer border color when focused.
    pub border_active: Color,
    /// 1px outer border color when unfocused.
    pub border_inactive: Color,
    /// Outer window corner rounding radius in pixels.
    pub corner_radius: u32,
    /// Title bar height in pixels.
    pub title_bar_height: u32,
    /// Control button size (width and height) in pixels.
    pub button_size: u32,
    /// Padding between adjacent control buttons in pixels.
    pub button_spacing: u32,
}

impl WindowTheme {
    /// Constructs the standard Obsidian Azure design system theme.
    pub const fn obsidian_azure() -> Self {
        Self {
            bg_color: Color::new(0x0a, 0x0e, 0x17),            // OBSIDIAN_NAVY
            title_active_bg: Color::new(0x1a, 0x24, 0x3b),     // Active slate highlight
            title_inactive_bg: Color::new(0x11, 0x17, 0x24),   // Inactive subdued slate
            title_active_fg: Color::WHITE,
            title_inactive_fg: Color::TEXT_MUTED,
            border_active: Color::ACCENT_CYAN,
            border_inactive: Color::BORDER_SLATE,
            corner_radius: 8,
            title_bar_height: 28,
            button_size: 14,
            button_spacing: 6,
        }
    }
}

impl Default for WindowTheme {
    fn default() -> Self {
        Self::obsidian_azure()
    }
}
