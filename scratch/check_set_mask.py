with open("scratch/kernel_v2_elf", "rb") as f:
    elf = f.read()

import subprocess
llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

# Find any call to set_mask or outb to 0x21
lines = res.stdout.splitlines()
for i, line in enumerate(lines):
    if "set_mask" in line:
        print(f"Line {i}: {line}")
    if "outb" in line and "0x21" in line:
        print(f"Line {i}: {line}")
