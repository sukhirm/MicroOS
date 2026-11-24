use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let is_build_only = args.iter().any(|a| a == "--build");
    let is_test_mode = args.iter().any(|a| a == "--test");
    let is_headless = args.iter().any(|a| a == "--headless") || is_test_mode;
    let is_release = args.iter().any(|a| a == "--release");
    let is_iso_requested = args.iter().any(|a| a == "--iso");

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: cargo run -p runner -- [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --build        Build the disk image without launching QEMU");
        println!("  --release      Build in release mode (creates both .img and .iso)");
        println!("  --iso          Synthesize and validate a genuine Hybrid UEFI bootable ISO image");
        println!("  --test         Run automated boot milestones in headless mode");
        println!("  --headless     Run QEMU with display=none");
        println!("  --display <be> Set QEMU display backend (sdl, gtk, none)");
        return Ok(());
    }

    let display_backend = if is_headless {
        "none".to_string()
    } else if let Some(pos) = args.iter().position(|a| a == "--display") {
        args.get(pos + 1).cloned().unwrap_or_else(|| "sdl".to_string())
    } else {
        "sdl".to_string()
    };

    println!("================================================================");
    println!("             Micro-OS Host Build & Test Runner                  ");
    println!("================================================================");

    // 1. Locate workspace root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().expect("Failed to locate root workspace directory");
    let target_dir = root_dir.join("target");
    let profile = if is_release { "release" } else { "debug" };
    let kernel_bin_path = target_dir
        .join("x86_64-unknown-none")
        .join(profile)
        .join("kernel");

    println!("[Runner] Workspace root: {}", root_dir.display());

    // 2. Build the kernel
    println!("[Runner] Compiling Micro-OS kernel for x86_64-unknown-none (profile: {})...", profile);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build_cmd = Command::new(&cargo);
    build_cmd
        .current_dir(root_dir)
        .arg("build")
        .arg("-p")
        .arg("kernel")
        .arg("--target")
        .arg("x86_64-unknown-none");

    if is_release {
        build_cmd.arg("--release");
    }

    let status = build_cmd.status()?;
    if !status.success() {
        anyhow::bail!("Failed to compile kernel");
    }
    println!("[Runner] Kernel compiled successfully: {}", kernel_bin_path.display());

    // 3. Create bootable UEFI disk image (.img) - Always preserved!
    let uefi_img_path = target_dir.join(format!("micro_os_uefi_{}.img", profile));
    println!("[Runner] Packaging bootable UEFI disk image with bootloader...");
    let uefi_boot = bootloader::UefiBoot::new(&kernel_bin_path);
    uefi_boot.create_disk_image(&uefi_img_path)?;
    println!("[Runner] Bootable UEFI image created: {}", uefi_img_path.display());

    // 3b. Create bootable Hybrid UEFI ISO image (.iso) if requested or in release mode
    if is_iso_requested || is_release {
        let uefi_iso_path = target_dir.join(format!("micro_os_uefi_{}.iso", profile));
        println!("[Runner] Packaging genuine Hybrid UEFI bootable ISO image (El Torito + Hybrid MBR)...");
        let iso_size = runner::iso::create_hybrid_uefi_iso_file(
            &kernel_bin_path,
            &uefi_iso_path,
            "MICRO_OS_LIVE",
        )?;
        println!(
            "[Runner] Hybrid UEFI ISO image created: {} ({} bytes / {:.2} MiB)",
            uefi_iso_path.display(),
            iso_size,
            iso_size as f64 / 1048576.0
        );

        // Run independent structural validation
        println!("[Runner] Running independent structural ISO validation...");
        let report = runner::iso_verify::verify_hybrid_iso_file(&uefi_iso_path)?;
        println!("{}", report.format_summary());
    }

    if is_build_only {
        println!("[Runner] Build completed successfully.");
        return Ok(());
    }

    // 4. Locate QEMU and OVMF firmware
    let qemu_path = find_qemu()?;
    let ovmf_path = find_ovmf()?;

    println!("[Runner] Found QEMU at: {}", qemu_path.display());
    println!("[Runner] Found OVMF firmware at: {}", ovmf_path.display());

    if is_test_mode {
        println!("[Runner] Running in automated verification mode (headless)...");
        let mut qemu_cmd = Command::new(&qemu_path);
        qemu_cmd.arg("-drive").arg(format!("if=pflash,format=raw,readonly=on,file={}", ovmf_path.display()));
        qemu_cmd.arg("-drive").arg(format!("format=raw,file={}", uefi_img_path.display()));
        qemu_cmd.arg("-m").arg("512M");
        qemu_cmd.arg("-serial").arg("stdio");
        qemu_cmd.arg("-display").arg(&display_backend);
        qemu_cmd.stdout(Stdio::piped());
        qemu_cmd.stderr(Stdio::piped());

        let mut child = qemu_cmd.spawn()?;
        let stdout = child.stdout.take().expect("Failed to open child stdout");

        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);
        let start_time = Instant::now();
        let timeout = Duration::from_secs(15);

        struct MilestoneCheck {
            name: &'static str,
            pattern: &'static str,
            passed: bool,
        }

        let mut checks = [
            MilestoneCheck {
                name: "1. Kernel GDT initialization",
                pattern: "Kernel GDT and TSS initialization complete.",
                passed: false,
            },
            MilestoneCheck {
                name: "2. Kernel TSS & Task Register (TR = 0x0018)",
                pattern: "Kernel Task Register (TR): 0x0018",
                passed: false,
            },
            MilestoneCheck {
                name: "3. Kernel IDT initialization",
                pattern: "Kernel IDT initialized (handlers: #BP, #DF with IST1, #GP, #PF).",
                passed: false,
            },
            MilestoneCheck {
                name: "4. Breakpoint (#BP) exception dispatch",
                pattern: "EXCEPTION: BREAKPOINT (#BP)",
                passed: false,
            },
            MilestoneCheck {
                name: "5. Return from breakpoint handler",
                pattern: "Exception verification complete: returned successfully from #BP handler.",
                passed: false,
            },
            MilestoneCheck {
                name: "6. Kernel virtual heap mapped (16 MiB)",
                pattern: "Kernel virtual heap mapped successfully",
                passed: false,
            },
            MilestoneCheck {
                name: "7. Kernel heap allocator registered",
                pattern: "Kernel heap allocator registered and active.",
                passed: false,
            },
            MilestoneCheck {
                name: "8. Box allocation & deallocation test",
                pattern: "[PASS] Box allocation & deallocation test",
                passed: false,
            },
            MilestoneCheck {
                name: "9. Vec allocation & dynamic growth test",
                pattern: "[PASS] Vec allocation & growth test",
                passed: false,
            },
            MilestoneCheck {
                name: "10. String allocation & formatting test",
                pattern: "[PASS] String allocation & formatting test",
                passed: false,
            },
            MilestoneCheck {
                name: "11. 64-byte memory alignment verification",
                pattern: "[PASS] Memory alignment verification",
                passed: false,
            },
            MilestoneCheck {
                name: "12. Multiple allocations & free-list coalescing",
                pattern: "[PASS] Multiple allocations & free-list re-use test",
                passed: false,
            },
            MilestoneCheck {
                name: "13. Allocation failure handling (2 MiB bounds)",
                pattern: "[PASS] Allocation failure handling",
                passed: false,
            },
            MilestoneCheck {
                name: "14. All Phase 3A heap self-tests passed",
                pattern: "[OK] All Phase 3A Heap Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "15. Task ID uniqueness self-test",
                pattern: "[PASS] Task ID uniqueness test",
                passed: false,
            },
            MilestoneCheck {
                name: "16. Task state transition self-test",
                pattern: "[PASS] Task state transition test",
                passed: false,
            },
            MilestoneCheck {
                name: "17. Ready queue FIFO & empty operations self-test",
                pattern: "[PASS] Ready queue FIFO & empty operations test",
                passed: false,
            },
            MilestoneCheck {
                name: "18. Task stack 16-byte alignment & 672B context test",
                pattern: "[PASS] Task stack 16-byte alignment test",
                passed: false,
            },
            MilestoneCheck {
                name: "19. All Phase 3B/3C task self-tests passed",
                pattern: "[OK] All Phase 3B/3C Task Subsystem Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "20. IrqGuard atomic state capture & nesting test",
                pattern: "[PASS] IrqGuard atomic state capture and nested preservation test",
                passed: false,
            },
            MilestoneCheck {
                name: "21. SpinMutex mutual exclusion & try_lock test",
                pattern: "[PASS] SpinMutex mutual exclusion and try_lock contention test",
                passed: false,
            },
            MilestoneCheck {
                name: "22. Semaphore bounded capacity test",
                pattern: "[PASS] Semaphore bounded capacity and atomic acquire/release test",
                passed: false,
            },
            MilestoneCheck {
                name: "23. All Phase 3C synchronization self-tests passed",
                pattern: "[OK] All Phase 3C Synchronization Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "24. 8259 PIC initialized and IRQ0 unmasked",
                pattern: "[PIC] 8259 PIC initialized",
                passed: false,
            },
            MilestoneCheck {
                name: "25. 8254 PIT Mode 3 square wave at 100 Hz",
                pattern: "[PIT] 8254 PIT initialized: Mode 3 square wave at 100 Hz",
                passed: false,
            },
            MilestoneCheck {
                name: "26. Task Alpha compute-bound loop started",
                pattern: "Started compute-bound loop WITHOUT voluntary yields...",
                passed: false,
            },
            MilestoneCheck {
                name: "27. Task Beta preempted Task Alpha mid-computation",
                pattern: "EXECUTING! Successfully preempted compute-bound Task Alpha",
                passed: false,
            },
            MilestoneCheck {
                name: "28. Non-yielding task preemption verified",
                pattern: "[PASS] Non-yielding task preemption verified",
                passed: false,
            },
            MilestoneCheck {
                name: "29. Concurrent SpinMutex counter verified (2000)",
                pattern: "[PASS] SpinMutex mutual exclusion verified (shared counter reached exactly 2000).",
                passed: false,
            },
            MilestoneCheck {
                name: "30. Semaphore cooperative acquire/release verified",
                pattern: "[PASS] Semaphore cooperative acquire & release verified.",
                passed: false,
            },
            MilestoneCheck {
                name: "31. Preemptive multitasking demo complete",
                pattern: "[OK] Preemptive Multitasking Demonstration Complete!",
                passed: false,
            },
            MilestoneCheck {
                name: "32. Return to main kernel thread",
                pattern: "[Scheduler] All worker tasks completed. Execution returned to main thread.",
                passed: false,
            },
            MilestoneCheck {
                name: "33. Phase 1 kernel boot completion banner",
                pattern: "[SUCCESS] Micro-OS Phase 1 Bare-Metal Kernel Boot Complete!",
                passed: false,
            },
            MilestoneCheck {
                name: "34. Surface geometry & boundary detection test",
                pattern: "[PASS] 1. Surface geometry, stride, and boundary detection test",
                passed: false,
            },
            MilestoneCheck {
                name: "35. Pixel rendering & format translation (RGB/BGR) test",
                pattern: "[PASS] 2. Pixel rendering, format translation (RGB/BGR), and boundary safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "36. Clipping rectangle enforcement test",
                pattern: "[PASS] 3. Clipping rectangle enforcement and zero-bleed validation test",
                passed: false,
            },
            MilestoneCheck {
                name: "37. Solid & stroked rectangle rendering test",
                pattern: "[PASS] 4. Solid rectangle filling and stroked outline rendering test",
                passed: false,
            },
            MilestoneCheck {
                name: "38. Line drawing (Bresenham) test",
                pattern: "[PASS] 5. Line drawing (horizontal, vertical, Bresenham diagonal) test",
                passed: false,
            },
            MilestoneCheck {
                name: "39. Rounded rectangle geometry test",
                pattern: "[PASS] 6. Rounded rectangle geometry and corner curvature test",
                passed: false,
            },
            MilestoneCheck {
                name: "40. Scaled text rendering & typography measurement test",
                pattern: "[PASS] 7. Text rendering, integer scaling (1x/2x), and typography measurement test",
                passed: false,
            },
            MilestoneCheck {
                name: "41. Embedded icon transparency test",
                pattern: "[PASS] 8. Embedded bitmap icon rendering and transparency preservation test",
                passed: false,
            },
            MilestoneCheck {
                name: "42. Dedicated 16 MiB backbuffer mapping & multi-resolution blit test",
                pattern: "[PASS] 9. Dedicated 16 MiB backbuffer mapping, multi-resolution (1280x800, 1920x1080) and bounds test",
                passed: false,
            },
            MilestoneCheck {
                name: "43. All Phase 4A graphics self-tests passed",
                pattern: "[OK] All Phase 4A Graphics Engine Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "44. Double-buffered graphics showcase screen rendered",
                pattern: "Rendered Phase 4A Graphics Engine demonstration screen (double-buffered).",
                passed: false,
            },
            MilestoneCheck {
                name: "45. Wallpaper cosmic canvas & grid test",
                pattern: "[PASS] 1. Wallpaper cosmic canvas, grid, and geometric accents test",
                passed: false,
            },
            MilestoneCheck {
                name: "46. Status bar geometry & honest metrics test",
                pattern: "[PASS] 2. Status bar geometry (34px), top placement, and honest metrics test",
                passed: false,
            },
            MilestoneCheck {
                name: "47. Floating dock positioning & centering test",
                pattern: "[PASS] 3. Floating dock positioning, horizontal centering, and geometry test",
                passed: false,
            },
            MilestoneCheck {
                name: "48. Dock 6-application icon mapping test",
                pattern: "[PASS] 4. Dock 6-application icon mapping, card slots, and labels test",
                passed: false,
            },
            MilestoneCheck {
                name: "49. Mouse cursor 12x18 sprite bounds & clipping test",
                pattern: "[PASS] 5. Mouse cursor 12x18 sprite bounds, clipping, and restore test",
                passed: false,
            },
            MilestoneCheck {
                name: "50. Desktop resolution adaptation & stride test",
                pattern: "[PASS] 6. Desktop shell resolution adaptation & stride compatibility test",
                passed: false,
            },
            MilestoneCheck {
                name: "51. Desktop shell multi-layer composite test",
                pattern: "[PASS] 7. Desktop shell multi-layer composite integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "52. All Phase 4B desktop self-tests passed",
                pattern: "[OK] All Phase 4B Desktop Shell Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "53. Desktop environment shell rendered",
                pattern: "Rendered Phase 4B Desktop Environment Shell (wallpaper, status bar, dock, cursor).",
                passed: false,
            },
            MilestoneCheck {
                name: "54. Window creation & default state test",
                pattern: "[PASS] 1. Window creation, bounds, and default state test",
                passed: false,
            },
            MilestoneCheck {
                name: "55. Unique window ID generation test",
                pattern: "[PASS] 2. Unique window ID generation test",
                passed: false,
            },
            MilestoneCheck {
                name: "56. Window closing & collection purge test",
                pattern: "[PASS] 3. Window closing, state transition, and collection purge test",
                passed: false,
            },
            MilestoneCheck {
                name: "57. Window minimize & restore test",
                pattern: "[PASS] 4. Window minimize and restore test",
                passed: false,
            },
            MilestoneCheck {
                name: "58. Window maximize & restore test",
                pattern: "[PASS] 5. Window maximize and restore dimensions test",
                passed: false,
            },
            MilestoneCheck {
                name: "59. Active & inactive window focus test",
                pattern: "[PASS] 6. Active and inactive window focus states test",
                passed: false,
            },
            MilestoneCheck {
                name: "60. Z-order reordering & bring-to-front test",
                pattern: "[PASS] 7. Z-order stack reordering and bring-to-front test",
                passed: false,
            },
            MilestoneCheck {
                name: "61. Window movement & geometry test",
                pattern: "[PASS] 8. Window movement and geometry recalculation test",
                passed: false,
            },
            MilestoneCheck {
                name: "62. Window resizing test",
                pattern: "[PASS] 9. Window resizing test",
                passed: false,
            },
            MilestoneCheck {
                name: "63. Minimum dimensions enforcement test",
                pattern: "[PASS] 10. Minimum window dimensions enforcement test",
                passed: false,
            },
            MilestoneCheck {
                name: "64. Window coordinate hit-testing test",
                pattern: "[PASS] 11. Window coordinate hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "65. Title bar boundaries & button exclusion test",
                pattern: "[PASS] 12. Title bar boundaries and button exclusion test",
                passed: false,
            },
            MilestoneCheck {
                name: "66. Control button hit-testing (Min/Max/Close) test",
                pattern: "[PASS] 13. Control button hit-testing (Min, Max, Close) test",
                passed: false,
            },
            MilestoneCheck {
                name: "67. Overlapping window hit-test & Z-order test",
                pattern: "[PASS] 14. Overlapping window hit-test and Z-order priority test",
                passed: false,
            },
            MilestoneCheck {
                name: "68. Screen boundary clipping & buffer safety test",
                pattern: "[PASS] 15. Screen boundary clipping and buffer safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "69. Minimized window render omission test",
                pattern: "[PASS] 16. Minimized window omission from render pass test",
                passed: false,
            },
            MilestoneCheck {
                name: "70. Closed window render omission test",
                pattern: "[PASS] 17. Closed window omission from render pass test",
                passed: false,
            },
            MilestoneCheck {
                name: "71. Multi-window composite rendering test",
                pattern: "[PASS] 18. Multi-window composite rendering and focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "72. All Phase 4C window manager self-tests passed",
                pattern: "[OK] All Phase 4C Window Manager Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "73. Window manager desktop demonstration screen rendered",
                pattern: "Rendered Phase 4C Window Manager demonstration screen (3 sample windows, Z-stacked).",
                passed: false,
            },
            MilestoneCheck {
                name: "74. Keyboard scancode translation test",
                pattern: "[PASS] 1. Keyboard scancode character translation test",
                passed: false,
            },
            MilestoneCheck {
                name: "75. Key press & release tracking test",
                pattern: "[PASS] 2. Key press and release tracking test",
                passed: false,
            },
            MilestoneCheck {
                name: "76. Keyboard modifier state tracking test",
                pattern: "[PASS] 3. Keyboard modifier state tracking test",
                passed: false,
            },
            MilestoneCheck {
                name: "77. Keyboard Shift & Caps Lock logic test",
                pattern: "[PASS] 4. Keyboard Shift and Caps Lock combination logic test",
                passed: false,
            },
            MilestoneCheck {
                name: "78. Keyboard extended scancode decoding test",
                pattern: "[PASS] 5. Keyboard extended scancode decoding test",
                passed: false,
            },
            MilestoneCheck {
                name: "79. Mouse packet boundary synchronization test",
                pattern: "[PASS] 6. Mouse packet boundary synchronization test",
                passed: false,
            },
            MilestoneCheck {
                name: "80. Mouse signed movement & Y-inversion test",
                pattern: "[PASS] 7. Mouse signed movement decoding and coordinate inversion test",
                passed: false,
            },
            MilestoneCheck {
                name: "81. Mouse overflow bit suppression test",
                pattern: "[PASS] 8. Mouse overflow bit suppression test",
                passed: false,
            },
            MilestoneCheck {
                name: "82. Mouse button transition detection test",
                pattern: "[PASS] 9. Mouse button state transition detection test",
                passed: false,
            },
            MilestoneCheck {
                name: "83. Mouse cursor boundary clamping test",
                pattern: "[PASS] 10. Mouse cursor screen boundary clamping test",
                passed: false,
            },
            MilestoneCheck {
                name: "84. Event queue FIFO operations test",
                pattern: "[PASS] 11. Event queue FIFO insertion and retrieval test",
                passed: false,
            },
            MilestoneCheck {
                name: "85. Event queue bounded capacity & overrun test",
                pattern: "[PASS] 12. Event queue bounded capacity and overrun safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "86. Event queue ordering fidelity test",
                pattern: "[PASS] 13. Event queue sequential ordering fidelity test",
                passed: false,
            },
            MilestoneCheck {
                name: "87. Desktop window activation via click test",
                pattern: "[PASS] 14. Desktop window activation via mouse click test",
                passed: false,
            },
            MilestoneCheck {
                name: "88. Desktop window dragging via title bar test",
                pattern: "[PASS] 15. Desktop window dragging via title bar test",
                passed: false,
            },
            MilestoneCheck {
                name: "89. Window control buttons click dispatch test",
                pattern: "[PASS] 16. Window control buttons (Min, Max, Close) click dispatch test",
                passed: false,
            },
            MilestoneCheck {
                name: "90. Desktop application dock hit-testing test",
                pattern: "[PASS] 17. Desktop application dock hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "91. Desktop keyboard shortcut dispatch test",
                pattern: "[PASS] 18. Desktop keyboard shortcut dispatch (Alt+Tab, Escape) test",
                passed: false,
            },
            MilestoneCheck {
                name: "92. All Phase 4D input self-tests passed",
                pattern: "[OK] All Phase 4D Input Subsystem Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "93. 8042 PS/2 controller initialized",
                pattern: "[PS/2] 8042 Controller initialized successfully (IRQ1 & IRQ12 active).",
                passed: false,
            },
            MilestoneCheck {
                name: "94. Input IRQs enabled on 8259 PIC",
                pattern: "[PIC] Input IRQs enabled: IRQ1 (Keyboard), IRQ2 (Cascade), IRQ12 (Mouse).",
                passed: false,
            },
            MilestoneCheck {
                name: "95. Interactive desktop event loop engaged",
                pattern: "Desktop Event Loop engaged.",
                passed: false,
            },
            MilestoneCheck {
                name: "96. VFS initial directory listing test",
                pattern: "[PASS] 1. Initial VFS directory listing and sample tree test",
                passed: false,
            },
            MilestoneCheck {
                name: "97. VFS path canonicalization test",
                pattern: "[PASS] 2. VFS path canonicalization & relative navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "98. VFS hierarchical directory navigation test",
                pattern: "[PASS] 3. VFS hierarchical directory navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "99. VFS directory creation & empty directory test",
                pattern: "[PASS] 4. VFS directory creation & empty directory handling test",
                passed: false,
            },
            MilestoneCheck {
                name: "100. VFS file creation, reading, and overwriting test",
                pattern: "[PASS] 5. VFS file creation, reading, and overwriting test",
                passed: false,
            },
            MilestoneCheck {
                name: "101. VFS safe file & directory deletion test",
                pattern: "[PASS] 6. VFS safe file & directory deletion test",
                passed: false,
            },
            MilestoneCheck {
                name: "102. VFS boundary enforcement & invalid path test",
                pattern: "[PASS] 7. VFS boundary enforcement & invalid path rejection test",
                passed: false,
            },
            MilestoneCheck {
                name: "103. File Manager initial state & directory listing test",
                pattern: "[PASS] 1. File Manager initial state & directory listing test",
                passed: false,
            },
            MilestoneCheck {
                name: "104. File Manager folder navigation & validation test",
                pattern: "[PASS] 2. File Manager folder navigation & validation test",
                passed: false,
            },
            MilestoneCheck {
                name: "105. File Manager Back, Forward, Up history test",
                pattern: "[PASS] 3. File Manager Back, Forward, and Up navigation history test",
                passed: false,
            },
            MilestoneCheck {
                name: "106. File Manager sidebar shortcut navigation test",
                pattern: "[PASS] 4. File Manager sidebar shortcut navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "107. File Manager mouse click hit-testing test",
                pattern: "[PASS] 5. File Manager mouse click hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "108. File Manager keyboard navigation test",
                pattern: "[PASS] 6. File Manager keyboard navigation (Arrows, Enter, Backspace) test",
                passed: false,
            },
            MilestoneCheck {
                name: "109. File Manager empty directory presentation test",
                pattern: "[PASS] 7. File Manager empty directory handling & presentation test",
                passed: false,
            },
            MilestoneCheck {
                name: "110. File Manager window manager registration test",
                pattern: "[PASS] 8. File Manager window manager registration & focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "111. File Manager launched on desktop",
                pattern: "Rendered Phase 4E Desktop with active File Manager window.",
                passed: false,
            },
            MilestoneCheck {
                name: "112. Settings initial state & default preferences test",
                pattern: "[PASS] 1. Settings initial state & default preferences test",
                passed: false,
            },
            MilestoneCheck {
                name: "113. Settings section navigation & metadata consistency test",
                pattern: "[PASS] 2. Settings section navigation & metadata consistency test",
                passed: false,
            },
            MilestoneCheck {
                name: "114. Settings accent colour selection & palette mapping test",
                pattern: "[PASS] 3. Settings accent colour selection & palette mapping test",
                passed: false,
            },
            MilestoneCheck {
                name: "115. Settings toggle switch state inversion test",
                pattern: "[PASS] 4. Settings toggle switch state inversion test",
                passed: false,
            },
            MilestoneCheck {
                name: "116. Settings display information & backbuffer capacity test",
                pattern: "[PASS] 5. Settings display information & backbuffer capacity test",
                passed: false,
            },
            MilestoneCheck {
                name: "117. Settings input subsystem info & queue capacity test",
                pattern: "[PASS] 6. Settings input subsystem info & queue capacity test",
                passed: false,
            },
            MilestoneCheck {
                name: "118. Settings sidebar mouse hit-testing test",
                pattern: "[PASS] 7. Settings sidebar mouse hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "119. Settings appearance controls mouse hit-testing test",
                pattern: "[PASS] 8. Settings appearance controls mouse hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "120. Settings keyboard navigation (Arrows & cycling) test",
                pattern: "[PASS] 9. Settings keyboard navigation (Arrows & cycling) test",
                passed: false,
            },
            MilestoneCheck {
                name: "121. Settings window manager registration & focus integration test",
                pattern: "[PASS] 10. Settings window manager registration & focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "122. Settings rendering clipping safety test",
                pattern: "[PASS] 11. Settings rendering clipping safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "123. All Phase 4E Settings self-tests passed",
                pattern: "[OK] All Phase 4E Settings Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "124. System Monitor initial state test",
                pattern: "[PASS] 1. System Monitor initial state test",
                passed: false,
            },
            MilestoneCheck {
                name: "125. System Monitor section navigation & title consistency test",
                pattern: "[PASS] 2. System Monitor section navigation & title consistency test",
                passed: false,
            },
            MilestoneCheck {
                name: "126. System Monitor system info & timer telemetry test",
                pattern: "[PASS] 3. System Monitor system info & timer telemetry test",
                passed: false,
            },
            MilestoneCheck {
                name: "127. System Monitor memory telemetry & heap capacity test",
                pattern: "[PASS] 4. System Monitor memory telemetry & heap capacity test",
                passed: false,
            },
            MilestoneCheck {
                name: "128. System Monitor task listing & snapshot collection test",
                pattern: "[PASS] 5. System Monitor task listing & snapshot collection test",
                passed: false,
            },
            MilestoneCheck {
                name: "129. System Monitor scheduler performance counters test",
                pattern: "[PASS] 6. System Monitor scheduler performance counters test",
                passed: false,
            },
            MilestoneCheck {
                name: "130. System Monitor sidebar mouse hit-testing test",
                pattern: "[PASS] 7. System Monitor sidebar mouse hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "131. System Monitor keyboard navigation test",
                pattern: "[PASS] 8. System Monitor keyboard navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "132. System Monitor window manager registration & focus integration test",
                pattern: "[PASS] 9. System Monitor window manager registration & focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "133. System Monitor rendering clipping safety test",
                pattern: "[PASS] 10. System Monitor rendering clipping safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "134. All Phase 4E System Monitor self-tests passed",
                pattern: "[OK] All Phase 4E System Monitor Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "135. PCI config space address calculation test",
                pattern: "[PASS] 1. PCI config space address calculation test",
                passed: false,
            },
            MilestoneCheck {
                name: "136. PCI network controller classification test",
                pattern: "[PASS] 2. PCI network controller classification test",
                passed: false,
            },
            MilestoneCheck {
                name: "137. Realtek RTL8852BE-VT & RTL8168 device identification test",
                pattern: "[PASS] 3. Realtek RTL8852BE-VT & RTL8168 device identification test",
                passed: false,
            },
            MilestoneCheck {
                name: "138. Network adapter driver state lifecycle test",
                pattern: "[PASS] 4. Network adapter driver state lifecycle test",
                passed: false,
            },
            MilestoneCheck {
                name: "139. Network connection state integrity & no fake connectivity test",
                pattern: "[PASS] 5. Network connection state integrity & no fake connectivity test",
                passed: false,
            },
            MilestoneCheck {
                name: "140. Invalid PCI device rejection test",
                pattern: "[PASS] 6. Invalid PCI device rejection test",
                passed: false,
            },
            MilestoneCheck {
                name: "141. Network Manager initial state test",
                pattern: "[PASS] 7. Network Manager initial state test",
                passed: false,
            },
            MilestoneCheck {
                name: "142. Network Manager section navigation & title consistency test",
                pattern: "[PASS] 8. Network Manager section navigation & title consistency test",
                passed: false,
            },
            MilestoneCheck {
                name: "143. Network Manager sidebar mouse hit-testing test",
                pattern: "[PASS] 9. Network Manager sidebar mouse hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "144. Network Manager rescan control click test",
                pattern: "[PASS] 10. Network Manager rescan control click test",
                passed: false,
            },
            MilestoneCheck {
                name: "145. Network Manager keyboard navigation test",
                pattern: "[PASS] 11. Network Manager keyboard navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "146. Network Manager window manager registration & focus integration test",
                pattern: "[PASS] 12. Network Manager window manager registration & focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "147. Network Manager rendering clipping safety test",
                pattern: "[PASS] 13. Network Manager rendering clipping safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "148. All Phase 4E Network Manager self-tests passed",
                pattern: "[OK] All Phase 4E Network Manager Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "149. Terminal initial state & prompt format test",
                pattern: "[PASS] 1. Terminal initial state & prompt format test",
                passed: false,
            },
            MilestoneCheck {
                name: "150. Terminal path resolution & token parsing test",
                pattern: "[PASS] 2. Terminal path resolution & token parsing test",
                passed: false,
            },
            MilestoneCheck {
                name: "151. Terminal empty input handling test",
                pattern: "[PASS] 3. Terminal empty input handling test",
                passed: false,
            },
            MilestoneCheck {
                name: "152. Terminal unknown command rejection test",
                pattern: "[PASS] 4. Terminal unknown command rejection test",
                passed: false,
            },
            MilestoneCheck {
                name: "153. Terminal pwd and ls command execution test",
                pattern: "[PASS] 5. Terminal pwd and ls command execution test",
                passed: false,
            },
            MilestoneCheck {
                name: "154. Terminal cd directory navigation test",
                pattern: "[PASS] 6. Terminal cd directory navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "155. Terminal mkdir and touch VFS integration test",
                pattern: "[PASS] 7. Terminal mkdir and touch VFS integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "156. Terminal cat file reading test",
                pattern: "[PASS] 8. Terminal cat file reading test",
                passed: false,
            },
            MilestoneCheck {
                name: "157. Terminal rm file and directory deletion test",
                pattern: "[PASS] 9. Terminal rm file and directory deletion test",
                passed: false,
            },
            MilestoneCheck {
                name: "158. Terminal command history navigation test",
                pattern: "[PASS] 10. Terminal command history navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "159. Terminal system telemetry & utility commands test",
                pattern: "[PASS] 11. Terminal system telemetry & utility commands test",
                passed: false,
            },
            MilestoneCheck {
                name: "160. Terminal date & reboot hardware honesty warnings test",
                pattern: "[PASS] 12. Terminal date & reboot hardware honesty warnings test",
                passed: false,
            },
            MilestoneCheck {
                name: "161. Terminal window manager registration & focus integration test",
                pattern: "[PASS] 13. Terminal window manager registration & focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "162. Terminal rendering clipping safety test",
                pattern: "[PASS] 14. Terminal rendering clipping safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "163. All Phase 4E Terminal self-tests passed",
                pattern: "[OK] All Phase 4E Terminal Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "164. Package metadata validation & size formatting test",
                pattern: "[PASS] 1. Package metadata validation & size formatting test",
                passed: false,
            },
            MilestoneCheck {
                name: "165. Unique package IDs enforcement test",
                pattern: "[PASS] 2. Unique package IDs enforcement test",
                passed: false,
            },
            MilestoneCheck {
                name: "166. Search query filtering test",
                pattern: "[PASS] 3. Search query filtering test",
                passed: false,
            },
            MilestoneCheck {
                name: "167. Category tab filtering test",
                pattern: "[PASS] 4. Category tab filtering test",
                passed: false,
            },
            MilestoneCheck {
                name: "168. Built-in vs. available repository status distinction test",
                pattern: "[PASS] 5. Built-in vs. available repository status distinction test",
                passed: false,
            },
            MilestoneCheck {
                name: "169. Invalid package metadata rejection test",
                pattern: "[PASS] 6. Invalid package metadata rejection test",
                passed: false,
            },
            MilestoneCheck {
                name: "170. Installation safety transition & anti-fabrication test",
                pattern: "[PASS] 7. Installation safety transition & anti-fabrication test",
                passed: false,
            },
            MilestoneCheck {
                name: "171. Uninstallation built-in component protection test",
                pattern: "[PASS] 8. Uninstallation built-in component protection test",
                passed: false,
            },
            MilestoneCheck {
                name: "172. Dependency metadata verification test",
                pattern: "[PASS] 9. Dependency metadata verification test",
                passed: false,
            },
            MilestoneCheck {
                name: "173. Empty catalogue search handling test",
                pattern: "[PASS] 10. Empty catalogue search handling test",
                passed: false,
            },
            MilestoneCheck {
                name: "174. Store initial state & tab navigation test",
                pattern: "[PASS] 11. Store initial state & tab navigation test",
                passed: false,
            },
            MilestoneCheck {
                name: "175. Store mouse click hit-testing test",
                pattern: "[PASS] 12. Store mouse click hit-testing test",
                passed: false,
            },
            MilestoneCheck {
                name: "176. Store window manager registration & focus integration test",
                pattern: "[PASS] 13. Store window manager registration & focus integration test",
                passed: false,
            },
            MilestoneCheck {
                name: "177. Store rendering clipping safety test",
                pattern: "[PASS] 14. Store rendering clipping safety test",
                passed: false,
            },
            MilestoneCheck {
                name: "178. All Phase 4E Software Store self-tests passed",
                pattern: "[OK] All Phase 4E Software Store Self-Tests Passed!",
                passed: false,
            },
            MilestoneCheck {
                name: "179. Package registration and lookup verified",
                pattern: "[PASS] 1. Package registration and lookup verified",
                passed: false,
            },
            MilestoneCheck {
                name: "180. Duplicate package ID rejection verified",
                pattern: "[PASS] 2. Duplicate package ID rejection verified",
                passed: false,
            },
            MilestoneCheck {
                name: "181. Package metadata validation verified",
                pattern: "[PASS] 3. Package metadata validation verified",
                passed: false,
            },
            MilestoneCheck {
                name: "182. Search filtering across metadata verified",
                pattern: "[PASS] 4. Search filtering across metadata verified",
                passed: false,
            },
            MilestoneCheck {
                name: "183. Installed and built-in packages listing verified",
                pattern: "[PASS] 5. Installed and built-in packages listing verified",
                passed: false,
            },
            MilestoneCheck {
                name: "184. Dependency resolution for satisfied dependencies verified",
                pattern: "[PASS] 6. Dependency resolution for satisfied dependencies verified",
                passed: false,
            },
            MilestoneCheck {
                name: "185. Missing dependency detection verified",
                pattern: "[PASS] 7. Missing dependency detection verified",
                passed: false,
            },
            MilestoneCheck {
                name: "186. Cyclic dependency detection verified",
                pattern: "[PASS] 8. Cyclic dependency detection verified",
                passed: false,
            },
            MilestoneCheck {
                name: "187. Protected built-in removal rejection verified",
                pattern: "[PASS] 9. Protected built-in removal rejection verified",
                passed: false,
            },
            MilestoneCheck {
                name: "188. Invalid package name error handling verified",
                pattern: "[PASS] 10. Invalid package name error handling verified",
                passed: false,
            },
            MilestoneCheck {
                name: "189. Installation failure handling and rollback verified",
                pattern: "[PASS] 11. Installation failure handling and rollback verified",
                passed: false,
            },
            MilestoneCheck {
                name: "190. Bundled package installation and removal verified",
                pattern: "[PASS] 12. Bundled package installation and removal verified",
                passed: false,
            },
            MilestoneCheck {
                name: "191. Repository offline honest error reporting verified",
                pattern: "[PASS] 13. Repository offline honest error reporting verified",
                passed: false,
            },
            MilestoneCheck {
                name: "192. Database maximum capacity boundary (64 pkgs) verified",
                pattern: "[PASS] 14. Database maximum capacity boundary (64 pkgs) verified",
                passed: false,
            },
            MilestoneCheck {
                name: "193. Terminal APT CLI commands output verified",
                pattern: "[PASS] 15. Terminal APT CLI commands output verified",
                passed: false,
            },
            MilestoneCheck {
                name: "194. Software Store and Package DB synchronization verified",
                pattern: "[PASS] 16. Software Store and Package DB synchronization verified",
                passed: false,
            },
            MilestoneCheck {
                name: "195. All Phase 4E.7 MicroOS APT self-tests passed",
                pattern: "[OK] All Phase 4E.7 MicroOS APT Self-Tests Passed!",
                passed: false,
            },
        ];

        let mut captured_lines = Vec::new();

        for line_result in reader.lines() {
            if let Ok(line) = line_result {
                println!("{}", line);
                for check in &mut checks {
                    if !check.passed && line.contains(check.pattern) {
                        check.passed = true;
                    }
                }
                captured_lines.push(line);
            }

            let all_passed = checks.iter().all(|c| c.passed);
            if all_passed || start_time.elapsed() > timeout {
                break;
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        println!("\n================================================================");
        println!("             Micro-OS Automated Verification Summary            ");
        println!("================================================================");
        let mut all_passed = true;
        for check in &checks {
            if check.passed {
                println!("  \x1b[1;32m[PASS]\x1b[0m {}", check.name);
            } else {
                println!("  \x1b[1;31m[FAIL]\x1b[0m {} (Expected pattern: {:?})", check.name, check.pattern);
                all_passed = false;
            }
        }
        println!("================================================================");

        if all_passed {
            println!("\x1b[1;32m[Runner Test] ALL CHECKS PASSED: Phase 1 through Phase 4E.7 (MicroOS APT Package Manager) Verified.\x1b[0m");
            println!("================================================================");
            Ok(())
        } else {
            let failed_count = checks.iter().filter(|c| !c.passed).count();
            anyhow::bail!("Automated verification failed: {}/{} milestone checks failed", failed_count, checks.len());
        }
    } else {
        println!("[Runner] Launching QEMU with graphical window (display: {})...", display_backend);
        #[cfg(windows)]
        {
            win32_launcher::run_qemu_interactive_windows(
                &qemu_path,
                &ovmf_path,
                &uefi_img_path,
                &display_backend,
            )?;
        }
        #[cfg(not(windows))]
        {
            let mut qemu_cmd = Command::new(&qemu_path);
            qemu_cmd.arg("-drive").arg(format!("if=pflash,format=raw,readonly=on,file={}", ovmf_path.display()));
            qemu_cmd.arg("-drive").arg(format!("format=raw,file={}", uefi_img_path.display()));
            qemu_cmd.arg("-m").arg("512M");
            qemu_cmd.arg("-serial").arg("stdio");
            qemu_cmd.arg("-display").arg(&display_backend);
            let mut child = qemu_cmd.spawn()?;
            let _ = child.wait();
        }
        Ok(())
    }
}

