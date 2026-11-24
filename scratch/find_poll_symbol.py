import re

with open('scratch/kernel_v2_elf', 'rb') as f:
    text = f.read().decode('latin1', errors='ignore')

matches = set(re.findall(r'[a-zA-Z0-9_]+poll[a-zA-Z0-9_]+', text))
print("Poll symbols found:", matches)
