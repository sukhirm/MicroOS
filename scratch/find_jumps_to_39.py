import subprocess
import re

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "--start-address=0x51000", "--stop-address=0x52560", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

jumps_to_39 = []
for line in res.stdout.splitlines():
    m = re.search(r'(j[a-z]+)\s+0x([0-9a-f]+)', line)
    if m:
        target = int(m.group(2), 16)
        if 0x39000 <= target <= 0x3a000:
            jumps_to_39.append((line.strip(), hex(target)))

print(f"Jumps from desktop loop to 0x39xxx: {len(jumps_to_39)}")
for raw, target in jumps_to_39:
    print(f"  {raw} -> {target}")
