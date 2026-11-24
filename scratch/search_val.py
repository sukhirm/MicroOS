with open("scratch/kernel_v2_elf", "rb") as f:
    elf = f.read()

import struct

pattern4 = struct.pack("<I", 0x39b12)
pattern8 = struct.pack("<Q", 0x39b12)

matches4 = [i for i in range(len(elf)-4) if elf[i:i+4] == pattern4]
matches8 = [i for i in range(len(elf)-8) if elf[i:i+8] == pattern8]

print(f"Matches for 4-byte 0x39b12: {[hex(m) for m in matches4]}")
print(f"Matches for 8-byte 0x39b12: {[hex(m) for m in matches8]}")
