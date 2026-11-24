import subprocess
import re

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

ins = []
current_func = ""
for line in res.stdout.splitlines():
    if line.endswith(">:"):
        current_func = line
    m = re.search(r':\s+[0-9a-f ]+\s+(in[b|w|l]?\s+.*)', line)
    if m:
        ins.append((current_func, line.strip(), m.group(1)))

print(f"Total IN instructions: {len(ins)}")
for func, raw, mnem in ins:
    print(f"{func}\n  {raw}\n  => {mnem}")
