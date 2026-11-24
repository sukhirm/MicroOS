use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let kernel_path = PathBuf::from("target/x86_64-unknown-none/release/kernel");
    let out_iso_path = PathBuf::from("target/micro_os_v3_keyboard_stable.iso");

    if !kernel_path.exists() {
        anyhow::bail!("Kernel executable not found at {}", kernel_path.display());
    }

    println!("Synthesizing V3 stable keyboard ISO from: {}", kernel_path.display());
    let iso_size = runner::iso::create_hybrid_uefi_iso_file(
        &kernel_path,
        &out_iso_path,
        "MICRO_OS_V3",
    )?;

    println!("V3 stable ISO created: {} ({} bytes)", out_iso_path.display(), iso_size);

    println!("Running independent ISO structural verification...");
    let report = runner::iso_verify::verify_hybrid_iso_file(&out_iso_path)?;
    println!("{}", report.format_summary());

    Ok(())
}
