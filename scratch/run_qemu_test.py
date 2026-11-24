import subprocess
import time
import os
import sys

# Locate QEMU and OVMF
qemu_paths = [
    r"C:\Program Files\qemu\qemu-system-x86_64.exe",
    r"C:\Program Files (x86)\qemu\qemu-system-x86_64.exe",
]
qemu = None
for p in qemu_paths:
    if os.path.exists(p):
        qemu = p
        break

if not qemu:
    print("QEMU not found in standard paths")
    sys.exit(1)

# Find OVMF
ovmf = None
for root, dirs, files in os.walk("."):
    for f in files:
        if f.lower().endswith(".fd") and "ovmf" in f.lower():
            ovmf = os.path.abspath(os.path.join(root, f))
            break
    if ovmf:
        break

if not ovmf:
    # search standard locations or runner
    ovmf_candidates = [
        r"runner\ovmf\OVMF.fd",
        r"C:\Program Files\qemu\share\edk2-x86_64-code.fd",
    ]
    for c in ovmf_candidates:
        if os.path.exists(c):
            ovmf = c
            break

print(f"Using QEMU: {qemu}")
print(f"Using OVMF: {ovmf}")

iso_path = os.path.abspath("target/micro_os_v3_keyboard_stable.iso")
cmd = [
    qemu,
    "-drive", f"if=pflash,format=raw,readonly=on,file={ovmf}",
    "-cdrom", iso_path,
    "-m", "512M",
    "-serial", "stdio",
    "-display", "none",
]

print("Launching QEMU...")
proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

start = time.time()
output = []
try:
    while time.time() - start < 15:
        line = proc.stdout.readline()
        if not line:
            break
        print(line, end="")
        output.append(line)
        if "Obsidian Azure Desktop compositor started" in line or "PANIC" in line:
            # give it 2 more seconds
            time.sleep(2)
            break
finally:
    proc.terminate()
    try:
        proc.wait(timeout=2)
    except:
        proc.kill()

full_text = "".join(output)
if "PANIC" in full_text:
    print("\n--- DETECTED KERNEL PANIC! ---")
else:
    print("\n--- BOOT SUCCESSFUL! NO PANIC DETECTED ---")
