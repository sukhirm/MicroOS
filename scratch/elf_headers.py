import struct

with open("scratch/kernel_v2_elf", "rb") as f:
    elf_data = f.read()

e_type, e_machine, e_version, e_entry, e_phoff, e_shoff, e_flags, e_ehsize, e_phentsize, e_phnum = struct.unpack_from("<HHIQQQIHHH", elf_data, 16)
print(f"e_type: {e_type} (2=ET_EXEC, 3=ET_DYN)")
print(f"e_entry: {hex(e_entry)}")
print(f"e_phnum: {e_phnum}")

for i in range(e_phnum):
    off = e_phoff + i * e_phentsize
    p_type, p_flags, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align = struct.unpack_from("<IIQQQQQQ", elf_data, off)
    ptype_str = {1: "PT_LOAD", 2: "PT_DYNAMIC", 3: "PT_INTERP", 4: "PT_NOTE", 6: "PT_PHDR", 7: "PT_TLS", 0x6474e550: "PT_GNU_EH_FRAME", 0x6474e551: "PT_GNU_STACK", 0x6474e552: "PT_GNU_RELRO"}.get(p_type, hex(p_type))
    print(f"PHDR {i}: type={ptype_str} vaddr={hex(p_vaddr)} paddr={hex(p_paddr)} filesz={hex(p_filesz)} memsz={hex(p_memsz)} flags={bin(p_flags)}")
