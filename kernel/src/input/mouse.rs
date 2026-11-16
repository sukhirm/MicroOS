//! PS/2 Mouse Driver and 3-Byte Packet Decoder.
//!
//! Synchronizes packet boundaries using the status byte bit 3 check, decodes signed X/Y
//! deltas with Y-axis inversion, handles overflow conditions, tracks button state transitions,
//! and clamps cursor coordinates to the display boundaries.

use super::event::{ButtonAction, MouseButton, MouseButtons, MouseEvent};
use crate::sync::mutex::SpinMutex;

/// Internal state machine for the PS/2 mouse packet decoder.
pub struct MouseDecoder {
    packet: [u8; 3],
    packet_index: usize,
    x: i32,
    y: i32,
    screen_width: u32,
    screen_height: u32,
    buttons: MouseButtons,
}

#[allow(dead_code)]
impl MouseDecoder {
    /// Creates a new MouseDecoder initialized at screen center.
    pub const fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            packet: [0; 3],
            packet_index: 0,
            x: (screen_width as i32) / 2,
            y: (screen_height as i32) / 2,
            screen_width,
            screen_height,
            buttons: MouseButtons {
                left: false,
                right: false,
                middle: false,
            },
        }
    }

    /// Reconfigures the display boundary dimensions.
    pub fn set_screen_dimensions(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
        self.x = self.x.clamp(0, (width.saturating_sub(1)) as i32);
        self.y = self.y.clamp(0, (height.saturating_sub(1)) as i32);
    }

    /// Sets the current internal cursor coordinates directly.
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x.clamp(0, (self.screen_width.saturating_sub(1)) as i32);
        self.y = y.clamp(0, (self.screen_height.saturating_sub(1)) as i32);
    }

    /// Returns the current clamped (x, y) cursor position.
    #[inline(always)]
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Returns the current state of all mouse buttons.
    #[inline(always)]
    pub fn buttons(&self) -> MouseButtons {
        self.buttons
    }

    /// Decodes a raw byte from the PS/2 mouse stream.
    ///
    /// Returns `Some(MouseEvent)` upon completing a valid 3-byte packet, or `None` if
    /// more bytes are needed or packet synchronization was lost.
    pub fn process_byte(&mut self, byte: u8) -> Option<MouseEvent> {
        match self.packet_index {
            0 => {
                // Byte 0 Synchronization check: Bit 3 is ALWAYS 1 in standard PS/2 packets
                if (byte & 0x08) == 0 {
                    // Out of sync: discard and wait for a valid header byte
                    return None;
                }
                self.packet[0] = byte;
                self.packet_index = 1;
                None
            }
            1 => {
                self.packet[1] = byte;
                self.packet_index = 2;
                None
            }
            2 => {
                self.packet[2] = byte;
                self.packet_index = 0; // Reset for next packet

                self.decode_packet()
            }
            _ => {
                self.packet_index = 0;
                None
            }
        }
    }

    /// Decodes a verified 3-byte mouse packet into a MouseEvent.
    fn decode_packet(&mut self) -> Option<MouseEvent> {
        let b0 = self.packet[0];
        let b1 = self.packet[1];
        let b2 = self.packet[2];

        // 1. Button states
        let left = (b0 & 0x01) != 0;
        let right = (b0 & 0x02) != 0;
        let middle = (b0 & 0x04) != 0;

        // 2. Overflow flags
        let x_overflow = (b0 & 0x40) != 0;
        let y_overflow = (b0 & 0x80) != 0;

        // 3. Delta X decoding (sign-extended using bit 4)
        let x_sign = (b0 & 0x10) != 0;
        let delta_x = if x_overflow {
            0
        } else if x_sign {
            (b1 as i32) - 256
        } else {
            b1 as i32
        };

        // 4. Delta Y decoding (sign-extended using bit 5)
        // Note: Hardware PS/2 Y is positive upwards, but screen coordinates are positive downwards.
        // Inverting delta_y maps hardware movement correctly onto screen coordinates.
        let y_sign = (b0 & 0x20) != 0;
        let hardware_delta_y = if y_overflow {
            0
        } else if y_sign {
            (b2 as i32) - 256
        } else {
            b2 as i32
        };
        let screen_delta_y = -hardware_delta_y;

        // 5. Update clamped position
        let max_x = (self.screen_width.saturating_sub(1)) as i32;
        let max_y = (self.screen_height.saturating_sub(1)) as i32;
        self.x = (self.x + delta_x).clamp(0, max_x);
        self.y = (self.y + screen_delta_y).clamp(0, max_y);

        // 6. Detect button transitions
        let new_buttons = MouseButtons { left, right, middle };
        let button_change = if left != self.buttons.left {
            Some((
                MouseButton::Left,
                if left { ButtonAction::Pressed } else { ButtonAction::Released },
            ))
        } else if right != self.buttons.right {
            Some((
                MouseButton::Right,
                if right { ButtonAction::Pressed } else { ButtonAction::Released },
            ))
        } else if middle != self.buttons.middle {
            Some((
                MouseButton::Middle,
                if middle { ButtonAction::Pressed } else { ButtonAction::Released },
            ))
        } else {
            None
        };

        self.buttons = new_buttons;

        Some(MouseEvent {
            x: self.x,
            y: self.y,
            delta_x,
            delta_y: screen_delta_y,
            buttons: new_buttons,
            button_change,
        })
    }
}

/// Global system mouse decoder singleton.
static GLOBAL_MOUSE_DECODER: SpinMutex<MouseDecoder> =
    SpinMutex::new(MouseDecoder::new(1280, 800));

/// Reconfigures global mouse screen dimensions.
pub fn set_screen_dimensions(width: u32, height: u32) {
    GLOBAL_MOUSE_DECODER.lock().set_screen_dimensions(width, height);
}

/// Sets global cursor position directly.
#[allow(dead_code)]
pub fn set_position(x: i32, y: i32) {
    GLOBAL_MOUSE_DECODER.lock().set_position(x, y);
}

/// Processes an incoming raw mouse byte from IRQ12 and queues the resulting event.
pub fn handle_byte(byte: u8) {
    if let Some(event) = GLOBAL_MOUSE_DECODER.lock().process_byte(byte) {
        super::queue::push_event(super::event::InputEvent::Mouse(event));
    }
}
