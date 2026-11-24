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
    // SAFETY: Dedicated 20 KiB IST1 stack in TSS prevents triple faults.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::arch::x86_64::gdt::DOUBLE_FAULT_IST_INDEX);
    }

    // 3. General Protection Fault (#GP, Vector 13)
    // SAFETY: Dedicated 20 KiB IST3 stack in TSS prevents stack-related double faults.
    unsafe {
        idt.general_protection_fault
            .set_handler_fn(general_protection_fault_handler)
            .set_stack_index(crate::arch::x86_64::gdt::GENERAL_PROTECTION_IST_INDEX);
    }

    // 4. Page Fault (#PF, Vector 14)
    // SAFETY: Dedicated 20 KiB IST2 stack in TSS prevents stack-related double faults.
    unsafe {
        idt.page_fault
            .set_handler_fn(page_fault_handler)
            .set_stack_index(crate::arch::x86_64::gdt::PAGE_FAULT_IST_INDEX);
    }

    // Standard Hardware CPU Exception Gates (Vectors 0..31)
    idt.divide_error.set_handler_fn(divide_error_handler);
    idt.debug.set_handler_fn(debug_handler);
    idt.non_maskable_interrupt.set_handler_fn(nmi_handler);
    idt.overflow.set_handler_fn(overflow_handler);
    idt.bound_range_exceeded.set_handler_fn(bound_range_exceeded_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.device_not_available.set_handler_fn(device_not_available_handler);
    idt.invalid_tss.set_handler_fn(invalid_tss_handler);
    idt.segment_not_present.set_handler_fn(segment_not_present_handler);
    idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);
    idt.x87_floating_point.set_handler_fn(x87_floating_point_handler);
    idt.alignment_check.set_handler_fn(alignment_check_handler);
    idt.machine_check.set_handler_fn(machine_check_handler);
    idt.simd_floating_point.set_handler_fn(simd_floating_point_handler);
    idt.virtualization.set_handler_fn(virtualization_handler);
    idt.cp_protection_exception.set_handler_fn(cp_protection_exception_handler);
    idt.hv_injection_exception.set_handler_fn(hv_injection_exception_handler);
    idt.vmm_communication_exception.set_handler_fn(vmm_communication_exception_handler);
    idt.security_exception.set_handler_fn(security_exception_handler);

    // 5. Hardware Timer Interrupt (Vector 32 / IRQ0)
    // SAFETY: Handled by assembly stub to preserve complete register/FPU state.
    unsafe {
        idt[32].set_handler_addr(x86_64::VirtAddr::new(
            crate::task::context::timer_interrupt_entry as *const () as usize as u64,
        ));
    }

    // 6. Hardware Keyboard Interrupt (Vector 33 / IRQ1)
    idt[33].set_handler_fn(keyboard_interrupt_handler);

    // 7. Unhandled Master PIC Interrupts (Vectors 34..=38 / IRQs 2..6)
    // Acknowledges Master PIC only (port 0x20).
    for vec in 34..=38 {
        idt[vec].set_handler_fn(unhandled_pic_master_handler);
    }

    // 8. Spurious Master PIC Interrupt (Vector 39 / IRQ7)
    // Per Intel 8259A hardware specification: NO EOI must be sent!
    idt[39].set_handler_fn(spurious_pic1_handler);

    // 9. Unhandled Slave PIC Interrupts (Vectors 40..=43 / IRQs 8..11)
    // Acknowledges both Slave PIC (port 0xA0) and Master PIC (port 0x20).
    for vec in 40..=43 {
        idt[vec].set_handler_fn(unhandled_pic_slave_handler);
    }

    // 10. Hardware Mouse Interrupt (Vector 44 / IRQ12)
    idt[44].set_handler_fn(mouse_interrupt_handler);

    // 11. Unhandled Slave PIC Interrupts (Vectors 45..=46 / IRQs 13..14)
    // Acknowledges both Slave PIC (port 0xA0) and Master PIC (port 0x20).
    for vec in 45..=46 {
        idt[vec].set_handler_fn(unhandled_pic_slave_handler);
    }

    // 12. Spurious Slave PIC Interrupt (Vector 47 / IRQ15)
    // Per Intel 8259A hardware specification: EOI sent to Master PIC ONLY!
    idt[47].set_handler_fn(spurious_pic2_handler);

    // 13. Voluntary Cooperative Yield (Vector 48)
    // SAFETY: Triggered by software `int 48` in Ring 0. NO PIC EOI.
    unsafe {
        idt[48].set_handler_addr(x86_64::VirtAddr::new(
            crate::task::context::yield_interrupt_entry as *const () as usize as u64,
        ));
    }

    // 14. Default Unhandled Non-PIC Vectors (Vectors 49..=255)
    // Software interrupts, MSI, APIC, HPET: NO PIC EOI!
    // Safely absorbs without corrupting 8259 PIC in-service register.
    for vec in 49..=255 {
        idt[vec].set_handler_fn(unhandled_non_pic_vector_handler);
    }

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
    crate::framebuffer::render_exception_screen(
        "DOUBLE FAULT (#DF, Vector 8)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Dedicated IST1 stack active. System unrecoverable.",
    );

    crate::println!("\n\x1b[1;31m================================================================");
    crate::println!("            FATAL EXCEPTION: DOUBLE FAULT (#DF)                 ");
    crate::println!("================================================================\x1b[0m");
    log::error!("Double fault error code: {:#x}", error_code);
    if let Ok(cr2) = Cr2::read() {
        log::error!("CR2 Accessed Memory Address: {:#018x}", cr2.as_u64());
    }
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
    crate::framebuffer::render_exception_screen(
        "GENERAL PROTECTION FAULT (#GP, Vector 13)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Privilege, segment limit, or reserved bit violation.",
    );

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
    crate::framebuffer::render_exception_screen(
        "PAGE FAULT (#PF, Vector 14)",
        stack_frame.instruction_pointer.as_u64(),
        error_code.bits(),
        "Page translation fault or unmapped virtual memory access.",
    );

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

