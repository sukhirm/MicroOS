//! 16550 UART Serial driver for kernel logging and diagnostics.
//! Operates on COM1 standard port (0x3F8).

use core::fmt::{self, Write};
use spin::Mutex;
use uart_16550::SerialPort;

/// Global spinlock-protected COM1 serial port interface.
///
/// # Safety
/// 0x3F8 is the standard base I/O port address for the primary 16550 UART (COM1)
/// on IBM PC-compatible x86_64 architectures. The port is exclusively owned and synchronized
/// by this static Mutex, preventing concurrent access and race conditions.
pub static SERIAL1: Mutex<SerialPort> = Mutex::new(unsafe { SerialPort::new(0x3F8) });

/// Initializes the COM1 16550 UART port with 115200 baud, 8N1.
pub fn init() {
    SERIAL1.lock().init();
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use x86_64::instructions::interrupts;

    // Output through serial port with interrupts disabled to prevent deadlock
    interrupts::without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Writing to 16550 UART failed");
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
