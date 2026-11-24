//! Kernel panic handler.
//! Emits panic diagnostics over the serial console, renders panic diagnostics
//! directly onto the linear framebuffer without heap allocations, and halts the CPU.

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 1. Render high-contrast panic diagnostics directly onto the GOP framebuffer if available
    crate::framebuffer::render_panic_screen(info);

    // 2. Stream panic details over the serial port (non-blocking, bounded timeout)
    crate::println!("\n\x1b[1;31m================================================================");
    crate::println!("                       KERNEL PANIC!                            ");
    crate::println!("================================================================\x1b[0m");
    crate::println!("\x1b[1;31mDetails: {}\x1b[0m", info);
    let heap_diag = crate::memory::allocator::heap_diagnostics();
    crate::println!(
        "\x1b[1;33mHeap Diagnostics: cap={} B, alloc={} B, free={} B, largest={} B, blocks={}, fails={}\x1b[0m",
        heap_diag.total_bytes,
        heap_diag.allocated_bytes,
        heap_diag.total_free_bytes,
        heap_diag.largest_free_block_bytes,
        heap_diag.free_blocks_count,
        heap_diag.fail_count
    );
    if heap_diag.last_failed_size > 0 {
        crate::println!(
            "\x1b[1;31mLast Failed Allocation Request: {} bytes (align {})\x1b[0m",
            heap_diag.last_failed_size,
            heap_diag.last_failed_align
        );
    }
    crate::println!("\x1b[1;33mSystem halted via CPU hlt loop.\x1b[0m");
    crate::println!("\x1b[1;31m================================================================\x1b[0m");

    // 3. Halt the CPU indefinitely in a safe low-power loop
    crate::arch::x86_64::cpu::hlt_loop();
}
