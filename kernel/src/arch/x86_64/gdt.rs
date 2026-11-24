//! Global Descriptor Table (GDT) and Task State Segment (TSS) setup.
//!
//! Provides a kernel-owned GDT and TSS with a dedicated Interrupt Stack Table (IST)
//! for handling double faults and stack overflow conditions safely.

use spin::lazy::Lazy;
use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Interrupt Stack Table (IST) index for the Double Fault handler (IST1 = index 0 in table).
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Interrupt Stack Table (IST) index for the Page Fault handler (IST2 = index 1 in table).
pub const PAGE_FAULT_IST_INDEX: u16 = 1;

/// Interrupt Stack Table (IST) index for the General Protection Fault handler (IST3 = index 2 in table).
pub const GENERAL_PROTECTION_IST_INDEX: u16 = 2;

/// Stack size for dedicated IST stacks (20 KiB each).
const IST_STACK_SIZE: usize = 4096 * 5;

/// 16-byte aligned memory buffer for IST stacks.
#[repr(align(16))]
#[allow(dead_code)]
struct IstStack([u8; IST_STACK_SIZE]);

/// Dedicated stack memory for double fault exceptions (IST1).
static mut DOUBLE_FAULT_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);

/// Dedicated stack memory for page fault exceptions (IST2).
static mut PAGE_FAULT_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);

/// Dedicated stack memory for general protection fault exceptions (IST3).
static mut GENERAL_PROTECTION_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);

/// Static Task State Segment (TSS) containing IST configuration.
static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    // SAFETY: We calculate pointers to the tops of the dedicated static stack buffers.
    // The buffers are statically allocated, 16-byte aligned, and never deallocated.
    // Stacks grow downward on x86_64, so the initial RSP must point to the top of the stack.
    let df_start = VirtAddr::from_ptr(core::ptr::addr_of!(DOUBLE_FAULT_STACK));
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = df_start + IST_STACK_SIZE as u64;

    let pf_start = VirtAddr::from_ptr(core::ptr::addr_of!(PAGE_FAULT_STACK));
    tss.interrupt_stack_table[PAGE_FAULT_IST_INDEX as usize] = pf_start + IST_STACK_SIZE as u64;

    let gp_start = VirtAddr::from_ptr(core::ptr::addr_of!(GENERAL_PROTECTION_STACK));
    tss.interrupt_stack_table[GENERAL_PROTECTION_IST_INDEX as usize] = gp_start + IST_STACK_SIZE as u64;

    tss
});

/// GDT segment selectors.
#[derive(Debug, Clone, Copy)]
pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
}

/// Static Global Descriptor Table (GDT) and its associated selectors.
static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code_selector = gdt.append(Descriptor::kernel_code_segment());
    let data_selector = gdt.append(Descriptor::kernel_data_segment());
    let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            code_selector,
            data_selector,
            tss_selector,
        },
    )
});

/// Initializes and loads the kernel-owned GDT, reloads CS/DS/ES/SS, and loads the TSS into TR.
pub fn init() {
    // Load the GDT pointer into GDTR
    GDT.0.load();

    // SAFETY:
    // 1. `CS::set_reg`: Reloads the code segment register with our verified 64-bit Ring 0 code descriptor.
    // 2. `DS/ES/SS::set_reg`: Reloads data and stack segment registers with our verified 64-bit Ring 0 data descriptor.
    // 3. `load_tss`: Loads the Task Register (TR) with our valid TSS selector pointing to the static TSS.
    // All structures have static lifetimes and remain valid for the entire runtime of the kernel.
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        ES::set_reg(GDT.1.data_selector);
        SS::set_reg(GDT.1.data_selector);
        load_tss(GDT.1.tss_selector);
    }
}

/// Returns the active kernel GDT selectors.
#[allow(dead_code)]
pub fn selectors() -> Selectors {
    GDT.1
}
