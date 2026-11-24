with open("scratch/kernel_v2_elf", "rb") as f:
    elf = f.read()

import struct

e_phoff, = struct.unpack_from("<Q", elf, 32)
e_phnum, = struct.unpack_from("<H", elf, 56)
e_phentsize, = struct.unpack_from("<H", elf, 54)

def vaddr_to_file(vaddr):
    for i in range(e_phnum):
        off = e_phoff + i * e_phentsize
        p_type, p_flags, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align = struct.unpack_from("<IIQQQQQQ", elf, off)
        if p_type == 1 and p_vaddr <= vaddr < p_vaddr + p_filesz:
            return p_offset + (vaddr - p_vaddr)
    return None

file_off = vaddr_to_file(0x39b00)
print(f"File offset for 0x39b00: {hex(file_off)}")
bytes_slice = elf[file_off:file_off+48]
print("Hexdump around 0x39b12:")
for i in range(0, len(bytes_slice), 16):
    chunk = bytes_slice[i:i+16]
    addr = 0x39b00 + i
    hex_str = " ".join(f"{b:02x}" for b in chunk)
    print(f"{hex(addr)}: {hex_str}")
