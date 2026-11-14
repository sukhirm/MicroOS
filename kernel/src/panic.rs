//! Kernel panic handler.
//! Emits panic diagnostics over the serial console and halts the CPU.

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::println!("\n\x1b[1;31m================================================================");
    crate::println!("                       KERNEL PANIC!                            ");
    crate::println!("================================================================\x1b[0m");
    crate::println!("\x1b[1;31mDetails: {}\x1b[0m", info);
    crate::println!("\x1b[1;33mSystem halted via CPU hlt loop.\x1b[0m");
    crate::println!("\x1b[1;31m================================================================\x1b[0m");

    crate::arch::x86_64::cpu::hlt_loop();
}
