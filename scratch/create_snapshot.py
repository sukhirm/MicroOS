import shutil, os

src_files = [
    "kernel/src/main.rs",
    "kernel/src/arch/x86_64/pic.rs",
    "kernel/src/task/scheduler.rs",
    "kernel/src/task/mod.rs",
    "kernel/src/input/keyboard.rs",
    "kernel/src/desktop/status_bar.rs",
    "kernel/src/apps/sysmon.rs",
]

dst_dir = "backup_v2_investigation_state"
os.makedirs(dst_dir, exist_ok=True)

for f in src_files:
    if os.path.exists(f):
        target = os.path.join(dst_dir, f.replace("/", "_").replace("\\", "_"))
        shutil.copy2(f, target)
        print(f"Backed up {f} -> {target}")

print("Snapshot complete.")
