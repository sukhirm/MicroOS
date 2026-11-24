import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "--start-address=0xb9c60", "--stop-address=0xb9dc0", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)
print(res.stdout)
