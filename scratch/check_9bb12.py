import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "--start-address=0x9bae0", "--stop-address=0x9bb40", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)
print(res.stdout)
