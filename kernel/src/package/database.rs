//! Package Database for MicroOS APT.
//!
//! Stores package definitions, installation states, dependency metadata,
//! and cryptographic checksums in a bounded, spinlock-protected data structure.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum number of packages supported in the in-memory database to prevent heap exhaustion.
pub const MAX_PACKAGES: usize = 64;

/// Global spinlock-protected Package Database.
pub static PACKAGE_DB: Mutex<PackageDatabase> = Mutex::new(PackageDatabase::new());

/// High-level categories for organizing packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PackageCategory {
    System,
    Utilities,
    Productivity,
    Development,
    Media,
}

impl PackageCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageCategory::System => "System",
            PackageCategory::Utilities => "Utilities",
            PackageCategory::Productivity => "Productivity",
            PackageCategory::Development => "Development",
            PackageCategory::Media => "Media",
        }
    }
}

/// Installation status of an individual package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    /// Statically linked into the kernel binary; permanent and immutable.
    Builtin,
    /// Installed in the operating system; uninstallable.
    Installed,
    /// Available in remote/catalogue repository; payload pending download.
    Available,
    /// Installation attempted, but repository payload or network driver is offline.
    Unavailable,
}

impl InstallStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallStatus::Builtin => "built-in",
            InstallStatus::Installed => "installed",
            InstallStatus::Available => "available",
            InstallStatus::Unavailable => "unavailable",
        }
    }
}

/// Source origin of a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSource {
    CoreSystem,
    OfficialRepository,
    Community,
}

impl PackageSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageSource::CoreSystem => "Micro-OS Core System",
            PackageSource::OfficialRepository => "Official Package Index",
            PackageSource::Community => "Community Contrib",
        }
    }
}

/// Errors that can occur during package database and manager operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    PackageNotFound(String),
    AlreadyInstalled(String),
    CannotRemoveProtected(String),
    MissingDependency(String),
    CyclicDependency(String),
    DependencyRequired(String, String),
    PayloadMissing(String),
    RepositoryOffline,
    DatabaseFull,
    InvalidMetadata(String),
    DuplicatePackageId(String),
}

/// Metadata description for an individual package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: PackageCategory,
    pub size_bytes: usize,
    pub dependencies: Vec<String>,
    pub status: InstallStatus,
    pub source: PackageSource,
    pub checksum: String,
    pub is_protected: bool,
    pub is_featured: bool,
    pub has_payload: bool,
}

impl Package {
    /// Constructs a new Package instance.
    pub fn new(
        id: &str,
        name: &str,
        version: &str,
        description: &str,
        category: PackageCategory,
        size_bytes: usize,
        dependencies: &[&str],
        status: InstallStatus,
        source: PackageSource,
        checksum: &str,
        is_protected: bool,
        is_featured: bool,
        has_payload: bool,
    ) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            version: version.to_owned(),
            description: description.to_owned(),
            category,
            size_bytes,
            dependencies: dependencies.iter().map(|&s| s.to_owned()).collect(),
            status,
            source,
            checksum: checksum.to_owned(),
            is_protected,
            is_featured,
            has_payload,
        }
    }

    /// Formats package size into human-readable string.
    pub fn formatted_size(&self) -> String {
        if self.size_bytes >= 1024 * 1024 {
            format!("{} MiB", self.size_bytes / (1024 * 1024))
        } else {
            format!("{} KiB", self.size_bytes / 1024)
        }
    }

    /// Validates package metadata fields to prevent corrupt catalogue records.
    pub fn validate_metadata(&self) -> Result<(), PackageError> {
        if self.id.is_empty() {
            return Err(PackageError::InvalidMetadata("Package ID cannot be empty".into()));
        }
        if self.name.is_empty() {
            return Err(PackageError::InvalidMetadata("Package name cannot be empty".into()));
        }
        if self.version.is_empty() {
            return Err(PackageError::InvalidMetadata("Package version cannot be empty".into()));
        }
        if self.description.is_empty() {
            return Err(PackageError::InvalidMetadata("Package description cannot be empty".into()));
        }
        if self.size_bytes == 0 {
            return Err(PackageError::InvalidMetadata("Package size must be greater than zero".into()));
        }
        Ok(())
    }
}

