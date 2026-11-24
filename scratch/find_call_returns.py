import subprocess
import re

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

# Find all calls and check address of instruction immediately following the call
lines = res.stdout.splitlines()
call_returns = []
for i, line in enumerate(lines):
    if "callq" in line:
        # Find next instruction line
        for k in range(i+1, min(len(lines), i+5)):
            if lines[k].strip() and ":" in lines[k]:
                next_addr_str = lines[k].split(":")[0].strip()
                try:
                    next_addr = int(next_addr_str, 16)
                    if (next_addr & 0xFFF) == 0xb12 or (next_addr & 0xFFF) == 0xb13 or abs(next_addr - 0x39b12) < 0x20:
                        call_returns.append((lines[i].strip(), lines[k].strip()))
                except ValueError:
                    pass
                break

print(f"Call return addresses near 0x39b12: {len(call_returns)}")
for c, r in call_returns:
    print(f"Call: {c}\nReturn: {r}\n")
