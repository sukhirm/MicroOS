//! PS/2 Keyboard Driver and Set 1 Scancode Decoder.
//!
//! Decodes single-byte and extended 0xE0 multi-byte Set 1 scancodes into strongly-typed
//! `KeyEvent` structures, tracking Shift, Caps Lock, Ctrl, and Alt modifier states,
//! and mapping printable keys to characters.

use super::event::{KeyAction, KeyCode, KeyEvent, KeyModifiers};
use crate::sync::mutex::SpinMutex;

/// Internal state machine for the keyboard decoder.
pub struct KeyboardDecoder {
    extended: bool,
    modifiers: KeyModifiers,
}

impl KeyboardDecoder {
    /// Creates a new KeyboardDecoder with all modifiers cleared.
    pub const fn new() -> Self {
        Self {
            extended: false,
            modifiers: KeyModifiers {
                shift: false,
                caps_lock: false,
                ctrl: false,
                alt: false,
                super_key: false,
            },
        }
    }

    /// Returns the current active keyboard modifier states.
    #[inline(always)]
    pub fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }

    /// Decodes a raw scancode byte.
    ///
    /// Returns `Some(KeyEvent)` when a complete key event is assembled, or `None` if
    /// the byte was an escape prefix (`0xE0`) or unmapped.
    pub fn process_byte(&mut self, scancode: u8) -> Option<KeyEvent> {
        // 1. Extended prefix check
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }

        let is_extended = self.extended;
        self.extended = false; // Reset extended prefix

        // 2. Determine Press vs Release (Bit 7 is set on Key Release in Set 1)
        let is_release = (scancode & 0x80) != 0;
        let base_code = scancode & 0x7F;
        let action = if is_release {
            KeyAction::Released
        } else {
            KeyAction::Pressed
        };

        // 3. Translate base scancode to KeyCode
        let key_code = if is_extended {
            match base_code {
                0x48 => KeyCode::Up,
                0x50 => KeyCode::Down,
                0x4B => KeyCode::Left,
                0x4D => KeyCode::Right,
                0x53 => KeyCode::Delete,
                0x47 => KeyCode::Home,
                0x4F => KeyCode::End,
                0x49 => KeyCode::PageUp,
                0x51 => KeyCode::PageDown,
                0x1D => KeyCode::RightCtrl,
                0x38 => KeyCode::RightAlt,
                0x5B => KeyCode::LeftSuper,
                other => KeyCode::Unknown(other | 0x80),
            }
        } else {
            match base_code {
                0x01 => KeyCode::Escape,
                0x02 => KeyCode::Num1,
                0x03 => KeyCode::Num2,
                0x04 => KeyCode::Num3,
                0x05 => KeyCode::Num4,
                0x06 => KeyCode::Num5,
                0x07 => KeyCode::Num6,
                0x08 => KeyCode::Num7,
                0x09 => KeyCode::Num8,
                0x0A => KeyCode::Num9,
                0x0B => KeyCode::Num0,
                0x0C => KeyCode::Minus,
                0x0D => KeyCode::Equals,
                0x0E => KeyCode::Backspace,
                0x0F => KeyCode::Tab,

                0x10 => KeyCode::Q,
                0x11 => KeyCode::W,
                0x12 => KeyCode::E,
                0x13 => KeyCode::R,
                0x14 => KeyCode::T,
                0x15 => KeyCode::Y,
                0x16 => KeyCode::U,
                0x17 => KeyCode::I,
                0x18 => KeyCode::O,
                0x19 => KeyCode::P,
                0x1A => KeyCode::LeftBracket,
                0x1B => KeyCode::RightBracket,
                0x1C => KeyCode::Enter,
                0x1D => KeyCode::LeftCtrl,

                0x1E => KeyCode::A,
                0x1F => KeyCode::S,
                0x20 => KeyCode::D,
                0x21 => KeyCode::F,
                0x22 => KeyCode::G,
                0x23 => KeyCode::H,
                0x24 => KeyCode::J,
                0x25 => KeyCode::K,
                0x26 => KeyCode::L,
                0x27 => KeyCode::Semicolon,
                0x28 => KeyCode::Quote,
                0x29 => KeyCode::Grave,
                0x2A => KeyCode::LeftShift,
                0x2B => KeyCode::Backslash,

                0x2C => KeyCode::Z,
                0x2D => KeyCode::X,
                0x2E => KeyCode::C,
                0x2F => KeyCode::V,
                0x30 => KeyCode::B,
                0x31 => KeyCode::N,
                0x32 => KeyCode::M,
                0x33 => KeyCode::Comma,
                0x34 => KeyCode::Period,
                0x35 => KeyCode::Slash,
                0x36 => KeyCode::RightShift,
                0x38 => KeyCode::LeftAlt,
                0x39 => KeyCode::Space,
                0x3A => KeyCode::CapsLock,

                0x3B => KeyCode::F1,
                0x3C => KeyCode::F2,
                0x3D => KeyCode::F3,
                0x3E => KeyCode::F4,
                0x3F => KeyCode::F5,
                0x40 => KeyCode::F6,
                0x41 => KeyCode::F7,
                0x42 => KeyCode::F8,
                0x43 => KeyCode::F9,
                0x44 => KeyCode::F10,
                0x57 => KeyCode::F11,
                0x58 => KeyCode::F12,

                other => KeyCode::Unknown(other),
            }
        };

        // 4. Update modifier states
        let is_pressed = action == KeyAction::Pressed;
        match key_code {
            KeyCode::LeftShift | KeyCode::RightShift => {
                self.modifiers.shift = is_pressed;
            }
            KeyCode::LeftCtrl | KeyCode::RightCtrl => {
                self.modifiers.ctrl = is_pressed;
            }
            KeyCode::LeftAlt | KeyCode::RightAlt => {
                self.modifiers.alt = is_pressed;
            }
            KeyCode::LeftSuper => {
                self.modifiers.super_key = is_pressed;
            }
            KeyCode::CapsLock => {
                if is_pressed {
                    self.modifiers.caps_lock = !self.modifiers.caps_lock;
                }
            }
            _ => {}
        }

        // 5. Translate key to printable character
        let character = if is_pressed {
            self.translate_character(key_code)
        } else {
            None
        };

        Some(KeyEvent::new(key_code, action, character, self.modifiers))
    }

    /// Maps a KeyCode to an ASCII character based on active Shift and Caps Lock states.
    fn translate_character(&self, code: KeyCode) -> Option<char> {
        let shift = self.modifiers.shift;
        let upper = shift ^ self.modifiers.caps_lock;

        match code {
            KeyCode::A => Some(if upper { 'A' } else { 'a' }),
            KeyCode::B => Some(if upper { 'B' } else { 'b' }),
            KeyCode::C => Some(if upper { 'C' } else { 'c' }),
            KeyCode::D => Some(if upper { 'D' } else { 'd' }),
            KeyCode::E => Some(if upper { 'E' } else { 'e' }),
            KeyCode::F => Some(if upper { 'F' } else { 'f' }),
            KeyCode::G => Some(if upper { 'G' } else { 'g' }),
            KeyCode::H => Some(if upper { 'H' } else { 'h' }),
            KeyCode::I => Some(if upper { 'I' } else { 'i' }),
            KeyCode::J => Some(if upper { 'J' } else { 'j' }),
            KeyCode::K => Some(if upper { 'K' } else { 'k' }),
            KeyCode::L => Some(if upper { 'L' } else { 'l' }),
            KeyCode::M => Some(if upper { 'M' } else { 'm' }),
            KeyCode::N => Some(if upper { 'N' } else { 'n' }),
            KeyCode::O => Some(if upper { 'O' } else { 'o' }),
            KeyCode::P => Some(if upper { 'P' } else { 'p' }),
            KeyCode::Q => Some(if upper { 'Q' } else { 'q' }),
            KeyCode::R => Some(if upper { 'R' } else { 'r' }),
            KeyCode::S => Some(if upper { 'S' } else { 's' }),
            KeyCode::T => Some(if upper { 'T' } else { 't' }),
            KeyCode::U => Some(if upper { 'U' } else { 'u' }),
            KeyCode::V => Some(if upper { 'V' } else { 'v' }),
            KeyCode::W => Some(if upper { 'W' } else { 'w' }),
            KeyCode::X => Some(if upper { 'X' } else { 'x' }),
            KeyCode::Y => Some(if upper { 'Y' } else { 'y' }),
            KeyCode::Z => Some(if upper { 'Z' } else { 'z' }),

            KeyCode::Num1 => Some(if shift { '!' } else { '1' }),
            KeyCode::Num2 => Some(if shift { '@' } else { '2' }),
            KeyCode::Num3 => Some(if shift { '#' } else { '3' }),
            KeyCode::Num4 => Some(if shift { '$' } else { '4' }),
            KeyCode::Num5 => Some(if shift { '%' } else { '5' }),
            KeyCode::Num6 => Some(if shift { '^' } else { '6' }),
            KeyCode::Num7 => Some(if shift { '&' } else { '7' }),
            KeyCode::Num8 => Some(if shift { '*' } else { '8' }),
            KeyCode::Num9 => Some(if shift { '(' } else { '9' }),
            KeyCode::Num0 => Some(if shift { ')' } else { '0' }),

            KeyCode::Space => Some(' '),
            KeyCode::Enter => Some('\n'),
            KeyCode::Tab => Some('\t'),
            KeyCode::Backspace => Some('\x08'),

            KeyCode::Minus => Some(if shift { '_' } else { '-' }),
            KeyCode::Equals => Some(if shift { '+' } else { '=' }),
            KeyCode::LeftBracket => Some(if shift { '{' } else { '[' }),
            KeyCode::RightBracket => Some(if shift { '}' } else { ']' }),
            KeyCode::Backslash => Some(if shift { '|' } else { '\\' }),
            KeyCode::Semicolon => Some(if shift { ':' } else { ';' }),
            KeyCode::Quote => Some(if shift { '"' } else { '\'' }),
            KeyCode::Grave => Some(if shift { '~' } else { '`' }),
            KeyCode::Comma => Some(if shift { '<' } else { ',' }),
            KeyCode::Period => Some(if shift { '>' } else { '.' }),
            KeyCode::Slash => Some(if shift { '?' } else { '/' }),

            _ => None,
        }
    }
}

/// Global system keyboard decoder singleton.
static GLOBAL_KEYBOARD_DECODER: SpinMutex<KeyboardDecoder> =
    SpinMutex::new(KeyboardDecoder::new());

/// Processes an incoming scancode from IRQ1 and queues the resulting event, if any.
pub fn handle_scancode(scancode: u8) {
    if let Some(event) = GLOBAL_KEYBOARD_DECODER.lock().process_byte(scancode) {
        super::queue::push_event(super::event::InputEvent::Key(event));
    }
}
