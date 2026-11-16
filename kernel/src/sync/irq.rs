//! Atomic IRQ-State Management.
//!
//! Provides `IrqGuard` for safe, atomic interrupt disabling and restoration.
//! Avoids race conditions between checking IF and executing CLI, and avoids invalid
//! compiler assembly constraints (`nomem`, `preserves_flags`).

/// An RAII guard that atomically disables hardware interrupts on creation and
/// restores the original interrupt state upon drop.
#[derive(Debug)]
pub struct IrqGuard {
    saved_rflags: u64,
}

impl IrqGuard {
    /// Atomically captures the current RFLAGS and disables maskable hardware interrupts.
    ///
    /// # Safety and Compiler Constraints
    /// - `pushfq` and `pop` touch stack memory, so `options(nomem)` is omitted.
    /// - `cli` modifies the IF flag in RFLAGS, so `options(preserves_flags)` is omitted.
    /// - The target `x86_64-unknown-none` disables the red-zone, guaranteeing that stack
    ///   pushes will not overwrite compiler local variables.
    #[inline(always)]
    pub fn acquire() -> Self {
        let saved_rflags: u64;
        unsafe {
            core::arch::asm!(
                "pushfq",
                "cli",
                "pop {0}",
                out(reg) saved_rflags,
            );
        }
        Self { saved_rflags }
    }

    /// Returns true if interrupts were enabled at the time this guard was created.
    #[inline(always)]
    pub fn was_interrupt_enabled(&self) -> bool {
        (self.saved_rflags & 0x200) != 0
    }
}

impl Drop for IrqGuard {
    /// Restores the original interrupt state.
    ///
    /// Executes `sti` if and only if the IF bit (bit 9, 0x200) was set when `acquire()` was called.
    /// If interrupts were already disabled prior to acquiring this guard, they remain disabled.
    #[inline(always)]
    fn drop(&mut self) {
        if self.was_interrupt_enabled() {
            unsafe {
                core::arch::asm!("sti");
            }
        }
    }
}