/// Hardware Keyboard Interrupt Handler (Vector 33 / IRQ1).
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut status_port: x86_64::instructions::port::Port<u8> =
        x86_64::instructions::port::Port::new(crate::input::controller::PS2_STATUS_PORT);
    let status = unsafe { status_port.read() };

    if (status & crate::input::controller::STATUS_OUTPUT_BUFFER_FULL) != 0 {
        let mut data_port: x86_64::instructions::port::Port<u8> =
            x86_64::instructions::port::Port::new(crate::input::controller::PS2_DATA_PORT);
        let scancode = unsafe { data_port.read() };
        crate::input::keyboard::handle_scancode(scancode);
    }

    unsafe {
        crate::arch::x86_64::pic::send_eoi(1);
    }
}

/// Hardware Mouse Interrupt Handler (Vector 44 / IRQ12).
extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut status_port: x86_64::instructions::port::Port<u8> =
        x86_64::instructions::port::Port::new(crate::input::controller::PS2_STATUS_PORT);
    let status = unsafe { status_port.read() };

    if (status & crate::input::controller::STATUS_OUTPUT_BUFFER_FULL) != 0 {
        let mut data_port: x86_64::instructions::port::Port<u8> =
            x86_64::instructions::port::Port::new(crate::input::controller::PS2_DATA_PORT);
        let byte = unsafe { data_port.read() };
        crate::input::mouse::handle_byte(byte);
    }

    unsafe {
        crate::arch::x86_64::pic::send_eoi(12);
    }
}

/// Spurious Interrupt Handler for Master PIC (Vector 39 / IRQ7).
///
/// Under 8259 PIC specification, spurious IRQ7 does NOT receive an EOI.
extern "x86-interrupt" fn spurious_pic1_handler(_stack_frame: InterruptStackFrame) {
    // Spurious IRQ 7: No EOI required per Intel 8259A hardware specification
}

/// Spurious Interrupt Handler for Slave PIC (Vector 47 / IRQ15).
///
/// Under 8259 PIC specification, spurious IRQ15 must send EOI to Master PIC only.
extern "x86-interrupt" fn spurious_pic2_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        crate::arch::x86_64::pic::send_eoi(7); // Master PIC EOI
    }
}

/// Unhandled Master PIC Interrupt Handler (Vectors 34..=38 / IRQs 2..6).
///
/// Sends EOI to Master PIC port 0x20 only so lower-priority IRQs are not blocked.
extern "x86-interrupt" fn unhandled_pic_master_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        crate::arch::x86_64::pic::send_eoi(0); // Master PIC EOI only
    }
}

/// Unhandled Slave PIC Interrupt Handler (Vectors 40..=43, 45..=46 / IRQs 8..11, 13..14).
///
/// Sends dual EOI: Slave PIC (port 0xA0) first, then Master PIC (port 0x20).
extern "x86-interrupt" fn unhandled_pic_slave_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        crate::arch::x86_64::pic::send_eoi(8); // Dual EOI: Slave (0xA0) then Master (0x20)
    }
}

/// Unhandled Non-PIC Vector Handler (Vectors 49..=255).
///
/// Absorbs unexpected non-PIC interrupts (e.g. software traps, APIC, MSI).
/// CRITICAL: Must NEVER send PIC EOI, as touching ports 0x20/0xA0 for non-PIC vectors
/// corrupts the 8259 PIC Priority Resolver and In-Service Register.
extern "x86-interrupt" fn unhandled_non_pic_vector_handler(_stack_frame: InterruptStackFrame) {
    // Zero PIC interaction: no EOI command sent.
}

