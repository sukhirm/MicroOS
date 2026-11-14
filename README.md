# Micro-OS: Foundational Architecture (Phase 1)

**Micro-OS** is an educational, bare-metal operating system written entirely in Rust for the **x86_64** architecture. This repository represents a clean, greenfield foundation built from first principles on Windows 11.

---

## 1. Architectural Decisions

### Bootloader Architecture: Modern UEFI with `bootloader_api`
- **Why UEFI?** 
  - Modern PCs, laptops (specifically including the target **HP Victus** hardware platform), and cloud hypervisors have phased out legacy BIOS / CSM in favor of UEFI.
  - UEFI GOP (Graphics Output Protocol) guarantees a standardized 32-bit linear framebuffer across different display hardware. This directly enables our long-term vision of a premium graphical desktop (**Cosmic Slate / Obsidian Azure**). Legacy BIOS text mode (80x25 characters) is fundamentally inadequate for this visual direction.
- **Bootloader Implementation**:
  - The kernel leverages `bootloader_api` (v0.11), which provides a stable, type-safe ABI contract (`BootInfo`) between the bootloader and kernel.
  - The host tool (`runner`) packages the compiled kernel ELF into a bootable GPT/FAT32 UEFI disk image without requiring complex external disk utility toolchains.

### Host / Target Separation
The repository is structured as a clean Cargo workspace:
- **`kernel`**: Bare-metal `#![no_std]` crate compiling for `x86_64-unknown-none`. It has zero host dependencies and interacts purely with hardware and boot structures.
- **`runner`**: Host-side CLI tool (`x86_64-pc-windows-msvc`) that coordinates building the kernel, packaging the bootable disk image, discovering QEMU/OVMF, and running automated test verification.
- **`scripts`**: Developer convenience scripts (`build.ps1`, `run.ps1`) for Windows 11 PowerShell.

---

## 2. Directory Structure

```
MicroOS_New/
├── .cargo/
│   └── (cargo workspace configuration)
├── rust-toolchain.toml          # Nightly channel, rust-src, llvm-tools, x86_64 targets
├── Cargo.toml                   # Workspace manifest (members: kernel, runner)
├── README.md                    # Project documentation
├── kernel/                      # Bare-metal kernel crate
│   ├── Cargo.toml               # Kernel dependencies (x86_64, uart_16550, spin, etc.)
│   ├── .cargo/
│   │   └── config.toml          # target = "x86_64-unknown-none"
│   └── src/
│       ├── main.rs              # Kernel entry point (`kernel_main`) and startup sequence
│       ├── serial.rs            # 16550 UART serial driver (COM1: 0x3F8)
│       ├── logger.rs            # Global kernel logger (routes log facade to serial)
│       ├── framebuffer.rs       # Early GOP framebuffer renderer & Cosmic Slate theme
│       ├── panic.rs             # Safe panic handler with serial diagnostics
│       └── arch/
│           ├── mod.rs
│           └── x86_64/
│               ├── mod.rs
│               └── cpu.rs       # CPU registers (CR0, CR3, CR4, RFLAGS) & halt loop
├── runner/                      # Host-side image builder & QEMU launcher
│   ├── Cargo.toml               # Host dependencies (bootloader, anyhow)
│   └── src/
│       └── main.rs              # Disk image packaging & QEMU process orchestration
└── scripts/
    ├── build.ps1                # PowerShell build script
    └── run.ps1                  # PowerShell run & test script
```

---

## 3. Development Workflow & Commands

### Prerequisites
- **Rust Nightly**: Toolchain and targets (`x86_64-unknown-none`, `x86_64-unknown-uefi`) managed via `rust-toolchain.toml`.
- **QEMU**: `qemu-system-x86_64` (installed at `C:\Program Files\qemu\qemu-system-x86_64.exe` with OVMF firmware `edk2-x86_64-code.fd`).

### Building the Project
To compile the kernel and generate the bootable UEFI image:
```powershell
# Using cargo directly:
cargo run -p runner -- --build

# Or using the PowerShell helper:
.\scripts\build.ps1
```

