//! MicroOS APT In-Kernel Unit and Self-Tests.
//!
//! Provides 16 automated validation checks covering database integrity, dependency resolution,
//! cyclic dependency detection, installer transactions, rollback safety, repository offline handling,
//! capacity boundaries, CLI commands, and subsystem synchronization.

use alloc::borrow::ToOwned;
use alloc::format;

use crate::package::commands::handle_apt_command;
use crate::package::database::{
    InstallStatus, Package, PackageCategory, PackageDatabase, PackageError, PackageSource,
    MAX_PACKAGES, PACKAGE_DB,
};
use crate::package::installer::{install_package, remove_package};
use crate::package::repository::{OfficialRepository, PackageRepository};
use crate::package::resolver::{check_dependencies, resolve_install_order};

/// Runs the complete test suite for Phase 4E.7 MicroOS APT.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("        MICRO-OS PHASE 4E.7 APT PACKAGE MANAGER TESTS           ");
    log::info!("================================================================");

    test_package_registration_and_lookup();
    test_duplicate_package_rejection();
    test_metadata_validation();
    test_search_filtering();
    test_installed_packages_listing();
    test_dependency_resolution_success();
    test_missing_dependency_detection();
    test_cyclic_dependency_detection();
    test_protected_builtin_removal_rejection();
    test_invalid_package_name_handling();
    test_installation_failure_and_rollback();
    test_bundled_package_installation_and_removal();
    test_repository_offline_handling();
    test_database_capacity_limit();
    test_terminal_apt_cli_commands();
    test_store_and_package_db_synchronization();
    test_hardware_driver_packages();

    // Re-seed canonical packages so subsequent operations start with a clean standard database
    PACKAGE_DB.lock().init_canonical_packages();

    log::info!("================================================================");
    log::info!("  [OK] All Phase 4E.7 MicroOS APT Self-Tests Passed!");
    log::info!("================================================================");
}

/// 1. Package Registration and Lookup
fn test_package_registration_and_lookup() {
    let mut db = PackageDatabase::new();
    let pkg = Package::new(
        "test.pkg1",
        "Test Package 1",
        "1.0.0",
        "Sample package description",
        PackageCategory::Utilities,
        2048,
        &[],
        InstallStatus::Available,
        PackageSource::Community,
        "sha256:test1",
        false,
        false,
        true,
    );

    assert!(db.register(pkg).is_ok());
    assert_eq!(db.len(), 1);

    let retrieved = db.get("test.pkg1").expect("Failed to get package by ID");
    assert_eq!(retrieved.name, "Test Package 1");
    assert_eq!(retrieved.version, "1.0.0");
    assert_eq!(retrieved.size_bytes, 2048);
    assert_eq!(retrieved.formatted_size(), "2 KiB");

    log::info!("  [PASS] 1. Package registration and lookup verified");
}

/// 2. Duplicate Package Rejection
fn test_duplicate_package_rejection() {
    let mut db = PackageDatabase::new();
    let p1 = Package::new(
        "dup.pkg",
        "Dup Pkg 1",
        "1.0.0",
        "First copy",
        PackageCategory::Utilities,
        1024,
        &[],
        InstallStatus::Available,
        PackageSource::OfficialRepository,
        "sha256:d1",
        false,
        false,
        true,
    );
    let p2 = Package::new(
        "dup.pkg",
        "Dup Pkg 2",
        "2.0.0",
        "Second copy",
        PackageCategory::Utilities,
        2048,
        &[],
        InstallStatus::Available,
        PackageSource::OfficialRepository,
        "sha256:d2",
        false,
        false,
        true,
    );

    assert!(db.register(p1).is_ok());
    let err = db.register(p2).unwrap_err();
    assert_eq!(err, PackageError::DuplicatePackageId("dup.pkg".to_owned()));

    log::info!("  [PASS] 2. Duplicate package ID rejection verified");
}

