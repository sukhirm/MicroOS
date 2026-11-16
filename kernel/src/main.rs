#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod arch;
mod framebuffer;
mod logger;
mod memory;
mod panic;
mod serial;
mod sync;
mod task;

use bootloader_api::config::Mapping;
use bootloader_api::info::MemoryRegionKind;
use bootloader_api::{entry_point, BootInfo, BootloaderConfig};

/// Kernel bootloader configuration.
/// Requests dynamic physical memory mapping and a 128 KiB stack.
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.kernel_stack_size = 128 * 1024;
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

/// Bare-metal kernel entry point called by the UEFI bootloader.
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // 1. Initialize 16550 UART serial driver for early diagnostic logging
    serial::init();

    // 2. Initialize global log framework
    logger::init();

    println!("\x1b[1;36m================================================================");
    println!("             Micro-OS Bare-Metal Kernel (x86_64)                ");
    println!("             Phase 1: Foundational Architecture                 ");
    println!("        Visual Identity: Cosmic Slate / Obsidian Azure          ");
    println!("================================================================\x1b[0m");

    log::info!("Micro-OS kernel startup initiated.");
    log::info!("API version: {}.{}.{}", 
        boot_info.api_version.version_major(),
        boot_info.api_version.version_minor(),
        boot_info.api_version.version_patch()
    );

    // 3. Hardware CPU Diagnostics
    let cpu_diag = arch::x86_64::cpu::read_diagnostics();
    log::info!("CPU State: Long Mode 64-bit verified.");
    log::info!("CPU Register CR0: {:?}", cpu_diag.cr0);
    log::info!("CPU Register CR3 (PML4 root frame): {:#018x}", cpu_diag.cr3_frame);
    log::info!("CPU Register CR4: {:?}", cpu_diag.cr4);
    log::info!("CPU RFLAGS: {:?}", cpu_diag.rflags);

    let desc_diag = arch::x86_64::cpu::read_descriptor_diagnostics();
    log::info!("Bootloader GDTR: base={:#018x}, limit={:#06x}", desc_diag.gdtr_base, desc_diag.gdtr_limit);
    log::info!("Bootloader IDTR: base={:#018x}, limit={:#06x}", desc_diag.idtr_base, desc_diag.idtr_limit);
    log::info!("Bootloader Segment Selectors: CS={:?}, SS={:?}, DS={:?}, ES={:?}, FS={:?}, GS={:?}",
        desc_diag.cs, desc_diag.ss, desc_diag.ds, desc_diag.es, desc_diag.fs, desc_diag.gs);
    log::info!("Bootloader Task Register (TR): {:#06x}", desc_diag.tr);

    // 4. Initialize Kernel-owned GDT & TSS with dedicated Double Fault IST
    log::info!("Initializing kernel-owned GDT and TSS...");
    arch::x86_64::gdt::init();
    let kernel_desc_diag = arch::x86_64::cpu::read_descriptor_diagnostics();
    log::info!("Kernel GDTR: base={:#018x}, limit={:#06x}", kernel_desc_diag.gdtr_base, kernel_desc_diag.gdtr_limit);
    log::info!("Kernel Segment Selectors: CS={:?}, SS={:?}, DS={:?}, ES={:?}",
        kernel_desc_diag.cs, kernel_desc_diag.ss, kernel_desc_diag.ds, kernel_desc_diag.es);
    log::info!("Kernel Task Register (TR): {:#06x}", kernel_desc_diag.tr);
    log::info!("Kernel GDT and TSS initialization complete.");

    // 5. Initialize Interrupt Descriptor Table (IDT) with hardware exception handlers
    log::info!("Initializing Interrupt Descriptor Table (IDT)...");
    arch::x86_64::idt::init();
    let idt_diag = arch::x86_64::cpu::read_descriptor_diagnostics();
    log::info!("Kernel IDTR: base={:#018x}, limit={:#06x}", idt_diag.idtr_base, idt_diag.idtr_limit);
    log::info!("Kernel IDT initialized (handlers: #BP, #DF with IST1, #GP, #PF).");

    // 6. Test controlled exception dispatch (Breakpoint #BP)
    log::info!("Testing controlled exception dispatch via int3 (software breakpoint)...");
    x86_64::instructions::interrupts::int3();
    log::info!("Exception verification complete: returned successfully from #BP handler.");

    // 7. Memory Management & Heap Allocator (Phase 3A)
    log::info!("Initializing Phase 3A Memory Management Subsystem...");
    let phys_mem_offset = boot_info
        .physical_memory_offset
        .into_option()
        .expect("Bootloader did not provide physical memory offset");
    // SAFETY: boot_info is statically allocated and valid for the entire runtime.
    let memory_regions = unsafe { &*core::ptr::addr_of!(boot_info.memory_regions) };
    memory::init(memory_regions, phys_mem_offset);
    memory::run_heap_self_tests();

    // 8. Synchronization Subsystem (Phase 3C)
    log::info!("Initializing Phase 3C Synchronization Subsystem...");
    sync::run_self_tests();

    // 9. Task Management Subsystem (Phase 3B/3C)
    log::info!("Initializing Task Subsystem...");
    task::run_self_tests();

    // 10. Hardware Timer & Interrupt Controller (Phase 3C)
    log::info!("Initializing 8259 PIC and 8254 PIT (100 Hz timer)...");
    unsafe {
        arch::x86_64::pic::init();
        arch::x86_64::timer::init();
    }

    // 11. Run Phase 3C Preemptive Multitasking Demo
    log::info!("Starting Phase 3C Preemptive Multitasking Demonstration...");
    // SAFETY: Enabling interrupts now that IDT (vector 32/33), PIC, PIT, and scheduler are fully initialized
    x86_64::instructions::interrupts::enable();
    task::run_preemption_demo();
    // Disable interrupts after multitasking demonstration completes
    x86_64::instructions::interrupts::disable();

    // 12. Memory Diagnostics
    let mut usable_bytes: u64 = 0;
    let mut usable_regions: usize = 0;
    let total_regions = boot_info.memory_regions.len();

    for region in boot_info.memory_regions.iter() {
        if region.kind == MemoryRegionKind::Usable {
            usable_regions += 1;
            usable_bytes += region.end - region.start;
        }
    }

    log::info!(
        "Memory Map: {} regions detected ({} usable, {} MiB total usable RAM)",
        total_regions,
        usable_regions,
        usable_bytes / (1024 * 1024)
    );

    if let Some(&phys_offset) = boot_info.physical_memory_offset.as_ref() {
        log::info!("Physical memory mapped at virtual offset: {:#018x}", phys_offset);
    } else {
        log::warn!("Physical memory mapping was not provided by bootloader.");
    }

    if let Some(&rsdp) = boot_info.rsdp_addr.as_ref() {
        log::info!("ACPI RSDP found at physical address: {:#018x}", rsdp);
    }

    // 5. Framebuffer initialization & Cosmic Slate / Obsidian Azure canvas
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        let info = fb.info();
        log::info!(
            "GOP Framebuffer initialized: {}x{} (stride: {}, format: {:?}, size: {} bytes)",
            info.width,
            info.height,
            info.stride,
            info.pixel_format,
            info.byte_len
        );

        let mut writer = framebuffer::FramebufferWriter::new(fb);
        writer.render_boot_screen();
        log::info!("Rendered Cosmic Slate / Obsidian Azure diagnostic splash canvas.");
    } else {
        log::warn!("No linear framebuffer detected; running in headless/serial mode.");
    }

    println!("\x1b[1;32m================================================================");
    println!(" [SUCCESS] Micro-OS Phase 1 Bare-Metal Kernel Boot Complete!    ");
    println!(" CPU entering low-power halt state. Ready for Phase 2.          ");
    println!("================================================================\x1b[0m");

    log::info!("Idle halt loop engaged.");
    arch::x86_64::cpu::hlt_loop();
}
