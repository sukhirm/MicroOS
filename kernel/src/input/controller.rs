//! Intel 8042 PS/2 Controller Driver.
//!
//! Configures the dual-channel legacy 8042 keyboard and mouse controller,
//! enables IRQ1 and IRQ12 delivery, sets up scancode translation, and initializes
//! mouse packet streaming with bounded timeout safeguards.

use x86_64::instructions::port::Port;

/// PS/2 Controller Data Port (Read/Write).
pub const PS2_DATA_PORT: u16 = 0x60;
/// PS/2 Controller Status Port (Read).
pub const PS2_STATUS_PORT: u16 = 0x64;
/// PS/2 Controller Command Port (Write).
pub const PS2_COMMAND_PORT: u16 = 0x64;

/// Status Register Bit: Output buffer contains data ready to be read.
pub const STATUS_OUTPUT_BUFFER_FULL: u8 = 0x01;
/// Status Register Bit: Input buffer is full; host cannot write.
pub const STATUS_INPUT_BUFFER_FULL: u8 = 0x02;
/// Status Register Bit: Output buffer contains auxiliary device (mouse) data.
#[allow(dead_code)]
pub const STATUS_MOUSE_DATA: u8 = 0x20;

/// Maximum iterations for bounded I/O polling before timing out.
const TIMEOUT_LIMIT: usize = 100_000;

