import subprocess

llvm_objdump = r"C:\Users\sukhi\.rustup\toolchains\nightly-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-objdump.exe"
cmd = [llvm_objdump, "-d", "scratch/kernel_v2_elf"]
res = subprocess.run(cmd, capture_output=True, text=True)

unconditional_sti = []
current_func = ""
lines = res.stdout.splitlines()
for i, line in enumerate(lines):
    if line.endswith(">:"):
        current_func = line
    if "\tsti" in line:
        # Check instructions before sti
        # If there is a conditional jump (je, jne, jz, jnz, etc.) skipping over sti, then it's conditional.
        # Otherwise it's unconditional!
        is_conditional = False
        for k in range(max(0, i-4), i):
            if any(j in lines[k] for j in ("je\t", "jne\t", "jz\t", "jnz\t", "jbe\t", "jae\t")):
                # Check if target of jump is after sti
                is_conditional = True
                break
        if not is_conditional:
            unconditional_sti.append((current_func, line, lines[max(0, i-3):i+2]))

print(f"Total unconditional STI: {len(unconditional_sti)}")
for func, line, ctx in unconditional_sti:
    print("--------------------------------------------------")
    print(f"Func: {func}")
    print(f"STI : {line}")
    for c in ctx:
        print("  ", c)
