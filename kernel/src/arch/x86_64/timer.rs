//! 8254 Programmable Interval Timer (PIT) Driver.
//!
//! Configures Channel 0 in Mode 3 (Square Wave Generator) to fire periodic
//! hardware timer interrupts on IRQ0 at 100 Hz (10 ms intervals).

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::instructions::port::Port;

/// PIT I/O ports.
const PIT_CHANNEL_0_DATA: u16 = 0x40;
const PIT_COMMAND_PORT: u16 = 0x43;

/// Base oscillator frequency of the 8254 PIT in Hz.
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

/// Target timer interrupt frequency: 100 Hz (10 ms per tick).
pub const TARGET_FREQUENCY_HZ: u32 = 100;

/// Global atomic counter tracking elapsed timer ticks since boot.
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Initializes the 8254 PIT Channel 0 at 100 Hz.
///
/// # Safety
/// Caller must ensure that interrupt handling is properly configured.
pub unsafe fn init() {
    let mut cmd_port: Port<u8> = Port::new(PIT_COMMAND_PORT);
    let mut data_port: Port<u8> = Port::new(PIT_CHANNEL_0_DATA);

    // Command byte 0x36: Channel 0, Lobyle/Hibyte access, Mode 3 (square wave), binary
    cmd_port.write(0x36);

    let divisor = (PIT_BASE_FREQUENCY / TARGET_FREQUENCY_HZ) as u16;
    let low = (divisor & 0xFF) as u8;
    let high = ((divisor >> 8) & 0xFF) as u8;

    data_port.write(low);
    data_port.write(high);

    log::info!(
        "[PIT] 8254 PIT initialized: Mode 3 square wave at {} Hz (divisor: {}).",
        TARGET_FREQUENCY_HZ,
        divisor
    );
}

/// Increments the global tick counter. Called by the timer interrupt handler.
pub fn on_tick() -> u64 {
    TICKS.fetch_add(1, Ordering::Relaxed) + 1
}

/// Returns the total number of timer ticks elapsed since boot.
#[allow(dead_code)]
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Returns the system uptime in milliseconds (10 ms per tick at 100 Hz).
#[allow(dead_code)]
pub fn uptime_ms() -> u64 {
    ticks() * (1000 / TARGET_FREQUENCY_HZ as u64)
}