#[cfg(windows)]
#[allow(non_snake_case, dead_code)]
mod win32_launcher {
    use std::ffi::c_void;
    use std::path::Path;
    use std::time::Duration;

    type HWND = isize;
    type HANDLE = isize;
    type BOOL = i32;
    type DWORD = u32;
    type WORD = u16;
    type LPWSTR = *mut u16;
    type LPCWSTR = *const u16;
    type LPARAM = isize;
    type WNDENUMPROC = unsafe extern "system" fn(HWND, LPARAM) -> BOOL;

    #[repr(C)]
    struct SECURITY_ATTRIBUTES {
        nLength: DWORD,
        lpSecurityDescriptor: *mut c_void,
        bInheritHandle: BOOL,
    }

    #[repr(C)]
    struct STARTUPINFOW {
        cb: DWORD,
        lpReserved: LPWSTR,
        lpDesktop: LPWSTR,
        lpTitle: LPWSTR,
        dwX: DWORD,
        dwY: DWORD,
        dwXSize: DWORD,
        dwYSize: DWORD,
        dwXCountChars: DWORD,
        dwYCountChars: DWORD,
        dwFillAttribute: DWORD,
        dwFlags: DWORD,
        wShowWindow: WORD,
        cbReserved2: WORD,
        lpReserved2: *mut u8,
        hStdInput: HANDLE,
        hStdOutput: HANDLE,
        hStdError: HANDLE,
    }

