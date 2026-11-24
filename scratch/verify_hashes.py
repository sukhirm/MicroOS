import hashlib
import os

files = [
    "backup_known_good_desktop_iso/micro_os_uefi_release_known_good_01F19C38.iso",
    "backup_working_keyboard_desktop_build/micro_os_keyboard_test.iso",
    "target/micro_os_v3_keyboard_stable.iso"
]

for f in files:
    if os.path.exists(f):
        h = hashlib.sha256()
        with open(f, "rb") as fp:
            while chunk := fp.read(65536):
                h.update(chunk)
        print(f"{h.hexdigest().upper()}  {f}")
    else:
        print(f"NOT FOUND: {f}")