### Running in QEMU
To run Micro-OS in QEMU:
```powershell
# Interactive GUI mode (displays the Cosmic Slate / Obsidian Azure canvas):
cargo run -p runner

# Headless mode (serial output only):
cargo run -p runner -- --headless

# Automated test & verification mode (exits with code 0 on boot detection):
cargo run -p runner -- --test

# Or using the PowerShell helper:
.\scripts\run.ps1 -Test
```

---

## 4. Phase 1 Verification Results

Boot verified in QEMU 11.1.0 using OVMF UEFI firmware (`edk2-x86_64-code.fd`):

```text
================================================================
             Micro-OS Bare-Metal Kernel (x86_64)                
             Phase 1: Foundational Architecture                 
        Visual Identity: Cosmic Slate / Obsidian Azure          
================================================================
[INFO ] [kernel] Micro-OS kernel startup initiated.
[INFO ] [kernel] API version: 0.11.17
[INFO ] [kernel] CPU State: Long Mode 64-bit verified.
[INFO ] [kernel] CPU Register CR0: Cr0Flags(PROTECTED_MODE_ENABLE | MONITOR_COPROCESSOR | EXTENSION_TYPE | NUMERIC_ERROR | WRITE_PROTECT | PAGING)
[INFO ] [kernel] CPU Register CR3 (PML4 root frame): 0x0000000000101000
[INFO ] [kernel] CPU Register CR4: Cr4Flags(DEBUGGING_EXTENSIONS | PHYSICAL_ADDRESS_EXTENSION | MACHINE_CHECK_EXCEPTION | OSFXSR | OSXMMEXCPT_ENABLE)
[INFO ] [kernel] CPU RFLAGS: RFlags(0x0)
[INFO ] [kernel] Memory Map: 104 regions detected (88 usable, 498 MiB total usable RAM)
[INFO ] [kernel] Physical memory mapped at virtual offset: 0x0000028000000000
[INFO ] [kernel] ACPI RSDP found at physical address: 0x000000001f77e014
[INFO ] [kernel] GOP Framebuffer initialized: 1280x800 (stride: 1280, format: Bgr, size: 4096000 bytes)
[INFO ] [kernel] Rendered Cosmic Slate / Obsidian Azure diagnostic splash canvas.
================================================================
 [SUCCESS] Micro-OS Phase 1 Bare-Metal Kernel Boot Complete!    
 CPU entering low-power halt state. Ready for Phase 2.          
================================================================
[INFO ] [kernel] Idle halt loop engaged.
```

---

## 5. Milestone Status & Next Steps

### What Works:
1. **Clean Bare-Metal Kernel**: Built strictly in Rust `#![no_std]` without external C/assembly dependencies or code reuse.
2. **UEFI Bootloader Handshake**: Seamless transition from UEFI firmware to 64-bit Long Mode kernel.
3. **Hardware Diagnostics**: Real CPU control registers (CR0, CR3 page table base, CR4, RFLAGS) queried and reported.
4. **Physical Memory Discovery**: Memory map parsed (104 regions, 498 MiB usable conventional RAM).
5. **Linear Framebuffer**: 1280x800 GOP framebuffer detected, configured, and painted with the Cosmic Slate / Obsidian Azure boot diagnostic canvas.
6. **16550 UART Serial Logging**: Reliable COM1 (0x3F8) streaming to QEMU stdio and logs.
7. **Automated Verification**: Host test runner executes automated headless boot testing and validates success signatures.

### Remaining for Future Phases:
- **Phase 2**: Global Descriptor Table (GDT), Interrupt Descriptor Table (IDT), PIC/APIC setup, and exception handling.
- **Phase 3**: Physical frame allocator, active page table manipulation, and dynamic kernel heap allocator.
- **Phase 4**: Cooperative and preemptive timer-driven multitasking, Ready Queue, and Round-Robin scheduler.
- **Phase 5**: Graphical compositing engine, window manager, and desktop applications (top status bar, dock, file manager, settings).
- **Phase 6**: Hardware device drivers (PCIe, VirtIO-Net, Wi-Fi / Ethernet), package manager, and bootable ISO production.