    #[repr(C)]
    struct PROCESS_INFORMATION {
        hProcess: HANDLE,
        hThread: HANDLE,
        dwProcessId: DWORD,
        dwThreadId: DWORD,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    const STD_INPUT_HANDLE: i32 = -10;
    const STARTF_USESTDHANDLES: u32 = 0x0000_0100;
    const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;
    const SW_HIDE: i32 = 0;
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;
    const HWND_TOPMOST: HWND = -1;
    const HWND_NOTOPMOST: HWND = -2;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const INFINITE: u32 = 0xFFFF_FFFF;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreatePipe(
            hReadPipe: *mut HANDLE,
            hWritePipe: *mut HANDLE,
            lpPipeAttributes: *mut SECURITY_ATTRIBUTES,
            nSize: DWORD,
        ) -> BOOL;
        fn SetHandleInformation(hObject: HANDLE, dwMask: DWORD, dwFlags: DWORD) -> BOOL;
        fn GetStdHandle(nStdHandle: i32) -> HANDLE;
        fn CreateProcessW(
            lpApplicationName: LPCWSTR,
            lpCommandLine: LPWSTR,
            lpProcessAttributes: *mut c_void,
            lpThreadAttributes: *mut c_void,
            bInheritHandles: BOOL,
            dwCreationFlags: DWORD,
            lpEnvironment: *mut c_void,
            lpCurrentDirectory: LPCWSTR,
            lpStartupInfo: *mut STARTUPINFOW,
            lpProcessInformation: *mut PROCESS_INFORMATION,
        ) -> BOOL;
        fn ReadFile(
            hFile: HANDLE,
            lpBuffer: *mut c_void,
            nNumberOfBytesToRead: DWORD,
            lpNumberOfBytesRead: *mut DWORD,
            lpOverlapped: *mut c_void,
        ) -> BOOL;
        fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
        fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
        fn CloseHandle(hObject: HANDLE) -> BOOL;
        fn GetLastError() -> DWORD;
    }

