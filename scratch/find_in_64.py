import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

in_64 = []
for line in res.stdout.splitlines():
    if ("inb\t$0x64" in line or "inb\t0x64" in line or "0x64" in line) and "in" in line:
        in_64.append(line)

print("Inb from 0x64 found:")
for line in in_64:
    print(line)
