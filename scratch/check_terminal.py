with open("kernel/src/apps/terminal.rs", "r", encoding="utf-8") as f:
    for i, line in enumerate(f, 1):
        if "fn execute_command" in line or "help" in line and "match" in line:
            print(f"{i}: {line.strip()}")
        if "apt" in line and "match" in line:
            print(f"{i}: {line.strip()}")
