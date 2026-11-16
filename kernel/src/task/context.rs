//! Low-level x86_64 context switching assembly and declarations.
//!
//! Implements cooperative context switching respecting the System V AMD64 calling convention.

core::arch::global_asm!(
    r#"
    .global switch_context
    .type switch_context, @function
    switch_context:
        # System V AMD64 ABI:
        # rdi = prev_rsp: *mut *mut u8
        # rsi = next_rsp: *mut u8

        # 1. Save callee-saved registers of outgoing task
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15
        pushfq

        # 2. Save current stack pointer to *prev_rsp
        mov [rdi], rsp

        # 3. Load incoming task's stack pointer
        mov rsp, rsi

        # 4. Restore callee-saved registers of incoming task
        popfq
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp

        # 5. Return to incoming task (pops RIP)
        ret

    .global switch_context_exit
    .type switch_context_exit, @function
    switch_context_exit:
        # System V AMD64 ABI:
        # rdi = next_rsp: *mut u8
        # Discards outgoing task's stack and switches to incoming task

        mov rsp, rdi
        popfq
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp
        ret

    .global task_trampoline
    .type task_trampoline, @function
    task_trampoline:
        # On entry, RSP % 16 == 8 (standard function entry condition).
        # Align stack to 16 bytes before call:
        sub rsp, 8
        # r12 contains the task entry point
        call r12
        # If entry point returns, terminate cleanly:
        call task_exit
    .Lhalt:
        hlt
        jmp .Lhalt
    "#
);

extern "C" {
    /// Switches execution context from the previous task to the next task.
    ///
    /// # Safety
    /// - `prev_rsp` must be a valid, aligned, writable pointer to `*mut u8`.
    /// - `next_rsp` must point to a valid saved `TaskContext` on a mapped stack.
    pub fn switch_context(prev_rsp: *mut *mut u8, next_rsp: *mut u8);

    /// Switches execution context to `next_rsp` without saving the current task's state.
    /// Used when a task terminates.
    ///
    /// # Safety
    /// - `next_rsp` must point to a valid saved `TaskContext` on a mapped stack.
    pub fn switch_context_exit(next_rsp: *mut u8) -> !;

    /// Entry trampoline for new tasks.
    pub fn task_trampoline();
}

/// Called by `task_trampoline` when a task entry function returns.
#[no_mangle]
pub extern "C" fn task_exit() -> ! {
    super::scheduler::exit_current_task();
}
