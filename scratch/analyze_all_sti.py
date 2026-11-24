import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

sti_details = []
current_func = ""
lines = res.stdout.splitlines()
for i, line in enumerate(lines):
    if line.endswith(">:"):
        current_func = line
    if "\tsti" in line:
        # Check context around sti (5 lines before)
        preceding = lines[max(0, i-6):i+1]
        sti_details.append((current_func, line, preceding))

print(f"Total STI: {len(sti_details)}")
for func, line, preceding in sti_details:
    print("--------------------------------------------------")
    print(f"Function: {func}")
    for p in preceding:
        print("  ", p)