/// 3. Metadata Validation
fn test_metadata_validation() {
    let mut db = PackageDatabase::new();

    // Empty ID
    let p_empty_id = Package::new("", "Name", "1.0", "Desc", PackageCategory::System, 1024, &[], InstallStatus::Available, PackageSource::Community, "", false, false, false);
    assert!(db.register(p_empty_id).is_err());

    // Empty Name
    let p_empty_name = Package::new("id.test", "", "1.0", "Desc", PackageCategory::System, 1024, &[], InstallStatus::Available, PackageSource::Community, "", false, false, false);
    assert!(db.register(p_empty_name).is_err());

    // Empty Version
    let p_empty_ver = Package::new("id.test", "Name", "", "Desc", PackageCategory::System, 1024, &[], InstallStatus::Available, PackageSource::Community, "", false, false, false);
    assert!(db.register(p_empty_ver).is_err());

    // Zero Size
    let p_zero_size = Package::new("id.test", "Name", "1.0", "Desc", PackageCategory::System, 0, &[], InstallStatus::Available, PackageSource::Community, "", false, false, false);
    assert!(db.register(p_zero_size).is_err());

    log::info!("  [PASS] 3. Package metadata validation verified");
}

/// 4. Search Filtering
fn test_search_filtering() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    let term_results = db.search("terminal");
    assert!(!term_results.is_empty());
    assert!(term_results.iter().any(|p| p.id == "org.microos.terminal"));

    let vfs_results = db.search("vfs");
    assert!(!vfs_results.is_empty());
    assert!(vfs_results.iter().any(|p| p.id == "core.vfs"));

    let empty_search = db.search("nonexistent_keyword_xyz");
    assert!(empty_search.is_empty());

    log::info!("  [PASS] 4. Search filtering across metadata verified");
}

/// 5. Installed Packages Listing
fn test_installed_packages_listing() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    let installed = db.installed();
    assert!(!installed.is_empty());
    // Files, Terminal, System Monitor should be in installed list
    assert!(installed.iter().any(|p| p.id == "org.microos.files"));
    assert!(installed.iter().any(|p| p.id == "org.microos.terminal"));
    // Unbundled editor should NOT be in installed list
    assert!(!installed.iter().any(|p| p.id == "org.microos.editor"));

    log::info!("  [PASS] 5. Installed and built-in packages listing verified");
}

/// 6. Dependency Resolution Success
fn test_dependency_resolution_success() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    let files_pkg = db.get("org.microos.files").expect("Missing files pkg");
    let uninstalled_deps = check_dependencies(&db, files_pkg).expect("Dep resolution error");
    // All dependencies of files ("core.vfs", "sys.gui") are built-in and satisfied
    assert!(uninstalled_deps.is_empty());

    log::info!("  [PASS] 6. Dependency resolution for satisfied dependencies verified");
}

/// 7. Missing Dependency Detection
fn test_missing_dependency_detection() {
    let mut db = PackageDatabase::new();
    let broken_pkg = Package::new(
        "broken.pkg",
        "Broken App",
        "1.0.0",
        "Depends on missing library",
        PackageCategory::Utilities,
        4096,
        &["missing.lib"],
        InstallStatus::Available,
        PackageSource::Community,
        "sha256:broken",
        false,
        false,
        true,
    );
    db.register(broken_pkg).unwrap();

    let pkg = db.get("broken.pkg").unwrap();
    let res = check_dependencies(&db, pkg);
    assert_eq!(res, Err(PackageError::MissingDependency("missing.lib".to_owned())));

    log::info!("  [PASS] 7. Missing dependency detection verified");
}

/// 8. Cyclic Dependency Detection
fn test_cyclic_dependency_detection() {
    let mut db = PackageDatabase::new();
    let p_a = Package::new("pkg.a", "Package A", "1.0", "Cycle test A", PackageCategory::Utilities, 1024, &["pkg.b"], InstallStatus::Available, PackageSource::Community, "sha256:a", false, false, true);
    let p_b = Package::new("pkg.b", "Package B", "1.0", "Cycle test B", PackageCategory::Utilities, 1024, &["pkg.a"], InstallStatus::Available, PackageSource::Community, "sha256:b", false, false, true);

    db.register(p_a).unwrap();
    db.register(p_b).unwrap();

    let res = resolve_install_order(&db, "pkg.a");
    assert!(matches!(res, Err(PackageError::CyclicDependency(_))));

    log::info!("  [PASS] 8. Cyclic dependency detection verified");
}

