//! Low-level x86_64 CPU control and hardware register diagnostics.

use x86_64::registers::control::{Cr0, Cr0Flags, Cr3, Cr4, Cr4Flags};
use x86_64::registers::rflags::{self, RFlags};

/// CPU diagnostic state captured during bare-metal initialization.
#[derive(Debug, Clone, Copy)]
pub struct CpuDiagnostics {
    pub cr0: Cr0Flags,
    pub cr3_frame: u64,
    pub cr4: Cr4Flags,
    pub rflags: RFlags,
}

/// Reads the current x86_64 CPU control registers and flags.
pub fn read_diagnostics() -> CpuDiagnostics {
    let cr0 = Cr0::read();
    let (cr3_frame, _) = Cr3::read();
    let cr4 = Cr4::read();
    let rflags = rflags::read();

    CpuDiagnostics {
        cr0,
        cr3_frame: cr3_frame.start_address().as_u64(),
        cr4,
        rflags,
    }
}

/// Halts the CPU indefinitely in a low-power state.
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