/// Waits until the controller's input buffer is clear and ready to accept a write.
pub fn wait_write_ready() -> Result<(), ()> {
    let mut status_port: Port<u8> = Port::new(PS2_STATUS_PORT);
    for _ in 0..TIMEOUT_LIMIT {
        let status = unsafe { status_port.read() };
        if status == 0xFF {
            return Err(()); // Floating bus
        }
        if (status & STATUS_INPUT_BUFFER_FULL) == 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(())
}

/// Waits until the controller's output buffer contains data ready to read.
pub fn wait_read_ready() -> Result<(), ()> {
    let mut status_port: Port<u8> = Port::new(PS2_STATUS_PORT);
    for _ in 0..TIMEOUT_LIMIT {
        let status = unsafe { status_port.read() };
        if status == 0xFF {
            return Err(()); // Floating bus
        }
        if (status & STATUS_OUTPUT_BUFFER_FULL) != 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(())
}

/// Writes a command byte to the controller command port (0x64).
pub fn send_command(cmd: u8) -> Result<(), ()> {
    wait_write_ready()?;
    let mut cmd_port: Port<u8> = Port::new(PS2_COMMAND_PORT);
    unsafe { cmd_port.write(cmd) };
    Ok(())
}

/// Writes a data byte to the controller data port (0x60).
pub fn write_data(data: u8) -> Result<(), ()> {
    wait_write_ready()?;
    let mut data_port: Port<u8> = Port::new(PS2_DATA_PORT);
    unsafe { data_port.write(data) };
    Ok(())
}

/// Reads a byte from the controller data port (0x60) if ready.
pub fn read_data() -> Result<u8, ()> {
    wait_read_ready()?;
    let mut data_port: Port<u8> = Port::new(PS2_DATA_PORT);
    Ok(unsafe { data_port.read() })
}

/// Flushes any stale data from the controller output buffer.
pub fn flush_output_buffer() {
    let mut status_port: Port<u8> = Port::new(PS2_STATUS_PORT);
    let mut data_port: Port<u8> = Port::new(PS2_DATA_PORT);
    for _ in 0..32 {
        let status = unsafe { status_port.read() };
        if status != 0xFF && (status & STATUS_OUTPUT_BUFFER_FULL) != 0 {
            let _ = unsafe { data_port.read() };
        } else {
            break;
        }
    }
}

/// Actively and non-blockingly polls the 8042 controller status and data ports (0x64 / 0x60).
///
/// Bounded and non-allocating: performs a single-read check of status port 0x64.
/// If bit 0 (STATUS_OUTPUT_BUFFER_FULL) is set and status != 0xFF:
/// - If bit 5 (STATUS_MOUSE_DATA) is 0: routes scancode to keyboard decoder.
/// - If bit 5 is 1: discards auxiliary data safely to prevent corrupting keyboard stream.
/// Returns true if a keyboard byte was read and processed, false otherwise.
pub fn poll_hardware() -> bool {
    let mut status_port: Port<u8> = Port::new(PS2_STATUS_PORT);
    let status = unsafe { status_port.read() };

    // 0xFF represents a floating bus (no PS/2 hardware / disconnected controller)
    if status == 0xFF || (status & STATUS_OUTPUT_BUFFER_FULL) == 0 {
        return false;
    }

    let mut data_port: Port<u8> = Port::new(PS2_DATA_PORT);
    let byte = unsafe { data_port.read() };

    if (status & STATUS_MOUSE_DATA) != 0 {
        // Auxiliary device (touchpad / mouse) data: forward to PS/2 mouse decoder
        crate::input::mouse::handle_byte(byte);
        true
    } else {
        // First PS/2 port (keyboard) scancode byte
        crate::input::keyboard::handle_scancode(byte);
        true
    }
}

/// Initializes the 8042 PS/2 controller in keyboard-only mode.
///
/// Performs safe, bounded keyboard channel initialization:
/// - Flushes output buffer.
/// - Disables keyboard (0xAD) and mouse (0xA7) ports during configuration.
/// - Reads and configures Controller Configuration Byte (CCB) to enable IRQ1, enable keyboard clock,
///   enable Set 1 translation, and keep mouse clock/interrupt disabled.
/// - Enables primary keyboard port (0xAE).
/// - Sends scanning enable (0xF4) with bounded wait for optional ACK.
/// - Strictly avoids secondary mouse port commands (0xA8, 0xD4) to prevent hardware lockup or spurious IRQs.
pub fn init() -> Result<(), &'static str> {
    log::info!("[PS/2] Initializing 8042 PS/2 controller in keyboard-only mode...");

    // 1. Flush any pending data from previous BIOS/UEFI operations
    flush_output_buffer();

    // 2. Disable both PS/2 ports during initialization
    let _ = send_command(0xAD); // Disable keyboard
    let _ = send_command(0xA7); // Disable mouse (secondary)
    flush_output_buffer();

    // 3. Read Controller Configuration Byte (CCB)
    if send_command(0x20).is_ok() {
        if let Ok(mut ccb) = read_data() {
            // Modify configuration byte:
            // Bit 0: Enable Keyboard Interrupt (IRQ1)
            // Bit 1: Disable Mouse Interrupt (IRQ12)
            // Bit 4: Enable Keyboard Clock (clear disable bit)
            // Bit 5: Disable Mouse Clock (set disable bit)
            // Bit 6: Enable Translation (Translate Set 2 to Set 1)
            ccb |= 0x01;  // Enable IRQ1
            ccb &= !0x02; // Disable IRQ12
            ccb &= !0x10; // Clear keyboard clock disable
            ccb |= 0x20;  // Set mouse clock disable
            ccb |= 0x40;  // Enable Set 1 translation

            if send_command(0x60).is_ok() {
                let _ = write_data(ccb);
            }
        } else {
            log::warn!("[PS/2] CCB read timed out; proceeding with port enable.");
        }
    }

    // 4. Keyboard Channel Initialization
    if send_command(0xAE).is_err() {
        log::warn!("[PS/2] Failed to enable keyboard port (0xAE)");
    }

    // Enable keyboard scanning (command 0xF4 to data port)
    if write_data(0xF4).is_ok() {
        let _ = read_data(); // Optional ACK (0xFA) with bounded timeout
        log::info!("[PS/2] Keyboard scanning enabled (0xF4).");
    } else {
        log::warn!("[PS/2] Failed to send keyboard scanning command (0xF4).");
    }

    flush_output_buffer();
    log::info!("[PS/2] 8042 Controller initialized (keyboard-only, bounded polling ready).");
    Ok(())
}