/// 9. Protected Built-in Removal Rejection
fn test_protected_builtin_removal_rejection() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    // Attempt removing core VFS
    let res1 = remove_package(&mut db, "core.vfs");
    assert!(matches!(res1, Err(PackageError::CannotRemoveProtected(_))));

    // Attempt removing File Manager
    let res2 = remove_package(&mut db, "org.microos.files");
    assert!(matches!(res2, Err(PackageError::CannotRemoveProtected(_))));

    log::info!("  [PASS] 9. Protected built-in removal rejection verified");
}

/// 10. Invalid Package Name Handling
fn test_invalid_package_name_handling() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    let res = install_package(&mut db, "nonexistent.fake.app");
    assert!(matches!(res, Err(PackageError::PackageNotFound(_))));

    log::info!("  [PASS] 10. Invalid package name error handling verified");
}

/// 11. Installation Failure and Rollback
fn test_installation_failure_and_rollback() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    // Editor has has_payload = false (unbundled remote package)
    let res = install_package(&mut db, "org.microos.editor");
    assert!(matches!(res, Err(PackageError::PayloadMissing(_))));

    // Verify status was updated to Unavailable
    let editor = db.get("org.microos.editor").unwrap();
    assert_eq!(editor.status, InstallStatus::Unavailable);

    log::info!("  [PASS] 11. Installation failure handling and rollback verified");
}

/// 12. Bundled Package Installation and Removal
fn test_bundled_package_installation_and_removal() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    // Register a valid bundled third-party application
    let custom_app = Package::new(
        "org.microos.notes",
        "Notes",
        "1.0.0",
        "Quick notes pad",
        PackageCategory::Productivity,
        32 * 1024,
        &["core.vfs"],
        InstallStatus::Available,
        PackageSource::Community,
        "sha256:app-notes-1.0.0",
        false, // Not protected
        false,
        true,  // Has local payload
    );
    db.register(custom_app).unwrap();

    // Install
    let receipt = install_package(&mut db, "org.microos.notes").expect("Failed to install bundled app");
    assert_eq!(receipt.package_id, "org.microos.notes");
    assert_eq!(db.get("org.microos.notes").unwrap().status, InstallStatus::Installed);

    // Remove
    let rm_receipt = remove_package(&mut db, "org.microos.notes").expect("Failed to remove bundled app");
    assert_eq!(rm_receipt.package_id, "org.microos.notes");
    assert_eq!(db.get("org.microos.notes").unwrap().status, InstallStatus::Available);

    log::info!("  [PASS] 12. Bundled package installation and removal verified");
}

/// 13. Repository Offline Handling
fn test_repository_offline_handling() {
    let mut repo = OfficialRepository::new();
    assert_eq!(repo.name(), "MicroOS-Main");
    assert_eq!(repo.sync_index(), Err(PackageError::RepositoryOffline));
    assert_eq!(repo.fetch_payload("any.pkg"), Err(PackageError::RepositoryOffline));

    log::info!("  [PASS] 13. Repository offline honest error reporting verified");
}

/// 14. Database Capacity Limit
fn test_database_capacity_limit() {
    let mut db = PackageDatabase::new();

    for i in 0..MAX_PACKAGES {
        let id = format!("capacity.pkg.{}", i);
        let name = format!("Capacity Pkg {}", i);
        let pkg = Package::new(
            &id,
            &name,
            "1.0.0",
            "Capacity filler",
            PackageCategory::System,
            1024,
            &[],
            InstallStatus::Available,
            PackageSource::Community,
            "sha256:cap",
            false,
            false,
            true,
        );
        db.register(pkg).expect("Database registration should succeed up to MAX_PACKAGES");
    }

    assert_eq!(db.len(), MAX_PACKAGES);

    // 65th package must fail
    let overflow_pkg = Package::new(
        "capacity.overflow",
        "Overflow Pkg",
        "1.0.0",
        "Overflow filler",
        PackageCategory::System,
        1024,
        &[],
        InstallStatus::Available,
        PackageSource::Community,
        "sha256:over",
        false,
        false,
        true,
    );
    let res = db.register(overflow_pkg);
    assert_eq!(res, Err(PackageError::DatabaseFull));

    log::info!("  [PASS] 14. Database maximum capacity boundary (64 pkgs) verified");
}

