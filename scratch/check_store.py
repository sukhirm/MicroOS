with open("kernel/src/apps/store.rs", "r", encoding="utf-8") as f:
    for i, line in enumerate(f, 1):
        if "STATIC_CATALOGUE" in line:
            print(f"{i}: {line.strip()}")
