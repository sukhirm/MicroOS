with open("kernel/src/input/tests.rs", "r", encoding="utf-8") as f:
    for i, line in enumerate(f, 1):
        if "test_desktop_keyboard_shortcuts" in line:
            print(f"{i}: {line.strip()}")
