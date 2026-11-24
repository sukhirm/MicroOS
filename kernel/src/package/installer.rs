//! Transactional Package Installer for MicroOS APT.
//!
//! Handles safe package installation and removal transactions with dependency validation,
//! built-in package protection, reverse-dependency safety, and transactional rollback.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::package::database::{InstallStatus, PackageDatabase, PackageError};
use crate::package::resolver;

/// Receipt returned upon successful package installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReceipt {
    pub package_id: String,
    pub name: String,
    pub version: String,
    pub size_bytes: usize,
}

/// Receipt returned upon successful package removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveReceipt {
    pub package_id: String,
    pub name: String,
    pub version: String,
    pub freed_bytes: usize,
}

/// Installs a package by ID or short name.
///
/// Steps:
/// 1. Finds target package in `db`.
/// 2. Verifies it is not already installed or built-in.
/// 3. Computes topological installation order for dependencies.
/// 4. Validates payload availability (rejects unbundled catalogue entries honestly).
/// 5. Performs state transitions transactionally, rolling back on error.
pub fn install_package(
    db: &mut PackageDatabase,
    query: &str,
) -> Result<InstallReceipt, PackageError> {
    let target_id = {
        let pkg = db
            .get(query)
            .ok_or_else(|| PackageError::PackageNotFound(query.to_string()))?;

        if pkg.status == InstallStatus::Builtin || pkg.status == InstallStatus::Installed {
            return Err(PackageError::AlreadyInstalled(pkg.name.clone()));
        }

        pkg.id.clone()
    };

    // Calculate topological order of packages that must be installed
    let install_plan = resolver::resolve_install_order(db, &target_id)?;

    // Track state modifications for rollback
    let mut modified_packages: Vec<(String, InstallStatus)> = Vec::new();

    for pkg_id in install_plan {
        let (name, _version, _size_bytes, has_payload) = {
            let pkg = match db.get(&pkg_id) {
                Some(p) => p,
                None => {
                    rollback(db, &modified_packages);
                    return Err(PackageError::PackageNotFound(pkg_id));
                }
            };
            (pkg.name.clone(), pkg.version.clone(), pkg.size_bytes, pkg.has_payload)
        };

        if !has_payload {
            // Mark as Unavailable in database to reflect failed offline fetch
            let _ = db.update_status(&pkg_id, InstallStatus::Unavailable);
            rollback(db, &modified_packages);
            return Err(PackageError::PayloadMissing(name));
        }

        // Apply installation state
        if let Some(pkg) = db.get_mut(&pkg_id) {
            let prev_status = pkg.status;
            pkg.status = InstallStatus::Installed;
            modified_packages.push((pkg_id.clone(), prev_status));
        }
    }

    // Retrieve target receipt info
    let target = db
        .get(&target_id)
        .ok_or_else(|| PackageError::PackageNotFound(target_id.clone()))?;

    Ok(InstallReceipt {
        package_id: target.id.clone(),
        name: target.name.clone(),
        version: target.version.clone(),
        size_bytes: target.size_bytes,
    })
}

/// Rolls back modified package states if a multi-package transaction fails.
fn rollback(db: &mut PackageDatabase, modified: &[(String, InstallStatus)]) {
    for (id, prev_status) in modified.iter().rev() {
        if let Some(pkg) = db.get_mut(id) {
            pkg.status = *prev_status;
        }
    }
}

/// Removes an installed package.
///
/// Steps:
/// 1. Finds target package in `db`.
/// 2. Verifies it is installed (cannot remove uninstalled or available packages).
/// 3. Protects built-in system packages (`is_protected` or `Builtin`).
/// 4. Verifies no other installed package depends on it (reverse-dependency check).
/// 5. Transitions status back to `Available`.
pub fn remove_package(
    db: &mut PackageDatabase,
    query: &str,
) -> Result<RemoveReceipt, PackageError> {
    let target_id = {
        let pkg = db
            .get(query)
            .ok_or_else(|| PackageError::PackageNotFound(query.to_string()))?;

        if pkg.is_protected || pkg.status == InstallStatus::Builtin {
            return Err(PackageError::CannotRemoveProtected(pkg.name.clone()));
        }

        if pkg.status != InstallStatus::Installed {
            return Err(PackageError::PackageNotFound(format!(
                "Package '{}' is not currently installed",
                pkg.name
            )));
        }

        pkg.id.clone()
    };

    // Check if any remaining installed package depends on this package
    resolver::check_reverse_dependencies(db, &target_id)?;

    // Transition back to Available
    let (name, version, freed_bytes) = {
        let pkg = db
            .get_mut(&target_id)
            .ok_or_else(|| PackageError::PackageNotFound(target_id.clone()))?;
        pkg.status = InstallStatus::Available;
        (pkg.name.clone(), pkg.version.clone(), pkg.size_bytes)
    };

    Ok(RemoveReceipt {
        package_id: target_id,
        name,
        version,
        freed_bytes,
    })
}
