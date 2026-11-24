import subprocess
import re

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

jumps_to_39b = []
for line in res.stdout.splitlines():
    m = re.search(r'(j[a-z]+|callq)\s+0x([0-9a-f]+)', line)
    if m:
        target = int(m.group(2), 16)
        if 0x39b00 <= target <= 0x39b30:
            jumps_to_39b.append((line.strip(), hex(target)))

print(f"Jumps/calls in entire kernel to 0x39b00..0x39b30: {len(jumps_to_39b)}")
for raw, target in jumps_to_39b:
    print(f"  {raw} -> {target}")
