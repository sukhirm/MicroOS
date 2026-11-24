import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

b12_insns = []
current_func = ""
for line in res.stdout.splitlines():
    if line.endswith(">:"):
        current_func = line
    if ":" in line:
        addr_str = line.split(":")[0].strip()
        try:
            addr = int(addr_str, 16)
            if (addr & 0xFFF) == 0xb12:
                b12_insns.append((current_func, line.strip()))
        except ValueError:
            pass

print(f"Total instructions starting at xxxb12: {len(b12_insns)}")
for func, line in b12_insns:
    print(f"{hex(int(line.split(':')[0], 16))}: {func} -> {line}")
