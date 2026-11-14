use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let is_build_only = args.iter().any(|a| a == "--build");
    let is_test_mode = args.iter().any(|a| a == "--test");
    let is_headless = args.iter().any(|a| a == "--headless") || is_test_mode;
    let is_release = args.iter().any(|a| a == "--release");

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

    // 5. Configure QEMU command
    let mut qemu_cmd = Command::new(&qemu_path);
    qemu_cmd.arg("-drive").arg(format!("if=pflash,format=raw,readonly=on,file={}", ovmf_path.display()));
    qemu_cmd.arg("-drive").arg(format!("format=raw,file={}", uefi_img_path.display()));
    qemu_cmd.arg("-m").arg("512M");
    qemu_cmd.arg("-serial").arg("stdio");

    if is_headless {
        qemu_cmd.arg("-display").arg("none");
    }

    if is_test_mode {
        println!("[Runner] Running in automated verification mode (headless)...");
        qemu_cmd.stdout(Stdio::piped());
        qemu_cmd.stderr(Stdio::piped());

        let mut child = qemu_cmd.spawn()?;
        let stdout = child.stdout.take().expect("Failed to open child stdout");

        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);
        let start_time = Instant::now();
        let timeout = Duration::from_secs(15);

        let mut captured_lines = Vec::new();
        let mut success_detected = false;

        for line_result in reader.lines() {
            if let Ok(line) = line_result {
                println!("{}", line);
                if line.contains("[SUCCESS] Micro-OS Phase 1 Bare-Metal Kernel Boot Complete!") {
                    success_detected = true;
                }
                captured_lines.push(line);
            }

            if success_detected || start_time.elapsed() > timeout {
                break;
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        if success_detected {
            println!("\n================================================================");
            println!("[Runner Test] VERIFICATION PASSED: Kernel booted and initialized.");
            println!("================================================================");
            Ok(())
        } else {
            anyhow::bail!("Verification timed out or did not detect kernel boot success banner");
        }
    } else {
        println!("[Runner] Launching QEMU...");
        let mut child = qemu_cmd.spawn()?;
        let _ = child.wait();
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