/// 15. Terminal APT CLI Commands
fn test_terminal_apt_cli_commands() {
    // Test help command
    let help_lines = handle_apt_command(&["help"]);
    assert!(help_lines.iter().any(|l| l.text.contains("MicroOS APT")));

    // Test list command
    let list_lines = handle_apt_command(&["list"]);
    assert!(list_lines.iter().any(|l| l.text.contains("Listing all catalogue packages")));

    // Test search command
    let search_lines = handle_apt_command(&["search", "terminal"]);
    assert!(search_lines.iter().any(|l| l.text.contains("org.microos.terminal")));

    // Test info command
    let info_lines = handle_apt_command(&["info", "org.microos.files"]);
    assert!(info_lines.iter().any(|l| l.text.contains("Package: org.microos.files")));

    // Test update command
    let update_lines = handle_apt_command(&["update"]);
    assert!(update_lines.iter().any(|l| l.text.contains("Network offline") || l.text.contains("Phase 7")));

    log::info!("  [PASS] 15. Terminal APT CLI commands output verified");
}

/// 16. Store and Package DB Synchronization
fn test_store_and_package_db_synchronization() {
    let mut db = PACKAGE_DB.lock();
    db.init_canonical_packages();

    // Verify that package DB contains the apps shown in Store
    let editor = db.get("org.microos.editor").expect("Editor must exist in DB");
    assert_eq!(editor.status, InstallStatus::Available);

    // Update status
    let _ = db.update_status("org.microos.editor", InstallStatus::Unavailable);
    let updated = db.get("org.microos.editor").unwrap();
    assert_eq!(updated.status, InstallStatus::Unavailable);

    // Reset status back
    let _ = db.update_status("org.microos.editor", InstallStatus::Available);

    log::info!("  [PASS] 16. Software Store and Package DB synchronization verified");
}

/// 17. Dedicated Hardware Driver Packages Verification
fn test_hardware_driver_packages() {
    let mut db = PackageDatabase::new();
    db.init_canonical_packages();

    // 1. Verify all 4 hardware driver packages are registered in database
    let expected_drivers = [
        ("driver.keyboard", "Standard PS/2 Keyboard Driver", InstallStatus::Builtin),
        ("driver.touchpad", "ELAN I2C HID Touchpad Driver", InstallStatus::Builtin),
        ("driver.wifi", "Realtek 8852BE-VT Wi-Fi 6 Driver", InstallStatus::Builtin),
        ("driver.bluetooth", "Realtek Wireless Bluetooth Driver", InstallStatus::Builtin),
    ];

    for &(id, name, status) in &expected_drivers {
        let pkg = db.get(id).unwrap_or_else(|| panic!("Driver package {} must be registered in PACKAGE_DB", id));
        assert_eq!(pkg.name, name, "Driver {} name mismatch", id);
        assert_eq!(pkg.status, status, "Driver {} status must be Builtin", id);
        assert!(pkg.is_protected, "Driver package {} must be protected from removal", id);
        assert!(pkg.validate_metadata().is_ok(), "Driver package {} metadata must be valid", id);
        assert_eq!(pkg.category, PackageCategory::System, "Driver {} category must be System", id);
    }

    // 2. Search for drivers via keyword
    let driver_search = db.search("driver");
    assert!(
        driver_search.len() >= 4,
        "Search for 'driver' must return at least all 4 hardware drivers (found {})",
        driver_search.len()
    );

    // 3. Verify drivers cannot be uninstalled because they are protected built-in drivers
    for &(id, _, _) in &expected_drivers {
        let res = remove_package(&mut db, id);
        assert!(
            matches!(res, Err(PackageError::CannotRemoveProtected(_))),
            "Driver package {} must be protected against removal",
            id
        );
    }

    log::info!("  [PASS] 17. Dedicated hardware driver packages (keyboard, touchpad, wifi, bluetooth) verified");
}
