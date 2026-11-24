import struct

elf_path = r"C:\Users\sukhi\OneDrive\Documents\MicroOS_New\scratch\kernel_v2_elf"

with open(elf_path, "rb") as f:
    hdr = f.read(64)
    sh_offset = struct.unpack("<Q", hdr[40:48])[0]
    shentsize = struct.unpack("<H", hdr[58:60])[0]
    shnum = struct.unpack("<H", hdr[60:62])[0]
    shstrndx = struct.unpack("<H", hdr[62:64])[0]

    # Read all section headers
    f.seek(sh_offset)
    shdrs = [f.read(shentsize) for _ in range(shnum)]

    # Read shstrtab
    shstr_sh = shdrs[shstrndx]
    shstr_offset = struct.unpack("<Q", shstr_sh[24:32])[0]
    shstr_size = struct.unpack("<Q", shstr_sh[32:40])[0]
    f.seek(shstr_offset)
    shstrtab = f.read(shstr_size)

    def get_sh_name(sh):
        name_idx = struct.unpack("<I", sh[:4])[0]
        return shstrtab[name_idx:].split(b'\0', 1)[0].decode('latin1')

    symtab_sh = None
    strtab_sh = None
    for sh in shdrs:
        name = get_sh_name(sh)
        if name == ".symtab":
            symtab_sh = sh
        elif name == ".strtab":
            strtab_sh = sh

    if not symtab_sh or not strtab_sh:
        print("No symtab/strtab found")
        exit(1)

    symtab_offset = struct.unpack("<Q", symtab_sh[24:32])[0]
    symtab_size = struct.unpack("<Q", symtab_sh[32:40])[0]
    sym_entsize = struct.unpack("<Q", symtab_sh[56:64])[0]

    strtab_offset = struct.unpack("<Q", strtab_sh[24:32])[0]
    strtab_size = struct.unpack("<Q", strtab_sh[32:40])[0]
    f.seek(strtab_offset)
    strtab = f.read(strtab_size)

    f.seek(symtab_offset)
    num_syms = symtab_size // sym_entsize
    symbols = []
    for _ in range(num_syms):
        sym_bytes = f.read(sym_entsize)
        st_name = struct.unpack("<I", sym_bytes[0:4])[0]
        st_info = sym_bytes[4]
        st_other = sym_bytes[5]
        st_shndx = struct.unpack("<H", sym_bytes[6:8])[0]
        st_value = struct.unpack("<Q", sym_bytes[8:16])[0]
        st_size = struct.unpack("<Q", sym_bytes[16:24])[0]

        name = strtab[st_name:].split(b'\0', 1)[0].decode('latin1', errors='ignore')
        if st_value > 0:
            symbols.append((st_value, st_size, name))

    symbols.sort(key=lambda x: x[0])

    print(f"Total symbols: {len(symbols)}")
    target_addr = 0x39b12
    print(f"Target address: 0x{target_addr:X}")

    # Find enclosing or nearby symbols
    for i, (addr, size, name) in enumerate(symbols):
        if addr <= target_addr < addr + size or (addr <= target_addr and i+1 < len(symbols) and symbols[i+1][0] > target_addr):
            print(f"MATCH: 0x{addr:X} - 0x{addr+size:X} (len {size}): {name}")
            # print surrounding 5 symbols
            print("--- Context ---")
            for j in range(max(0, i-5), min(len(symbols), i+6)):
                a, s, n = symbols[j]
                marker = "===> " if j == i else "     "
                print(f"{marker}0x{a:X} (+0x{s:X}): {n}")
            break
