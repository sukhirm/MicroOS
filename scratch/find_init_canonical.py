import os

matches = []
for root, dirs, files in os.walk("kernel/src"):
    for file in files:
        if file.endswith(".rs"):
            path = os.path.join(root, file)
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                for idx, line in enumerate(f, 1):
                    if "init_canonical_packages" in line:
                        matches.append((path, idx, line.strip()))

for p, i, l in matches:
    print(f"{p}:{i} {l}")
