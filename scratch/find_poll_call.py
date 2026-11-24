import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

# Find poll_hardware symbol address first
poll_addr = None
for line in res.stdout.splitlines():
    if "poll_hardware" in line and ">:" in line:
        poll_addr = line.split()[0]
        print(f"poll_hardware address: {line}")

# Now find calls to poll_hardware in kernel_main
for line in res.stdout.splitlines():
    if poll_addr and poll_addr in line and "call" in line:
        print(f"Call to poll_hardware: {line}")
    elif "poll_hardware" in line and "call" in line:
        print(f"Call: {line}")
