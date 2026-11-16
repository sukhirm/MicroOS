//! Input Subsystem Event Definitions and Keycode Representations.
//!
//! Provides strongly-typed representations for keyboard scan codes, key actions,
//! modifier states, mouse coordinates, and mouse button transitions.

use crate::graphics::geometry::Point;

/// Logical keyboard keys recognized by the MicroOS keyboard driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Alphanumeric
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,

    // Controls & Whitespace
    Escape,
    Enter,
    Space,
    Backspace,
    Tab,
    CapsLock,

    // Modifiers
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftSuper,

    // Navigation & Editing
    Up,
    Down,
    Left,
    Right,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,

    // Punctuation & Symbols
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Grave,
    Comma,
    Period,
    Slash,

    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,

    // Unmapped / raw scancode
    Unknown(u8),
}

/// Action performed on a keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Pressed,
    Released,
}

/// Active keyboard modifier key states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers {
    pub shift: bool,
    pub caps_lock: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

/// A decoded keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub action: KeyAction,
    pub character: Option<char>,
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    /// Creates a new KeyEvent.
    pub const fn new(
        code: KeyCode,
        action: KeyAction,
        character: Option<char>,
        modifiers: KeyModifiers,
    ) -> Self {
        Self {
            code,
            action,
            character,
            modifiers,
        }
    }
}

/// Physical buttons on a standard three-button mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Action performed on a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    Pressed,
    Released,
}

/// State of all three mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

/// A decoded mouse movement or button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    /// Absolute horizontal position on screen (clamped).
    pub x: i32,
    /// Absolute vertical position on screen (clamped).
    pub y: i32,
    /// Relative horizontal movement delta.
    pub delta_x: i32,
    /// Relative vertical movement delta.
    pub delta_y: i32,
    /// Current state of all buttons.
    pub buttons: MouseButtons,
    /// Specific button state transition that occurred with this event, if any.
    pub button_change: Option<(MouseButton, ButtonAction)>,
}

#[allow(dead_code)]
impl MouseEvent {
    /// Returns the mouse position as a 2D Point.
    #[inline(always)]
    pub const fn position(&self) -> Point {
        Point::new(self.x, self.y)
    }
}

/// Unified input event published by input drivers and consumed by the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}
