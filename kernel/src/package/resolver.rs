//! Dependency Resolver for MicroOS APT.
//!
//! Provides dependency graph validation, topological install order calculation,
//! cyclic dependency detection, and reverse-dependency protection against accidental removal.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::package::database::{InstallStatus, Package, PackageDatabase, PackageError};

/// Checks if all direct dependencies of a package exist and are satisfied.
/// Returns the list of uninstalled dependency IDs that must be installed first.
pub fn check_dependencies(
    db: &PackageDatabase,
    pkg: &Package,
) -> Result<Vec<String>, PackageError> {
    let mut missing_or_uninstalled = Vec::new();

    for dep_id in &pkg.dependencies {
        match db.get(dep_id) {
            None => {
                return Err(PackageError::MissingDependency(dep_id.clone()));
            }
            Some(dep_pkg) => {
                match dep_pkg.status {
                    InstallStatus::Builtin | InstallStatus::Installed => {
                        // Satisfied!
                    }
                    InstallStatus::Available | InstallStatus::Unavailable => {
                        missing_or_uninstalled.push(dep_id.clone());
                    }
                }
            }
        }
    }

    Ok(missing_or_uninstalled)
}

/// Recursively resolves the complete installation plan in topological order.
///
/// Returns a list of package IDs that need to be installed in the order they must be installed,
/// ending with the target package. Detects cyclic dependencies.
pub fn resolve_install_order(
    db: &PackageDatabase,
    target_pkg_id: &str,
) -> Result<Vec<String>, PackageError> {
    let target = db
        .get(target_pkg_id)
        .ok_or_else(|| PackageError::PackageNotFound(target_pkg_id.to_string()))?;

    let mut order = Vec::new();
    let mut visiting = Vec::new();

    resolve_internal(db, target, &mut order, &mut visiting)?;
    Ok(order)
}

fn resolve_internal(
    db: &PackageDatabase,
    current: &Package,
    order: &mut Vec<String>,
    visiting: &mut Vec<String>,
) -> Result<(), PackageError> {
    if visiting.contains(&current.id) {
        return Err(PackageError::CyclicDependency(format!(
            "{} -> {}",
            visiting.join(" -> "),
            current.id
        )));
    }

    // If already in order or already installed/builtin, nothing more to do
    if order.contains(&current.id)
        || current.status == InstallStatus::Builtin
        || current.status == InstallStatus::Installed
    {
        return Ok(());
    }

    visiting.push(current.id.clone());

    for dep_id in &current.dependencies {
        let dep_pkg = db
            .get(dep_id)
            .ok_or_else(|| PackageError::MissingDependency(dep_id.clone()))?;

        resolve_internal(db, dep_pkg, order, visiting)?;
    }

    visiting.pop();

    if !order.contains(&current.id) {
        order.push(current.id.clone());
    }

    Ok(())
}

/// Checks reverse dependencies before removing a package.
///
/// If any currently installed or built-in package depends on `pkg_id`,
/// removal must be rejected to prevent breaking dependent software.
pub fn check_reverse_dependencies(
    db: &PackageDatabase,
    pkg_id: &str,
) -> Result<(), PackageError> {
    let target = db
        .get(pkg_id)
        .ok_or_else(|| PackageError::PackageNotFound(pkg_id.to_string()))?;

    for installed in db.installed() {
        if installed.id == target.id {
            continue;
        }

        if installed.dependencies.iter().any(|d| d == &target.id) {
            return Err(PackageError::DependencyRequired(
                installed.name.clone(),
                target.name.clone(),
            ));
        }
    }

    Ok(())
}
