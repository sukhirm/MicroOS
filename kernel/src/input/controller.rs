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
        if (unsafe { status_port.read() } & STATUS_INPUT_BUFFER_FULL) == 0 {
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
        if (unsafe { status_port.read() } & STATUS_OUTPUT_BUFFER_FULL) != 0 {
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

/// Routes a data byte to the secondary PS/2 port (auxiliary mouse).
pub fn write_mouse(data: u8) -> Result<(), ()> {
    // Command 0xD4 instructs the 8042 controller to route the next data byte to the mouse
    send_command(0xD4)?;
    write_data(data)
}

/// Flushes any stale data from the controller output buffer.
pub fn flush_output_buffer() {
    let mut status_port: Port<u8> = Port::new(PS2_STATUS_PORT);
    let mut data_port: Port<u8> = Port::new(PS2_DATA_PORT);
    for _ in 0..32 {
        if (unsafe { status_port.read() } & STATUS_OUTPUT_BUFFER_FULL) != 0 {
            let _ = unsafe { data_port.read() };
        } else {
            break;
        }
    }
}

/// Initializes the 8042 PS/2 controller, keyboard, and mouse channels.
///
/// Returns `Ok(())` on successful configuration or `Err(&str)` on failure.
/// Will not panic or hang if the hardware is unresponsive.
pub fn init() -> Result<(), &'static str> {
    log::info!("[PS/2] Initializing 8042 PS/2 controller...");

    // 1. Flush any pending data
    flush_output_buffer();

    // 2. Disable both PS/2 ports during initialization
    let _ = send_command(0xAD); // Disable keyboard
    let _ = send_command(0xA7); // Disable mouse
    flush_output_buffer();

    // 3. Read Controller Configuration Byte (CCB)
    if send_command(0x20).is_err() {
        return Err("Failed to send read CCB command to 8042 controller");
    }
    let mut ccb = match read_data() {
        Ok(b) => b,
        Err(_) => return Err("Timed out reading CCB from 8042 controller"),
    };

    // 4. Modify configuration byte:
    // Bit 0: Enable Keyboard Interrupt (IRQ1)
    // Bit 1: Enable Mouse Interrupt (IRQ12)
    // Bit 4: Enable Keyboard Clock (clear disable bit)
    // Bit 5: Enable Mouse Clock (clear disable bit)
    // Bit 6: Enable Translation (Translate Set 2 to Set 1)
    ccb |= 0x01;  // Enable IRQ1
    ccb |= 0x02;  // Enable IRQ12
    ccb &= !0x10; // Clear keyboard clock disable
    ccb &= !0x20; // Clear mouse clock disable
    ccb |= 0x40;  // Enable Set 1 translation

    // 5. Write back modified CCB
    if send_command(0x60).is_err() || write_data(ccb).is_err() {
        return Err("Failed to write updated CCB to 8042 controller");
    }

    // 6. Enable both PS/2 device ports
    if send_command(0xAE).is_err() {
        log::warn!("[PS/2] Failed to enable keyboard port (0xAE)");
    }
    if send_command(0xA8).is_err() {
        log::warn!("[PS/2] Failed to enable mouse port (0xA8)");
    }

    // 7. Configure Mouse Device
    // Reset mouse (0xFF) -> expects ACK 0xFA, then self-test 0xAA, then device ID 0x00
    if write_mouse(0xFF).is_ok() {
        let _ = read_data(); // ACK 0xFA
        let _ = read_data(); // 0xAA
        let _ = read_data(); // 0x00
    }

    // Set mouse to default sampling parameters (0xF6)
    if write_mouse(0xF6).is_ok() {
        let _ = read_data(); // ACK 0xFA
    }

    // Enable mouse streaming data reporting (0xF4)
    if write_mouse(0xF4).is_ok() {
        let _ = read_data(); // ACK 0xFA
        log::info!("[PS/2] Mouse streaming data reporting enabled.");
    } else {
        log::warn!("[PS/2] Failed to enable mouse data reporting.");
    }

    // 8. Configure Keyboard Device
    // Enable keyboard scanning (0xF4)
    if write_data(0xF4).is_ok() {
        let _ = read_data(); // ACK 0xFA
        log::info!("[PS/2] Keyboard scanning enabled.");
    }

    flush_output_buffer();
    log::info!("[PS/2] 8042 Controller initialized successfully (IRQ1 & IRQ12 active).");
    Ok(())
}