/// In-memory package database.
pub struct PackageDatabase {
    packages: Vec<Package>,
}

impl PackageDatabase {
    /// Constructs an empty package database.
    pub const fn new() -> Self {
        Self {
            packages: Vec::new(),
        }
    }

    /// Clears the database.
    pub fn clear(&mut self) {
        self.packages.clear();
    }

    /// Number of packages currently registered.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Registers a new package into the database with validation.
    pub fn register(&mut self, pkg: Package) -> Result<(), PackageError> {
        pkg.validate_metadata()?;

        if self.packages.len() >= MAX_PACKAGES {
            return Err(PackageError::DatabaseFull);
        }

        if self.packages.iter().any(|p| p.id == pkg.id) {
            return Err(PackageError::DuplicatePackageId(pkg.id));
        }

        self.packages.push(pkg);
        Ok(())
    }

    /// Looks up a package by ID or short name.
    ///
    /// For example, `"terminal"` matches `"org.microos.terminal"`.
    pub fn get(&self, query: &str) -> Option<&Package> {
        let q_lower = query.to_lowercase();
        // 1. Exact ID match
        if let Some(p) = self.packages.iter().find(|p| p.id.to_lowercase() == q_lower) {
            return Some(p);
        }
        // 2. Short name match (e.g. suffix "terminal" or exact name)
        self.packages.iter().find(|p| {
            p.name.to_lowercase() == q_lower
                || p.id.strip_prefix("org.microos.").map(|s| s.to_lowercase()) == Some(q_lower.clone())
        })
    }

