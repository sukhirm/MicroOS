import subprocess
import re

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

indirect_calls = []
current_func = ""
for line in res.stdout.splitlines():
    if line.endswith(">:"):
        current_func = line
    if "callq\t*" in line or "jmpq\t*" in line:
        indirect_calls.append((current_func, line.strip()))

print(f"Total indirect calls/jmps: {len(indirect_calls)}")
for func, line in indirect_calls:
    print(f"{func} -> {line}")
