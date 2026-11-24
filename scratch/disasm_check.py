import subprocess
import os

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"

cmd = [llvm_objdump, "-d", "--no-show-raw-insn", "--start-address=0x39a00", "--stop-address=0x39c00", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)
print("=== Disassembly around 0x39b12 ===")
print(res.stdout)
if res.stderr:
    print("Stderr:", res.stderr)
