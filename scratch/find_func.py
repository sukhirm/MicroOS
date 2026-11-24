import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"

cmd = [llvm_objdump, "-t", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

target = 0x39b12
found = []
for line in res.stdout.splitlines():
    parts = line.split()
    if len(parts) >= 5:
        try:
            addr = int(parts[0], 16)
            size = int(parts[4], 16)
            name = parts[-1]
            if addr <= target < addr + size:
                found.append((addr, size, name))
        except ValueError:
            pass

print("Symbols covering 0x39b12:")
for addr, size, name in found:
    print(f"  {hex(addr)} - {hex(addr+size)}: {name}")
