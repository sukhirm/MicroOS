import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"

cmd = [llvm_objdump, "-t", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)
lines = [l for l in res.stdout.splitlines() if " F " in l or " 000000000003" in l]
print(f"Total symbol lines: {len(res.stdout.splitlines())}")
for l in lines[:50]:
    print(l)
