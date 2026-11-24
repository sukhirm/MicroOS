import struct

with open(r"C:\Users\sukhi\OneDrive\Documents\MicroOS_New\backup_working_keyboard_desktop_build\micro_os_keyboard_test.iso", "rb") as f:
    f.seek(40960)
    bs = f.read(512)
    bytes_per_sec = struct.unpack("<H", bs[11:13])[0]
    sec_per_clust = bs[13]
    reserved_sec = struct.unpack("<H", bs[14:16])[0]
    num_fats = bs[16]
    root_entries = struct.unpack("<H", bs[17:19])[0]
    total_sec16 = struct.unpack("<H", bs[19:21])[0]
    media = bs[21]
    sec_per_fat = struct.unpack("<H", bs[22:24])[0]
    total_sec32 = struct.unpack("<I", bs[32:36])[0]
    fs_type = bs[54:62]

    print(f"Bytes/sec: {bytes_per_sec}")
    print(f"Sec/clust: {sec_per_clust}")
    print(f"Reserved sec: {reserved_sec}")
    print(f"Num FATs: {num_fats}")
    print(f"Root entries: {root_entries}")
    print(f"Total sec 16: {total_sec16}")
    print(f"Sec/FAT: {sec_per_fat}")
    print(f"FS type: {fs_type}")

    fat1_start = 40960 + reserved_sec * bytes_per_sec
    print(f"FAT1 start: {fat1_start}")
    f.seek(fat1_start)
    fat_bytes = f.read(sec_per_fat * bytes_per_sec)
    print(f"FAT size: {len(fat_bytes)}")

    # Let's inspect FAT entries for cluster 86
    # In FAT12 vs FAT16:
    # If total_clusters < 4085 it's FAT12!
    root_dir_sectors = ((root_entries * 32) + (bytes_per_sec - 1)) // bytes_per_sec
    data_sectors = total_sec16 - (reserved_sec + (num_fats * sec_per_fat) + root_dir_sectors)
    total_clusters = data_sectors // sec_per_clust
    print(f"Total clusters: {total_clusters}")
    if total_clusters < 4085:
        print("It's FAT12!")
    else:
        print("It's FAT16!")