    #[link(name = "user32")]
    extern "system" {
        fn OpenDesktopW(
            lpszDesktop: LPCWSTR,
            dwFlags: DWORD,
            fInherit: BOOL,
            dwDesiredAccess: DWORD,
        ) -> HANDLE;
        fn EnumDesktopWindows(hDesktop: HANDLE, lpfn: WNDENUMPROC, lParam: LPARAM) -> BOOL;
        fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut DWORD) -> DWORD;
        fn GetClassNameW(hWnd: HWND, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
        fn SetThreadDesktop(hDesktop: HANDLE) -> BOOL;
        fn MoveWindow(hWnd: HWND, X: i32, Y: i32, nWidth: i32, nHeight: i32, bRepaint: BOOL) -> BOOL;
        fn SetWindowPos(
            hWnd: HWND,
            hWndInsertAfter: HWND,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> BOOL;
        fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
        fn SetForegroundWindow(hWnd: HWND) -> BOOL;
        fn BringWindowToTop(hWnd: HWND) -> BOOL;
        fn SwitchToThisWindow(hWnd: HWND, fUnknown: BOOL);
        fn CloseDesktop(hDesktop: HANDLE) -> BOOL;
        fn GetSystemMetrics(nIndex: i32) -> i32;
        fn IsWindowVisible(hWnd: HWND) -> BOOL;
    }

    fn quote_arg(s: &str) -> String {
        if s.contains(' ') || s.contains('\t') || s.is_empty() {
            format!("\"{}\"", s.replace('\"', "\\\""))
        } else {
            s.to_string()
        }
    }

    pub fn run_qemu_interactive_windows(
        qemu_path: &Path,
        ovmf_path: &Path,
        uefi_img_path: &Path,
        display_backend: &str,
    ) -> anyhow::Result<()> {
        let args = [
            qemu_path.to_string_lossy().to_string(),
            "-drive".to_string(),
            format!("if=pflash,format=raw,readonly=on,file={}", ovmf_path.display()),
            "-drive".to_string(),
            format!("format=raw,file={}", uefi_img_path.display()),
            "-m".to_string(),
            "512M".to_string(),
            "-serial".to_string(),
            "stdio".to_string(),
            "-display".to_string(),
            display_backend.to_string(),
        ];

        let cmd_line = args.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" ");
        let mut cmd_line_wide: Vec<u16> = cmd_line.encode_utf16().chain(std::iter::once(0)).collect();

        // Create pipe for serial stdout/stderr
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as DWORD,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };

