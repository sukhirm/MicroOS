import os
import hashlib
import time

kernel_path = "target/x86_64-unknown-none/release/kernel"
iso_path = "target/micro_os_v3_keyboard_stable.iso"

print("Kernel exists:", os.path.exists(kernel_path))
if os.path.exists(kernel_path):
    print("Kernel mtime:", time.ctime(os.path.getmtime(kernel_path)))
    print("Kernel size:", os.path.getsize(kernel_path))
    k_hash = hashlib.sha256(open(kernel_path, "rb").read()).hexdigest().upper()
    print("Kernel SHA-256:", k_hash)

print("ISO exists:", os.path.exists(iso_path))
if os.path.exists(iso_path):
    print("ISO mtime:", time.ctime(os.path.getmtime(iso_path)))
    print("ISO size:", os.path.getsize(iso_path))
    iso_hash = hashlib.sha256(open(iso_path, "rb").read()).hexdigest().upper()
    print("ISO SHA-256:", iso_hash)

# Check strings inside kernel binary
with open(kernel_path, "rb") as f:
    k_data = f.read()

checks = [
    b"driver.keyboard",
    b"driver.touchpad",
    b"driver.wifi",
    b"driver.bluetooth",
    b"Catalogue must contain exactly 10 initial packages",
    b"Dedicated hardware driver packages (keyboard, touchpad, wifi, bluetooth) verified",
    b"ELAN Input Device (I2C HID Probed | Pointer: Super+Arrows)",
    b"STATUS=INCLUDED_ACPI_PROBED",
]

print("\n--- Kernel Binary String Verification ---")
for c in checks:
    print(f"Contains '{c.decode('utf-8', errors='replace')}':", c in k_data)
