//! Interrupt Descriptor Table (IDT) and CPU Exception Handlers.
//!
//! Configures hardware exception vectors (#BP, #DF, #GP, #PF) and ensures
//! that double faults execute on a dedicated Interrupt Stack Table (IST).

use spin::lazy::Lazy;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

/// Static Interrupt Descriptor Table (IDT).
static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();

    // 1. Breakpoint Exception (#BP, Vector 3)
    idt.breakpoint.set_handler_fn(breakpoint_handler);

    // 2. Double Fault Exception (#DF, Vector 8)
    // SAFETY: The stack index refers to `DOUBLE_FAULT_IST_INDEX` (index 0 / IST1),
    // which has been configured with a dedicated 20 KiB static stack buffer in our TSS.
    // This prevents triple faults when a stack overflow or cascading fault occurs.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::arch::x86_64::gdt::DOUBLE_FAULT_IST_INDEX);
    }

    // 3. General Protection Fault (#GP, Vector 13)
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault_handler);

    // 4. Page Fault (#PF, Vector 14)
    idt.page_fault.set_handler_fn(page_fault_handler);

    idt
});

/// Initializes and loads the Interrupt Descriptor Table (IDT).
pub fn init() {
    IDT.load();
}

/// Breakpoint exception handler (#BP, Vector 3).
///
/// Triggered by the `int3` software interrupt instruction.
/// Logs CPU stack frame diagnostics and safely returns execution to the caller.
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    crate::println!("\x1b[1;35m================================================================");
    crate::println!("                EXCEPTION: BREAKPOINT (#BP)                     ");
    crate::println!("================================================================\x1b[0m");
    log::warn!(
        "Breakpoint trap encountered at instruction pointer (RIP): {:#018x}",
        stack_frame.instruction_pointer.as_u64()
    );
    log::warn!("Code Segment (CS): {:?}", stack_frame.code_segment);
    log::warn!("CPU RFLAGS: {:?}", stack_frame.cpu_flags);
    log::warn!("Stack Pointer (RSP): {:#018x}", stack_frame.stack_pointer.as_u64());
    log::warn!("Stack Segment (SS): {:?}", stack_frame.stack_segment);
    crate::println!("\x1b[1;35m[INFO ] Resuming normal kernel execution past breakpoint.\x1b[0m");
    crate::println!("\x1b[1;35m================================================================\x1b[0m");
}

/// Double fault exception handler (#DF, Vector 8).
///
/// Triggered when the CPU encounters an unhandled fault while trying to invoke
/// an earlier exception handler. Runs on the dedicated IST1 stack.
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    crate::println!("\n\x1b[1;31m================================================================");
    crate::println!("            FATAL EXCEPTION: DOUBLE FAULT (#DF)                 ");
    crate::println!("================================================================\x1b[0m");
    log::error!("Double fault error code: {:#x}", error_code);
    log::error!(
        "Faulting instruction pointer (RIP): {:#018x}",
        stack_frame.instruction_pointer.as_u64()
    );
    log::error!("Code Segment (CS): {:?}", stack_frame.code_segment);
    log::error!("CPU RFLAGS: {:?}", stack_frame.cpu_flags);
    log::error!("Stack Pointer (RSP): {:#018x}", stack_frame.stack_pointer.as_u64());
    log::error!("Stack Segment (SS): {:?}", stack_frame.stack_segment);
    crate::println!("\x1b[1;31mCaptured on dedicated Double Fault IST stack.\x1b[0m");
    crate::println!("\x1b[1;31mSystem halted via CPU hlt loop.\x1b[0m");
    crate::println!("\x1b[1;31m================================================================\x1b[0m");

    crate::arch::x86_64::cpu::hlt_loop();
}

/// General protection fault exception handler (#GP, Vector 13).
///
/// Triggered by privilege violations, segment limit violations, or reserved bits.
extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::println!("\n\x1b[1;31m================================================================");
    crate::println!("        EXCEPTION: GENERAL PROTECTION FAULT (#GP)               ");
    crate::println!("================================================================\x1b[0m");
    let external = (error_code & 1) != 0;
    let table = match (error_code >> 1) & 0b11 {
        0b00 => "GDT",
        0b01 => "IDT",
        0b10 => "LDT",
        _ => "IDT",
    };
    let selector_index = (error_code >> 3) & 0x1fff;
    log::error!("Raw Error Code: {:#x}", error_code);
    log::error!(
        "Decoded Descriptor: External={}, Table={}, SelectorIndex={:#x}",
        external,
        table,
        selector_index
    );
    log::error!(
        "Faulting instruction pointer (RIP): {:#018x}",
        stack_frame.instruction_pointer.as_u64()
    );
    log::error!("Code Segment (CS): {:?}", stack_frame.code_segment);
    log::error!("CPU RFLAGS: {:?}", stack_frame.cpu_flags);
    log::error!("Stack Pointer (RSP): {:#018x}", stack_frame.stack_pointer.as_u64());
    log::error!("Stack Segment (SS): {:?}", stack_frame.stack_segment);
    crate::println!("\x1b[1;31mSystem halted via CPU hlt loop.\x1b[0m");
    crate::println!("\x1b[1;31m================================================================\x1b[0m");

    crate::arch::x86_64::cpu::hlt_loop();
}

/// Page fault exception handler (#PF, Vector 14).
///
/// Triggered by unmapped virtual memory accesses or page-level protection violations.
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    crate::println!("\n\x1b[1;31m================================================================");
    crate::println!("                 EXCEPTION: PAGE FAULT (#PF)                    ");
    crate::println!("================================================================\x1b[0m");
    match Cr2::read() {
        Ok(addr) => log::error!("Accessed Virtual Address (CR2): {:#018x}", addr.as_u64()),
        Err(err) => log::error!("Accessed Non-Canonical Virtual Address (CR2): {:?}", err),
    }
    log::error!("Page Fault Error Flags: {:?}", error_code);
    log::error!("Protection Violation (P): {}", error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION));
    log::error!("Caused by Write (W/R): {}", error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE));
    log::error!("User Mode (U/S): {}", error_code.contains(PageFaultErrorCode::USER_MODE));
    log::error!("Malformed Table / Reserved Bit (RSVD): {}", error_code.contains(PageFaultErrorCode::MALFORMED_TABLE));
    log::error!("Instruction Fetch (I/D): {}", error_code.contains(PageFaultErrorCode::INSTRUCTION_FETCH));
    log::error!(
        "Faulting instruction pointer (RIP): {:#018x}",
        stack_frame.instruction_pointer.as_u64()
    );
    log::error!("Code Segment (CS): {:?}", stack_frame.code_segment);
    log::error!("CPU RFLAGS: {:?}", stack_frame.cpu_flags);
    log::error!("Stack Pointer (RSP): {:#018x}", stack_frame.stack_pointer.as_u64());
    log::error!("Stack Segment (SS): {:?}", stack_frame.stack_segment);
    crate::println!("\x1b[1;31mSystem halted via CPU hlt loop.\x1b[0m");
    crate::println!("\x1b[1;31m================================================================\x1b[0m");

    crate::arch::x86_64::cpu::hlt_loop();
}
