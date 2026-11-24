import hashlib
import os

files = {
    "known_good_backup": ("backup_known_good_desktop_iso/micro_os_uefi_release_known_good_01F19C38.iso", "01F19C38AEC4E3F0219CFB51123B934227D059FD536AE65C4164E0B12D484FE9"),
    "keyboard_v2_backup": ("backup_working_keyboard_desktop_build/micro_os_keyboard_test.iso", "73721AE3FBC47DACD71604C9A314A12E6317BA2E10065B95155597AD9F5EE6D9"),
    "v3_stable_new": ("target/micro_os_v3_keyboard_stable.iso", None),
}

for name, (path, expected) in files.items():
    if not os.path.exists(path):
        print(f"ERROR: {path} does not exist!")
        continue
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    digest = h.hexdigest().upper()
    size = os.path.getsize(path)
    if expected:
        match = (digest == expected.upper())
        print(f"[{name}] {path} ({size} bytes)")
        print(f"       Expected: {expected}")
        print(f"       Actual:   {digest}")
        print(f"       Match:    {match}")
    else:
        print(f"[{name}] {path} ({size} bytes)")
        print(f"       SHA-256:  {digest}")
