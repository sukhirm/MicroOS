use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let is_build_only = args.iter().any(|a| a == "--build");
    let is_test_mode = args.iter().any(|a| a == "--test");
    let is_headless = args.iter().any(|a| a == "--headless") || is_test_mode;
    let is_release = args.iter().any(|a| a == "--release");

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

    // 3. Create bootable UEFI disk image
    let uefi_img_path = target_dir.join(format!("micro_os_uefi_{}.img", profile));
    println!("[Runner] Packaging bootable UEFI disk image with bootloader...");
    let uefi_boot = bootloader::UefiBoot::new(&kernel_bin_path);
    uefi_boot.create_disk_image(&uefi_img_path)?;
    println!("[Runner] Bootable UEFI image created: {}", uefi_img_path.display());

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
                name: "6. Kernel virtual heap mapped (1 MiB)",
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
                name: "42. Dedicated 4 MiB backbuffer mapping & blit test",
                pattern: "[PASS] 9. Dedicated 4 MiB backbuffer mapping and off-screen blit fidelity test",
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
            println!("\x1b[1;32m[Runner Test] ALL CHECKS PASSED: Phase 1, Phase 2, Phase 3A, Phase 3B, Phase 3C & Phase 4A Verified.\x1b[0m");
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