        let mut h_read: HANDLE = 0;
        let mut h_write: HANDLE = 0;

        unsafe {
            if CreatePipe(&mut h_read, &mut h_write, &mut sa, 0) == 0 {
                anyhow::bail!("Failed to create pipe for QEMU serial output (code: {})", GetLastError());
            }
            SetHandleInformation(h_read, HANDLE_FLAG_INHERIT, 0);
        }

        let mut desktop_wide: Vec<u16> = "Default\0".encode_utf16().collect();

        let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
        si.cb = std::mem::size_of::<STARTUPINFOW>() as DWORD;
        si.lpDesktop = desktop_wide.as_mut_ptr();
        si.dwFlags = STARTF_USESTDHANDLES;
        si.hStdInput = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        si.hStdOutput = h_write;
        si.hStdError = h_write;

        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

        println!("[Runner] Spawning QEMU targeted to interactive desktop station ('WinSta0\\Default')...");
        let create_res = unsafe {
            CreateProcessW(
                std::ptr::null(),
                cmd_line_wide.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1, // bInheritHandles = TRUE
                0,
                std::ptr::null_mut(),
                std::ptr::null(),
                &mut si,
                &mut pi,
            )
        };

        if create_res == 0 {
            let err = unsafe { GetLastError() };
            unsafe {
                CloseHandle(h_read);
                CloseHandle(h_write);
            }
            anyhow::bail!("Failed to launch QEMU process on Default desktop (code: {})", err);
        }

