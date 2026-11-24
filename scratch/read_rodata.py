elf_path = r"C:\Users\sukhi\OneDrive\Documents\MicroOS_New\scratch\kernel_v2_elf"

with open(elf_path, "rb") as f:
    # rodata starts at offset 0x7370, address 0x7370
    # Let's read from offset 0x12129 - (address - offset) = 0x12129
    f.seek(0x12129)
    s1 = f.read(64)
    print("At 0x12129:", repr(s1))

    f.seek(0x1243c)
    s2 = f.read(64)
    print("At 0x1243c:", repr(s2))
