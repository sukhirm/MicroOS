with open("scratch/kernel_v2_elf", "rb") as f:
    elf_data = f.read()

# Let's inspect bytes in elf_data at offsets around 0x12129
def get_str(addr, length):
    # Since it's PIE, let's find the load addresses of sections
    # Or scan ELF program headers
    import struct
    e_phoff, = struct.unpack_from("<Q", elf_data, 32)
    e_phnum, = struct.unpack_from("<H", elf_data, 56)
    e_phentsize, = struct.unpack_from("<H", elf_data, 54)
    for i in range(e_phnum):
        off = e_phoff + i * e_phentsize
        p_type, p_flags, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align = struct.unpack_from("<IIQQQQQQ", elf_data, off)
        if p_type == 1: # PT_LOAD
            if p_vaddr <= addr < p_vaddr + p_filesz:
                file_off = p_offset + (addr - p_vaddr)
                return elf_data[file_off:file_off+length]
    return None

print("Str at 0x12129 (len 12):", get_str(0x12129, 12))
print("Str at 0x1243c (len 129):", get_str(0x1243c, 30))
print("Str at 0xc178 (len 22):", get_str(0xc178, 22))
print("Str at 0x18fa1 (len 149):", get_str(0x18fa1, 30))
