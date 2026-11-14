#![no_std]
#![no_main]

mod arch;
mod framebuffer;
mod logger;
mod panic;
mod serial;

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

    // 4. Memory Diagnostics
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