/// Divide Error (#DE, Vector 0).
extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "DIVIDE ERROR (#DE, Vector 0)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Division by zero or division result overflow.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Debug Exception (#DB, Vector 1).
extern "x86-interrupt" fn debug_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "DEBUG EXCEPTION (#DB, Vector 1)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Hardware debug event or single-step trap.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Non-Maskable Interrupt (#NMI, Vector 2).
extern "x86-interrupt" fn nmi_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "NON-MASKABLE INTERRUPT (#NMI, Vector 2)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Hardware NMI pin asserted (e.g. parity or uncorrectable memory error).",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Overflow (#OF, Vector 4).
extern "x86-interrupt" fn overflow_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "OVERFLOW (#OF, Vector 4)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "INTO instruction executed with overflow flag (OF) set.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Bound Range Exceeded (#BR, Vector 5).
extern "x86-interrupt" fn bound_range_exceeded_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "BOUND RANGE EXCEEDED (#BR, Vector 5)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "BOUND instruction operand outside defined range boundaries.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Invalid Opcode (#UD, Vector 6).
extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "INVALID OPCODE (#UD, Vector 6)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Attempted execution of invalid or reserved CPU instruction.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Device Not Available (#NM, Vector 7).
extern "x86-interrupt" fn device_not_available_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "DEVICE NOT AVAILABLE (#NM, Vector 7)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "FPU/MMX/SSE instruction attempted while CR0.TS or CR0.EM set.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Invalid TSS (#TS, Vector 10).
extern "x86-interrupt" fn invalid_tss_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    crate::framebuffer::render_exception_screen(
        "INVALID TSS (#TS, Vector 10)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Task State Segment validity or limit check failed.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Segment Not Present (#NP, Vector 11).
extern "x86-interrupt" fn segment_not_present_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    crate::framebuffer::render_exception_screen(
        "SEGMENT NOT PRESENT (#NP, Vector 11)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Referenced segment descriptor has Present bit cleared.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Stack-Segment Fault (#SS, Vector 12).
extern "x86-interrupt" fn stack_segment_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    crate::framebuffer::render_exception_screen(
        "STACK-SEGMENT FAULT (#SS, Vector 12)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Stack boundary limit exceeded or non-canonical stack address.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// x87 Floating-Point Exception (#MF, Vector 16).
extern "x86-interrupt" fn x87_floating_point_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "x87 FLOATING-POINT (#MF, Vector 16)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Legacy x87 FPU floating-point numeric error.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Alignment Check (#AC, Vector 17).
extern "x86-interrupt" fn alignment_check_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    crate::framebuffer::render_exception_screen(
        "ALIGNMENT CHECK (#AC, Vector 17)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Unaligned memory operand accessed with alignment check enabled.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Machine Check (#MC, Vector 18).
extern "x86-interrupt" fn machine_check_handler(stack_frame: InterruptStackFrame) -> ! {
    crate::framebuffer::render_exception_screen(
        "MACHINE CHECK (#MC, Vector 18)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Internal processor hardware error detected by Machine Check Architecture.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// SIMD Floating-Point Exception (#XM, Vector 19).
extern "x86-interrupt" fn simd_floating_point_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "SIMD FLOATING-POINT (#XM, Vector 19)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "SSE/AVX unmasked floating-point exception occurred.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Virtualization Exception (#VE, Vector 20).
extern "x86-interrupt" fn virtualization_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "VIRTUALIZATION (#VE, Vector 20)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "EPT violation converted to virtualization exception in guest.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Control Protection Exception (#CP, Vector 21).
extern "x86-interrupt" fn cp_protection_exception_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::framebuffer::render_exception_screen(
        "CONTROL PROTECTION (#CP, Vector 21)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Control Flow Enforcement (CET) shadow stack or indirect branch violation.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Hypervisor Injection Exception (#HV, Vector 28).
extern "x86-interrupt" fn hv_injection_exception_handler(stack_frame: InterruptStackFrame) {
    crate::framebuffer::render_exception_screen(
        "HV INJECTION (#HV, Vector 28)",
        stack_frame.instruction_pointer.as_u64(),
        0,
        "Hypervisor event notification injection.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// VMM Communication Exception (#VC, Vector 29).
extern "x86-interrupt" fn vmm_communication_exception_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::framebuffer::render_exception_screen(
        "VMM COMMUNICATION (#VC, Vector 29)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "AMD SEV-ES / SEV-SNP virtual machine communication event.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}

/// Security Exception (#SX, Vector 30).
extern "x86-interrupt" fn security_exception_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::framebuffer::render_exception_screen(
        "SECURITY EXCEPTION (#SX, Vector 30)",
        stack_frame.instruction_pointer.as_u64(),
        error_code,
        "Security-sensitive event or TPM/TXT violation.",
    );
    crate::arch::x86_64::cpu::hlt_loop();
}
