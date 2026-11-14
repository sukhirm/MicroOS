//! Low-level x86_64 CPU control and hardware register diagnostics.

use x86_64::instructions::tables::{sgdt, sidt};
use x86_64::registers::control::{Cr0, Cr0Flags, Cr3, Cr4, Cr4Flags};
use x86_64::registers::rflags::{self, RFlags};
use x86_64::registers::segmentation::{Segment, CS, DS, ES, FS, GS, SS};
use x86_64::structures::gdt::SegmentSelector;

/// CPU diagnostic state captured during bare-metal initialization.
#[derive(Debug, Clone, Copy)]
pub struct CpuDiagnostics {
    pub cr0: Cr0Flags,
    pub cr3_frame: u64,
    pub cr4: Cr4Flags,
    pub rflags: RFlags,
}

/// Hardware descriptor table and segment register state.
#[derive(Debug, Clone, Copy)]
pub struct DescriptorDiagnostics {
    pub gdtr_base: u64,
    pub gdtr_limit: u16,
    pub idtr_base: u64,
    pub idtr_limit: u16,
    pub cs: SegmentSelector,
    pub ss: SegmentSelector,
    pub ds: SegmentSelector,
    pub es: SegmentSelector,
    pub fs: SegmentSelector,
    pub gs: SegmentSelector,
    pub tr: u16,
}

/// Reads the current x86_64 descriptor tables and segment registers.
pub fn read_descriptor_diagnostics() -> DescriptorDiagnostics {
    let gdtr = sgdt();
    let idtr = sidt();
    let cs = CS::get_reg();
    let ss = SS::get_reg();
    let ds = DS::get_reg();
    let es = ES::get_reg();
    let fs = FS::get_reg();
    let gs = GS::get_reg();
    let tr: u16;
    // SAFETY: The `str` instruction stores the current 16-bit Task Register selector.
    // It does not modify memory, stack, or CPU flags.
    unsafe {
        core::arch::asm!("str {0:x}", out(reg) tr, options(nomem, nostack, preserves_flags));
    }

    DescriptorDiagnostics {
        gdtr_base: gdtr.base.as_u64(),
        gdtr_limit: gdtr.limit,
        idtr_base: idtr.base.as_u64(),
        idtr_limit: idtr.limit,
        cs,
        ss,
        ds,
        es,
        fs,
        gs,
        tr,
    }
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
