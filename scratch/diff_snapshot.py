import os, filecmp, difflib

src_files = [
    ("kernel/src/main.rs", "backup_v2_investigation_state/kernel_src_main.rs"),
    ("kernel/src/arch/x86_64/pic.rs", "backup_v2_investigation_state/kernel_src_arch_x86_64_pic.rs"),
    ("kernel/src/task/scheduler.rs", "backup_v2_investigation_state/kernel_src_task_scheduler.rs"),
    ("kernel/src/task/mod.rs", "backup_v2_investigation_state/kernel_src_task_mod.rs"),
    ("kernel/src/input/keyboard.rs", "backup_v2_investigation_state/kernel_src_input_keyboard.rs"),
    ("kernel/src/desktop/status_bar.rs", "backup_v2_investigation_state/kernel_src_desktop_status_bar.rs"),
    ("kernel/src/apps/sysmon.rs", "backup_v2_investigation_state/kernel_src_apps_sysmon.rs"),
]

for curr, backup in src_files:
    if not os.path.exists(curr) or not os.path.exists(backup):
        print(f"Skipping {curr} (missing)")
        continue
    with open(backup, "r", encoding="utf-8", errors="ignore") as f1, open(curr, "r", encoding="utf-8", errors="ignore") as f2:
        l1 = f1.readlines()
        l2 = f2.readlines()
    diff = list(difflib.unified_diff(l1, l2, fromfile=backup, tofile=curr))
    if diff:
        print(f"=== Diff for {curr} ===")
        for d in diff:
            print(d, end="")
        print()
    else:
        print(f"=== {curr}: No changes ===")
