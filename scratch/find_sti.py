import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

sti_locations = []
current_func = ""
for line in res.stdout.splitlines():
    if line.endswith(">:"):
        current_func = line
    if "\tsti\n" in line or line.strip().endswith("\tsti") or "\tsti " in line:
        sti_locations.append((current_func, line))

print(f"Total STI instructions found: {len(sti_locations)}")
for func, line in sti_locations:
    print(f"{func} -> {line}")