        // Close write handle in parent so reader thread receives EOF when QEMU exits
        unsafe {
            CloseHandle(h_write);
        }

        let child_pid = pi.dwProcessId;
        let child_process = pi.hProcess;
        println!("[Runner] QEMU running on physical desktop station (PID: {})", child_pid);

        // 1. Start background serial output streaming thread
        let read_handle = h_read;
        std::thread::spawn(move || {
            use std::io::Write;
            let mut buf = [0u8; 4096];
            let mut bytes_read: DWORD = 0;
            unsafe {
                while ReadFile(read_handle, buf.as_mut_ptr() as *mut _, buf.len() as DWORD, &mut bytes_read, std::ptr::null_mut()) != 0 {
                    if bytes_read == 0 {
                        break;
                    }
                    let _ = std::io::stdout().write_all(&buf[..bytes_read as usize]);
                    let _ = std::io::stdout().flush();
                }
                CloseHandle(read_handle);
            }
        });

        // 2. Start window tracker & foregrounder thread on Default desktop
        std::thread::spawn(move || {
            let desk_name: Vec<u16> = "Default\0".encode_utf16().collect();
            let hdesk = unsafe { OpenDesktopW(desk_name.as_ptr(), 0, 0, 0x01FF) };
            if hdesk == 0 {
                eprintln!("[Runner] Warning: Could not open 'Default' desktop for window tracking");
                return;
            }
            unsafe {
                SetThreadDesktop(hdesk);
            }

            struct WindowItem {
                hwnd: HWND,
                rect: RECT,
                visible: bool,
            }

            struct WindowFinder {
                pid: u32,
                windows: Vec<WindowItem>,
            }

            unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
                let finder = &mut *(lparam as *mut WindowFinder);
                let mut wpid: DWORD = 0;
                GetWindowThreadProcessId(hwnd, &mut wpid);
                if wpid == finder.pid {
                    let mut class_buf = [0u16; 256];
                    let len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), 256);
                    let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
                    if class_name == "SDL_app" {
                        let mut rect = RECT::default();
                        GetWindowRect(hwnd, &mut rect);
                        let is_vis = IsWindowVisible(hwnd) != 0;
                        if rect.right - rect.left > 100 {
                            finder.windows.push(WindowItem { hwnd, rect, visible: is_vis });
                        }
                    }
                }
                1 // continue search to find all windows
            }

            let screen_w = unsafe { GetSystemMetrics(0) };
            let screen_h = unsafe { GetSystemMetrics(1) };
            let mut last_size = (0, 0);

            // Track for up to 60 iterations (30 seconds) to catch initial window and GOP resolution change
            for _ in 0..60 {
                std::thread::sleep(Duration::from_millis(500));

                let wait_code = unsafe { WaitForSingleObject(child_process, 0) };
                if wait_code == 0 {
                    break;
                }

                let mut finder = WindowFinder {
                    pid: child_pid,
                    windows: Vec::new(),
                };

                unsafe {
                    EnumDesktopWindows(hdesk, enum_cb, &mut finder as *mut _ as LPARAM);
                }

                if !finder.windows.is_empty() {
                    // Sort descending by window area so the largest (main GOP canvas) is first
                    finder.windows.sort_by(|a, b| {
                        let area_a = (a.rect.right - a.rect.left) * (a.rect.bottom - a.rect.top);
                        let area_b = (b.rect.right - b.rect.left) * (b.rect.bottom - b.rect.top);
                        area_b.cmp(&area_a)
                    });

                    let main_win = &finder.windows[0];
                    let main_hwnd = main_win.hwnd;
                    let w = main_win.rect.right - main_win.rect.left;
                    let h = main_win.rect.bottom - main_win.rect.top;
                    let target_x = (screen_w - w).max(0) / 2;
                    let target_y = (screen_h - h).max(0) / 2;

                    // If main GOP canvas is up, hide any smaller stale text-mode windows
                    if w > 800 {
                        for aux in finder.windows.iter().skip(1) {
                            if aux.visible {
                                unsafe { ShowWindow(aux.hwnd, SW_HIDE); }
                            }
                        }
                    }

                    if (w, h) != last_size || main_win.rect.left != target_x || main_win.rect.top != target_y {
                        last_size = (w, h);
                        unsafe {
                            MoveWindow(main_hwnd, target_x, target_y, w, h, 1);
                            ShowWindow(main_hwnd, SW_RESTORE);
                            ShowWindow(main_hwnd, SW_SHOW);
                            SetWindowPos(main_hwnd, HWND_TOPMOST, target_x, target_y, w, h, SWP_SHOWWINDOW);
                            SetWindowPos(main_hwnd, HWND_NOTOPMOST, target_x, target_y, w, h, SWP_SHOWWINDOW);
                            SetForegroundWindow(main_hwnd);
                            BringWindowToTop(main_hwnd);
                            SwitchToThisWindow(main_hwnd, 1);
                        }
                    }
                }
            }

            unsafe {
                CloseDesktop(hdesk);
            }
        });

        // 3. Wait indefinitely for QEMU process to finish while user interacts with it
        println!("[Runner] Micro-OS display active on desktop. Press Ctrl+C or close QEMU window to stop.");
        unsafe {
            WaitForSingleObject(pi.hProcess, INFINITE);
            let mut exit_code: DWORD = 0;
            GetExitCodeProcess(pi.hProcess, &mut exit_code);
            CloseHandle(pi.hProcess);
            CloseHandle(pi.hThread);
        }

        println!("[Runner] QEMU session terminated.");
        Ok(())
    }
}

