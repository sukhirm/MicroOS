//! Host Integration Tests for MicroOS Package Architecture & Hardware Driver Separation.
//!
//! Validates:
//! 1. Software Store catalogue contract: exactly 10 application packages.
//! 2. Separation of hardware driver packages from the application catalogue.
//! 3. All 4 target laptop drivers (PS/2 keyboard, ELAN I2C HID touchpad, Realtek Wi-Fi 6, Realtek Bluetooth)
//!    are registered as immutable, protected, built-in system packages.
//! 4. Driver metadata integrity and safe removal protection.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageCategory {
    All,
    Featured,
    System,
    Utilities,
    Productivity,
    Installed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Builtin,
    Installed,
    Available,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPackage {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub category: PackageCategory,
    pub size_bytes: usize,
    pub status: InstallStatus,
    pub is_protected: bool,
}

impl HostPackage {
    pub fn validate_metadata(&self) -> bool {
        !self.id.is_empty()
            && self.id.contains('.')
            && !self.name.is_empty()
            && !self.version.is_empty()
            && !self.description.is_empty()
            && self.size_bytes > 0
    }
}

/// The 10 canonical native Micro-OS applications in the default software catalogue.
pub static STORE_CATALOGUE: [HostPackage; 10] = [
    HostPackage {
        id: "org.microos.files",
        name: "File Manager",
        version: "0.11.17",
        description: "Desktop hierarchical graphical file browser and in-memory VFS navigator",
        category: PackageCategory::System,
        size_bytes: 48 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "org.microos.terminal",
        name: "Terminal",
        version: "0.11.17",
        description: "Interactive monospaced command-line shell with 18 built-in system tools",
        category: PackageCategory::Productivity,
        size_bytes: 64 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "org.microos.sysmon",
        name: "System Monitor",
        version: "0.11.17",
        description: "Real-time task inspector, CPU, physical RAM, and scheduler telemetry",
        category: PackageCategory::System,
        size_bytes: 52 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "org.microos.settings",
        name: "Settings",
        version: "0.11.17",
        description: "Desktop themes, accent colors, display metrics, and input configuration",
        category: PackageCategory::System,
        size_bytes: 56 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "org.microos.network",
        name: "Network Manager",
        version: "0.11.17",
        description: "PCI bus hardware scanner, Realtek Wi-Fi 6 & GbE diagnostics",
        category: PackageCategory::System,
        size_bytes: 60 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "org.microos.editor",
        name: "Text Editor",
        version: "1.0.0",
        description: "Lightweight plain text and code editor for in-memory VFS documents",
        category: PackageCategory::Productivity,
        size_bytes: 72 * 1024,
        status: InstallStatus::Available,
        is_protected: false,
    },
    HostPackage {
        id: "org.microos.viewer",
        name: "Image Viewer",
        version: "1.0.0",
        description: "High-resolution 32-bit linear framebuffer canvas image viewer",
        category: PackageCategory::Utilities,
        size_bytes: 84 * 1024,
        status: InstallStatus::Available,
        is_protected: false,
    },
    HostPackage {
        id: "org.microos.calc",
        name: "Calculator",
        version: "1.1.0",
        description: "Standard arithmetic and programmer binary/hexadecimal utility",
        category: PackageCategory::Utilities,
        size_bytes: 32 * 1024,
        status: InstallStatus::Available,
        is_protected: false,
    },
    HostPackage {
        id: "org.microos.clock",
        name: "Clock & Alarms",
        version: "0.9.0",
        description: "World clock, countdown stopwatch, and system timer display",
        category: PackageCategory::Utilities,
        size_bytes: 40 * 1024,
        status: InstallStatus::Available,
        is_protected: false,
    },
    HostPackage {
        id: "org.microos.sysinfo",
        name: "System Information",
        version: "1.0.0",
        description: "Detailed hardware manifest, ACPI tables, and PCI device tree viewer",
        category: PackageCategory::System,
        size_bytes: 48 * 1024,
        status: InstallStatus::Available,
        is_protected: false,
    },
];

/// The 4 canonical hardware device drivers packaged separately from the application catalogue.
pub static HARDWARE_DRIVERS: [HostPackage; 4] = [
    HostPackage {
        id: "driver.keyboard",
        name: "Standard PS/2 Keyboard Driver",
        version: "0.11.17",
        description: "Intel 8042 PS/2 controller driver for laptop internal keyboard (Port 0x60/0x64)",
        category: PackageCategory::System,
        size_bytes: 16 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "driver.touchpad",
        name: "ELAN I2C HID Touchpad Driver",
        version: "0.11.17",
        description: "ELAN pointing device driver manifest for AMD DesignWare I2C HID bus (AMDI0010)",
        category: PackageCategory::System,
        size_bytes: 24 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "driver.wifi",
        name: "Realtek 8852BE-VT Wi-Fi 6 Driver",
        version: "0.11.17",
        description: "Realtek 802.11ax PCIe wireless network adapter driver & firmware payload (rtw8852b)",
        category: PackageCategory::System,
        size_bytes: 64 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
    HostPackage {
        id: "driver.bluetooth",
        name: "Realtek Wireless Bluetooth Driver",
        version: "0.11.17",
        description: "Realtek Bluetooth 5.2 HCI transport controller driver (Vendor 0x0BDA)",
        category: PackageCategory::System,
        size_bytes: 32 * 1024,
        status: InstallStatus::Builtin,
        is_protected: true,
    },
];

#[test]
fn test_store_catalogue_exact_count() {
    assert_eq!(
        STORE_CATALOGUE.len(),
        10,
        "Software Store catalogue must contain exactly 10 initial application packages"
    );
    for pkg in &STORE_CATALOGUE {
        assert!(pkg.validate_metadata(), "Package {} must have valid metadata", pkg.id);
    }
}

#[test]
fn test_hardware_driver_separation_and_count() {
    assert_eq!(
        HARDWARE_DRIVERS.len(),
        4,
        "Exactly 4 hardware drivers must be packaged for the laptop hardware profile"
    );

    // Verify drivers are distinct from store application catalogue
    for driver in &HARDWARE_DRIVERS {
        assert!(
            !STORE_CATALOGUE.iter().any(|app| app.id == driver.id),
            "Driver {} must not be part of user-facing Store application catalogue",
            driver.id
        );
        assert!(driver.validate_metadata(), "Driver {} must have valid metadata", driver.id);
        assert_eq!(driver.status, InstallStatus::Builtin, "Driver {} must be Builtin", driver.id);
        assert!(driver.is_protected, "Driver {} must be protected from removal", driver.id);
        assert_eq!(driver.category, PackageCategory::System, "Driver {} must be in System category", driver.id);
    }
}

#[test]
fn test_hardware_driver_ids_match_specification() {
    let ids: Vec<&'static str> = HARDWARE_DRIVERS.iter().map(|d| d.id).collect();
    assert!(ids.contains(&"driver.keyboard"), "Must include PS/2 keyboard driver");
    assert!(ids.contains(&"driver.touchpad"), "Must include ELAN I2C HID touchpad driver");
    assert!(ids.contains(&"driver.wifi"), "Must include Realtek 8852BE-VT Wi-Fi 6 driver");
    assert!(ids.contains(&"driver.bluetooth"), "Must include Realtek Bluetooth driver");
}
