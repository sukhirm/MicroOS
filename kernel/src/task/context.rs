//! Low-level x86_64 context switching assembly and declarations.
//!
//! Implements preemptive and cooperative context switching using a unified 672-byte frame:
//! - 512 bytes: FPU/SIMD state saved via `fxsave` and restored via `fxrstor`.
//! - 120 bytes: 15 General-Purpose Registers (r15..rax).
//! - 40 bytes: Intel 64 hardware frame (RIP, CS, RFLAGS, RSP, SS).

core::arch::global_asm!(
    r#"
    .global timer_interrupt_entry
    .type timer_interrupt_entry, @function
    timer_interrupt_entry:
        # 1. Push 15 General-Purpose Registers (120 bytes)
        push rax
        push rcx
        push rdx
        push rbx
        push rbp
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15

        # 2. Allocate 512 bytes on stack for FPU/SIMD state (RSP is 16-byte aligned)
        sub rsp, 512
        fxsave [rsp]

        # 3. Pass current stack pointer as 1st argument (rdi) to Rust handler
        mov rdi, rsp
        call timer_interrupt_handler

        # 4. rax contains target task's stack pointer
        mov rsp, rax

        # 5. Restore target task's FPU/SIMD state
        fxrstor [rsp]
        add rsp, 512

        # 6. Restore target task's 15 General-Purpose Registers
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rbx
        pop rdx
        pop rcx
        pop rax

        # 7. Return from interrupt (pops RIP, CS, RFLAGS, RSP, SS)
        iretq

    .global yield_interrupt_entry
    .type yield_interrupt_entry, @function
    yield_interrupt_entry:
        # 1. Push 15 General-Purpose Registers (120 bytes)
        push rax
        push rcx
        push rdx
        push rbx
        push rbp
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15

        # 2. Allocate 512 bytes for FPU/SIMD state
        sub rsp, 512
        fxsave [rsp]

        # 3. Pass current stack pointer (rdi) to Rust cooperative yield handler
        mov rdi, rsp
        call yield_interrupt_handler

        # 4. rax contains target task's stack pointer
        mov rsp, rax

        # 5. Restore target task's FPU/SIMD state
        fxrstor [rsp]
        add rsp, 512

        # 6. Restore target task's 15 General-Purpose Registers
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rbx
        pop rdx
        pop rcx
        pop rax

        # 7. Return from interrupt
        iretq

    .global switch_context_exit
    .type switch_context_exit, @function
    switch_context_exit:
        # rdi = next_rsp: *mut u8
        mov rsp, rdi
        fxrstor [rsp]
        add rsp, 512
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rbx
        pop rdx
        pop rcx
        pop rax
        iretq

    .global task_trampoline
    .type task_trampoline, @function
    task_trampoline:
        # On entry from iretq, RSP is at top (16-byte aligned, RSP % 16 == 0).
        # Before calling r12, RSP % 16 == 0. call r12 pushes 8 bytes, so inside r12 RSP % 16 == 8.
        call r12
        # If entry point returns, terminate cleanly:
        call task_exit
    .Lhalt:
        hlt
        jmp .Lhalt
    "#
);

extern "C" {
    /// Assembly entry point for the 8254 PIT hardware timer interrupt (Vector 32).
    pub fn timer_interrupt_entry();

    /// Assembly entry point for voluntary cooperative yield (Vector 33).
    pub fn yield_interrupt_entry();

    /// Switches execution context to `next_rsp` without saving the current task's state.
    /// Used when a task terminates.
    pub fn switch_context_exit(next_rsp: *mut u8) -> !;

    /// Entry trampoline for new tasks.
    pub fn task_trampoline();
}

/// Called by `timer_interrupt_entry` when a hardware timer interrupt arrives.
#[no_mangle]
pub extern "C" fn timer_interrupt_handler(curr_rsp: *mut u8) -> *mut u8 {
    // 1. Send EOI to PIC first:
    // SAFETY: We acknowledge IRQ0 on Master PIC before switching contexts.
    // CPU interrupts are disabled (cleared by interrupt gate), so no nested interrupt occurs.
    unsafe {
        crate::arch::x86_64::pic::send_eoi(0);
    }

    // 2. Increment tick counter:
    let ticks = crate::arch::x86_64::timer::on_tick();

    // 3. Delegate to scheduler:
    super::scheduler::on_timer_tick(curr_rsp, ticks)
}

/// Called by `yield_interrupt_entry` when a voluntary `yield_now()` is invoked.
#[no_mangle]
pub extern "C" fn yield_interrupt_handler(curr_rsp: *mut u8) -> *mut u8 {
    // Cooperative yield does NOT send PIC EOI because it was triggered by software int 33
    super::scheduler::on_cooperative_yield(curr_rsp)
}

/// Called by `task_trampoline` when a task entry function returns.
#[no_mangle]
pub extern "C" fn task_exit() -> ! {
    super::scheduler::exit_current_task();
}