fn find_qemu() -> anyhow::Result<PathBuf> {
    // 1. Check standard Windows installation directory
    let default_win_path = PathBuf::from(r"C:\Program Files\qemu\qemu-system-x86_64.exe");
    if default_win_path.exists() {
        return Ok(default_win_path);
    }

    // 2. Check PATH environment variable
    if let Some(paths) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&paths) {
            let exe = p.join("qemu-system-x86_64.exe");
            if exe.exists() {
                return Ok(exe);
            }
            let bare = p.join("qemu-system-x86_64");
            if bare.exists() {
                return Ok(bare);
            }
        }
    }

    anyhow::bail!("Could not find qemu-system-x86_64. Please ensure QEMU is installed.");
}

fn find_ovmf() -> anyhow::Result<PathBuf> {
    let candidate_paths = [
        r"C:\Program Files\qemu\share\edk2-x86_64-code.fd",
        r"C:\Program Files\qemu\edk2-x86_64-code.fd",
        r"/usr/share/OVMF/OVMF_CODE.fd",
        r"/usr/share/ovmf/OVMF.fd",
    ];

    for path_str in &candidate_paths {
        let p = PathBuf::from(path_str);
        if p.exists() {
            return Ok(p);
        }
    }

    anyhow::bail!("Could not locate OVMF UEFI firmware file (edk2-x86_64-code.fd)");
}
