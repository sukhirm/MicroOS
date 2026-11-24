import subprocess
import re

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "--start-address=0x520f0", "--stop-address=0x52560", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

calls = []
for line in res.stdout.splitlines():
    if "callq" in line:
        calls.append(line.strip())

print(f"Total calls in desktop loop: {len(calls)}")
for c in calls:
    print(c)
