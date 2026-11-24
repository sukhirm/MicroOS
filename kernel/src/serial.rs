//! 16550 UART Serial driver for kernel logging and diagnostics.
//! Operates on COM1 standard port (0x3F8) with bounded timeouts and non-blocking fallback.

use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::port::Port;

/// Standard COM1 base I/O port address on x86_64 IBM PC compatibles.
pub const COM1_BASE: u16 = 0x3F8;

/// Global spinlock-protected COM1 serial port interface.
pub static SERIAL1: Mutex<SerialPort> = Mutex::new(unsafe { SerialPort::new(COM1_BASE) });

/// Tracks whether serial hardware is offline or unresponsive.
/// When true, subsequent writes return immediately without spinning.
static SERIAL_OFFLINE: AtomicBool = AtomicBool::new(false);

/// Tracks whether the UART scratch register (0x3FF) passed the loopback test.
/// Note: per architecture rules, we do NOT rely exclusively on scratch register to disable UART,
/// but keep track of it as a diagnostic indicator.
static SCRATCH_OK: AtomicBool = AtomicBool::new(false);

/// Consecutive failed byte transmissions before marking serial offline.
static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);

/// Bounded spin timeout per byte transmission attempt (~5000 iterations).
const UART_SPIN_TIMEOUT: u32 = 5_000;

/// Threshold of consecutive timed-out bytes before disabling serial output.
const MAX_CONSECUTIVE_FAILURES: u32 = 16;

/// Initializes the COM1 16550 UART port with 115200 baud, 8N1.
/// Performs a non-exclusive scratch register probe and safe UART initialization.
pub fn init() {
    // 1. Non-exclusive scratch register probe on port 0x3FF (base + 7)
    let mut scratch_port = Port::<u8>::new(COM1_BASE + 7);
    let probe_byte = 0xA5;
    unsafe {
        scratch_port.write(probe_byte);
    }
    let read_back = unsafe { scratch_port.read() };
    if read_back == probe_byte {
        // Test with alternating pattern
        unsafe { scratch_port.write(0x5A); }
        if unsafe { scratch_port.read() } == 0x5A {
            SCRATCH_OK.store(true, Ordering::Relaxed);
        }
    }

    // 2. Initialize the serial port hardware registers
    SERIAL1.lock().init();

    // Reset failure counter
    CONSECUTIVE_FAILURES.store(0, Ordering::Relaxed);
    SERIAL_OFFLINE.store(false, Ordering::Relaxed);
}

/// Hardware verification status of the COM1 serial interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialHardwareStatus {
    /// 16550 UART hardware verified via scratch register loopback (0x3FF).
    VerifiedHardware,
    /// Scratch register probe failed (no UART echoed 0xA5/0x5A); unmapped port floats high (0xFF).
    FloatingBusUnmapped,
    /// Consecutive byte transmissions timed out; driver safely backsoff.
    OfflineTimedOut,
}

/// Returns true if the serial driver is currently online and accepting output.
#[allow(dead_code)]
pub fn is_online() -> bool {
    !SERIAL_OFFLINE.load(Ordering::Relaxed)
}

/// Returns true if the UART scratch register loopback probe succeeded during init.
pub fn is_scratch_ok() -> bool {
    SCRATCH_OK.load(Ordering::Relaxed)
}

/// Returns the evaluated hardware status of the serial interface.
pub fn hardware_status() -> SerialHardwareStatus {
    if is_scratch_ok() {
        SerialHardwareStatus::VerifiedHardware
    } else if SERIAL_OFFLINE.load(Ordering::Relaxed) {
        SerialHardwareStatus::OfflineTimedOut
    } else {
        SerialHardwareStatus::FloatingBusUnmapped
    }
}

/// Attempts to transmit a single byte on the serial port with a bounded spin timeout.
/// Returns true if transmission succeeded, or false if it timed out.
fn send_byte_bounded(port: &mut SerialPort, byte: u8) -> bool {
    if SERIAL_OFFLINE.load(Ordering::Relaxed) {
        return false;
    }

    let mut spins = 0;
    while spins < UART_SPIN_TIMEOUT {
        match port.try_send_raw(byte) {
            Ok(()) => {
                CONSECUTIVE_FAILURES.store(0, Ordering::Relaxed);
                return true;
            }
            Err(_) => {
                core::hint::spin_loop();
                spins += 1;
            }
        }
    }

    // Byte transmission timed out
    let fails = CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
    if fails >= MAX_CONSECUTIVE_FAILURES {
        SERIAL_OFFLINE.store(true, Ordering::Relaxed);
    }
    false
}

/// Safe wrapper implementing `core::fmt::Write` with bounded timeouts.
struct SafeSerialWriter<'a>(&'a mut SerialPort);

impl<'a> fmt::Write for SafeSerialWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if SERIAL_OFFLINE.load(Ordering::Relaxed) {
            return Ok(());
        }

        for byte in s.bytes() {
            if byte == b'\n' {
                if !send_byte_bounded(self.0, b'\r') {
                    break;
                }
            }
            if !send_byte_bounded(self.0, byte) {
                break;
            }
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use x86_64::instructions::interrupts;

    if SERIAL_OFFLINE.load(Ordering::Relaxed) {
        return;
    }

    // Output through serial port with interrupts disabled to prevent deadlock
    interrupts::without_interrupts(|| {
        let mut port = SERIAL1.lock();
        let mut writer = SafeSerialWriter(&mut *port);
        let _ = fmt::write(&mut writer, args);
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
