import struct

iso_path = r"C:\Users\sukhi\OneDrive\Documents\MicroOS_New\backup_working_keyboard_desktop_build\micro_os_keyboard_test.iso"
out_kernel_path = r"C:\Users\sukhi\OneDrive\Documents\MicroOS_New\scratch\kernel_v2_elf"

with open(iso_path, "rb") as f:
    fat_offset = 40960
    f.seek(fat_offset)
    bs = f.read(512)
    bytes_per_sec = struct.unpack("<H", bs[11:13])[0]
    sec_per_clust = bs[13]
    reserved_sec = struct.unpack("<H", bs[14:16])[0]
    num_fats = bs[16]
    root_entries = struct.unpack("<H", bs[17:19])[0]
    sec_per_fat = struct.unpack("<H", bs[22:24])[0]

    fat1_offset = fat_offset + reserved_sec * bytes_per_sec
    root_dir_offset = fat1_offset + num_fats * sec_per_fat * bytes_per_sec
    data_offset = root_dir_offset + root_entries * 32

    cluster_size = sec_per_clust * bytes_per_sec

    f.seek(fat1_offset)
    fat_table = f.read(sec_per_fat * bytes_per_sec)

    def get_fat12_next(c):
        offset = (c * 3) // 2
        raw = struct.unpack("<H", fat_table[offset:offset+2])[0]
        if c % 2 == 0:
            return raw & 0x0FFF
        else:
            return raw >> 4

    f.seek(root_dir_offset)
    root_dir = f.read(root_entries * 32)

    kernel_cluster = None
    kernel_size = None

    for i in range(root_entries):
        entry = root_dir[i*32:(i+1)*32]
        if entry[0] == 0:
            break
        if entry[0] == 0xE5:
            continue
        name = entry[:11].decode('latin1', errors='ignore')
        if name.startswith("KERNEL"):
            kernel_cluster = struct.unpack("<H", entry[26:28])[0]
            kernel_size = struct.unpack("<I", entry[28:32])[0]
            print(f"Found kernel: start cluster {kernel_cluster}, size {kernel_size}")
            break

    cur_clust = kernel_cluster
    kernel_data = bytearray()

    while cur_clust < 0x0FF8 and len(kernel_data) < kernel_size:
        c_offset = data_offset + (cur_clust - 2) * cluster_size
        f.seek(c_offset)
        chunk = f.read(min(cluster_size, kernel_size - len(kernel_data)))
        kernel_data.extend(chunk)
        nxt = get_fat12_next(cur_clust)
        cur_clust = nxt

    print(f"Extracted {len(kernel_data)} bytes (expected {kernel_size})")
    assert len(kernel_data) == kernel_size
    with open(out_kernel_path, "wb") as kf:
        kf.write(kernel_data)
    print(f"Saved extracted kernel to {out_kernel_path}")