    /// Looks up a mutable package by ID.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Package> {
        let q_lower = id.to_lowercase();
        self.packages.iter_mut().find(|p| {
            p.id.to_lowercase() == q_lower
                || p.id.strip_prefix("org.microos.").map(|s| s.to_lowercase()) == Some(q_lower.clone())
        })
    }

    /// Returns a slice of all packages in the database.
    pub fn all(&self) -> &[Package] {
        &self.packages
    }

    /// Returns all currently installed or built-in packages.
    pub fn installed(&self) -> Vec<&Package> {
        self.packages
            .iter()
            .filter(|p| p.status == InstallStatus::Builtin || p.status == InstallStatus::Installed)
            .collect()
    }

    /// Searches packages by keyword matching across ID, name, or description.
    pub fn search(&self, query: &str) -> Vec<&Package> {
        let q_lower = query.trim().to_lowercase();
        if q_lower.is_empty() {
            return self.packages.iter().collect();
        }

        self.packages
            .iter()
            .filter(|p| {
                p.id.to_lowercase().contains(&q_lower)
                    || p.name.to_lowercase().contains(&q_lower)
                    || p.description.to_lowercase().contains(&q_lower)
            })
            .collect()
    }

    /// Updates the installation status of a package.
    pub fn update_status(&mut self, id: &str, status: InstallStatus) -> Result<(), PackageError> {
        let pkg = self
            .get_mut(id)
            .ok_or_else(|| PackageError::PackageNotFound(id.to_string()))?;
        pkg.status = status;
        Ok(())
    }

    /// Seeds the database with canonical Micro-OS packages and internal system dependencies.
    pub fn init_canonical_packages(&mut self) {
        self.clear();

        // --- Core Internal Subsystem Virtual Packages ---
        let _ = self.register(Package::new(
            "core.vfs",
            "Virtual File System Core",
            "0.11.17",
            "In-memory hierarchical file and directory storage subsystem",
            PackageCategory::System,
            16 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:core-vfs-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "sys.gui",
            "Desktop Windowing System",
            "0.11.17",
            "Z-ordered window manager, surface clipping, and 2D graphics engine",
            PackageCategory::System,
            32 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:sys-gui-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "kernel.timer",
            "Hardware Timer Subsystem",
            "0.11.17",
            "8254 PIT 100 Hz timer and system uptime telemetry driver",
            PackageCategory::System,
            8 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:kernel-timer-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "kernel.scheduler",
            "Task Scheduler Subsystem",
            "0.11.17",
            "Preemptive round-robin multi-task scheduler and TCB queue",
            PackageCategory::System,
            16 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:kernel-scheduler-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "kernel.memory",
            "Memory Management Subsystem",
            "0.11.17",
            "Physical frame allocator and 16 MiB linked-list heap allocator",
            PackageCategory::System,
            12 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:kernel-memory-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "desktop.shell",
            "Cosmic Slate Desktop Shell",
            "0.11.17",
            "Wallpaper compositor, top status bar, floating dock, and cursor",
            PackageCategory::System,
            24 * 1024,
            &["sys.gui"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:desktop-shell-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "arch.pci",
            "PCI Hardware Subsystem",
            "0.11.17",
            "x86_64 PCI Configuration Mechanism #1 bus enumerator",
            PackageCategory::System,
            14 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:arch-pci-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "arch.x86_64",
            "CPU Architecture Primitives",
            "0.11.17",
            "GDT, TSS, IDT hardware exception handlers and control registers",
            PackageCategory::System,
            10 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:arch-x86_64-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "graphics.surface",
            "Linear Framebuffer Driver",
            "0.11.17",
            "UEFI GOP 32-bit linear framebuffer and 16 MiB double buffer",
            PackageCategory::System,
            20 * 1024,
            &[],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:graphics-surface-0.11.17",
            true,
            false,
            true,
        ));

        // --- Hardware Device Drivers (HP Victus 15 Supported Hardware) ---
        let _ = self.register(Package::new(
            "driver.keyboard",
            "Standard PS/2 Keyboard Driver",
            "0.11.17",
            "Intel 8042 PS/2 controller driver for laptop internal keyboard (Port 0x60/0x64)",
            PackageCategory::System,
            16 * 1024,
            &["arch.x86_64"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:driver-keyboard-0.11.17",
            true,
            false,
            true,
        ));

        let _ = self.register(Package::new(
            "driver.touchpad",
            "ELAN I2C HID Touchpad Driver",
            "0.11.17",
            "ELAN pointing device driver manifest for AMD DesignWare I2C HID bus (AMDI0010)",
            PackageCategory::System,
            24 * 1024,
            &["arch.pci", "sys.gui"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:driver-touchpad-0.11.17",
            true,
            false,
            true,
        ));

        let _ = self.register(Package::new(
            "driver.wifi",
            "Realtek 8852BE-VT Wi-Fi 6 Driver",
            "0.11.17",
            "Realtek 802.11ax PCIe wireless network adapter driver & firmware payload (rtw8852b)",
            PackageCategory::System,
            64 * 1024,
            &["arch.pci"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:driver-wifi-0.11.17",
            true,
            false,
            true,
        ));

        let _ = self.register(Package::new(
            "driver.bluetooth",
            "Realtek Wireless Bluetooth Driver",
            "0.11.17",
            "Realtek Bluetooth 5.2 HCI transport controller driver (Vendor 0x0BDA)",
            PackageCategory::System,
            32 * 1024,
            &["arch.pci"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:driver-bluetooth-0.11.17",
            true,
            false,
            true,
        ));

        // --- Core Built-in Applications ---
        let _ = self.register(Package::new(
            "org.microos.files",
            "File Manager",
            "0.11.17",
            "Desktop graphical file browser and in-memory VFS navigator",
            PackageCategory::System,
            48 * 1024,
            &["core.vfs", "sys.gui"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:app-files-0.11.17",
            true,
            true,
            true,
        ));

        let _ = self.register(Package::new(
            "org.microos.terminal",
            "Terminal",
            "0.11.17",
            "Interactive monospaced command-line shell with 18 built-in system tools",
            PackageCategory::Productivity,
            64 * 1024,
            &["core.vfs", "kernel.timer"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:app-terminal-0.11.17",
            true,
            true,
            true,
        ));

        let _ = self.register(Package::new(
            "org.microos.sysmon",
            "System Monitor",
            "0.11.17",
            "Real-time task inspector, CPU, physical RAM, and scheduler telemetry",
            PackageCategory::System,
            52 * 1024,
            &["kernel.scheduler", "kernel.memory"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:app-sysmon-0.11.17",
            true,
            true,
            true,
        ));

        let _ = self.register(Package::new(
            "org.microos.settings",
            "Settings",
            "0.11.17",
            "Desktop themes, accent colors, display metrics, and input configuration",
            PackageCategory::System,
            56 * 1024,
            &["desktop.shell"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:app-settings-0.11.17",
            true,
            false,
            true,
        ));


        let _ = self.register(Package::new(
            "org.microos.network",
            "Network Manager",
            "0.11.17",
            "PCI bus hardware scanner, Realtek Wi-Fi 6 & GbE diagnostics",
            PackageCategory::System,
            60 * 1024,
            &["arch.pci"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:app-network-0.11.17",
            true,
            false,
            true,
        ));

        let _ = self.register(Package::new(
            "org.microos.deadlock",
            "Deadlock Research Lab",
            "1.0.0",
            "Topic R14: Deadlock Avoidance vs. Detection laboratory, Banker matrices & benchmarks",
            PackageCategory::Development,
            96 * 1024,
            &["sys.gui"],
            InstallStatus::Builtin,
            PackageSource::CoreSystem,
            "sha256:app-deadlock-1.0.0",
            true,
            false,
            true,
        ));



        // --- Available Repository Packages (Unbundled / Remote Catalogue) ---
        let _ = self.register(Package::new(
            "org.microos.editor",
            "Text Editor",
            "1.0.0",
            "Lightweight plain text and code editor for in-memory VFS documents",
            PackageCategory::Productivity,
            72 * 1024,
            &["core.vfs", "sys.gui"],
            InstallStatus::Available,
            PackageSource::OfficialRepository,
            "sha256:app-editor-1.0.0",
            false,
            true,
            false, // Remote payload pending network/repository
        ));

        let _ = self.register(Package::new(
            "org.microos.viewer",
            "Image Viewer",
            "1.0.0",
            "High-resolution 32-bit linear framebuffer canvas image viewer",
            PackageCategory::Utilities,
            84 * 1024,
            &["graphics.surface"],
            InstallStatus::Available,
            PackageSource::OfficialRepository,
            "sha256:app-viewer-1.0.0",
            false,
            false,
            false,
        ));

        let _ = self.register(Package::new(
            "org.microos.calc",
            "Calculator",
            "1.1.0",
            "Standard arithmetic and programmer binary/hexadecimal utility",
            PackageCategory::Utilities,
            32 * 1024,
            &["sys.gui"],
            InstallStatus::Available,
            PackageSource::OfficialRepository,
            "sha256:app-calc-1.1.0",
            false,
            false,
            false,
        ));

        let _ = self.register(Package::new(
            "org.microos.clock",
            "Clock & Alarms",
            "0.9.0",
            "World clock, countdown stopwatch, and system timer display",
            PackageCategory::Utilities,
            40 * 1024,
            &["kernel.timer"],
            InstallStatus::Available,
            PackageSource::OfficialRepository,
            "sha256:app-clock-0.9.0",
            false,
            false,
            false,
        ));

        let _ = self.register(Package::new(
            "org.microos.sysinfo",
            "System Information",
            "1.0.0",
            "Detailed hardware manifest, ACPI tables, and PCI device tree viewer",
            PackageCategory::System,
            48 * 1024,
            &["arch.x86_64", "arch.pci"],
            InstallStatus::Available,
            PackageSource::Community,
            "sha256:app-sysinfo-1.0.0",
            false,
            false,
            false,
        ));
    }
}
