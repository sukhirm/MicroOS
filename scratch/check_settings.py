with open("kernel/src/apps/settings.rs", "r", encoding="utf-8") as f:
    for i, line in enumerate(f, 1):
        if "format!" in line or "String" in line or "Vec" in line or "Box" in line:
            print(f"{i}: {line.strip()}")
