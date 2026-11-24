//! Terminal Command Interpreter for MicroOS APT.
//!
//! Handles `apt` subcommands:
//! - `apt help`: Command overview and syntax reference.
//! - `apt list [--installed]`: Lists catalogue or installed packages.
//! - `apt search <term>`: Filters packages by keyword.
//! - `apt info <pkg>`: Displays comprehensive metadata for a package.
//! - `apt update`: Synchronizes repository index (honestly reports offline).
//! - `apt install <pkg>`: Installs package with dependency checks and rollback.
//! - `apt remove <pkg>`: Safely uninstalls non-protected package.
//! - `apt upgrade`: Upgrades installed packages (honestly reports remote status).

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::vec::Vec;

use crate::apps::terminal::TerminalLine;
use crate::graphics::color::Color;
use crate::package::database::{InstallStatus, PackageError, PACKAGE_DB};
use crate::package::installer;
use crate::package::repository::{OfficialRepository, PackageRepository};

/// Dispatches an `apt` command with arguments, returning lines to be appended
/// to the terminal's output buffer.
pub fn handle_apt_command(args: &[&str]) -> Vec<TerminalLine> {
    if args.is_empty() {
        return handle_help();
    }

    match args[0] {
        "help" | "--help" | "-h" => handle_help(),
        "list" => handle_list(&args[1..]),
        "search" => handle_search(&args[1..]),
        "info" | "show" => handle_info(&args[1..]),
        "update" => handle_update(),
        "install" => handle_install(&args[1..]),
        "remove" | "uninstall" | "purge" => handle_remove(&args[1..]),
        "upgrade" => handle_upgrade(),
        unknown => {
            let mut lines = Vec::new();
            lines.push(TerminalLine::new(
                &format!("E: Invalid operation '{}'", unknown),
                Color::DANGER_RED,
            ));
            lines.push(TerminalLine::new(
                "Type 'apt help' for available package manager commands.",
                Color::TEXT_MUTED,
            ));
            lines
        }
    }
}

/// `apt help`: Reference manual.
fn handle_help() -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    lines.push(TerminalLine::new(
        "MicroOS APT (Advanced Package Tool) v0.11.17",
        Color::ACCENT_CYAN,
    ));
    lines.push(TerminalLine::new(
        "Usage: apt <command> [options] [arguments]",
        Color::WHITE,
    ));
    lines.push(TerminalLine::new("", Color::WHITE));
    lines.push(TerminalLine::new(
        "Commands:",
        Color::ACCENT_AZURE,
    ));
    lines.push(TerminalLine::new(
        "  list [--installed]   List all or installed packages in the database",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  search <term>        Search package database by name or keyword",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  info <package>       Show detailed metadata and dependencies",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  update               Fetch updated package indices (reports offline)",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  install <package>    Install a package and its dependencies",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  remove <package>     Safely remove an installed package",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  upgrade              Upgrade installed packages to newest versions",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "  help                 Display this command reference",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new("", Color::WHITE));
    lines.push(TerminalLine::new(
        "Note: MicroOS APT is an educational package manager in pure Rust.",
        Color::TEXT_MUTED,
    ));
    lines.push(TerminalLine::new(
        "      All states are in-memory. Network downloads require Phase 7 drivers.",
        Color::TEXT_MUTED,
    ));
    lines
}

/// `apt list [--installed]`
fn handle_list(args: &[&str]) -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    let db = PACKAGE_DB.lock();

    let only_installed = args.iter().any(|&a| a == "--installed" || a == "-i");

    if only_installed {
        lines.push(TerminalLine::new(
            "Listing installed packages...",
            Color::ACCENT_CYAN,
        ));
        let installed = db.installed();
        for pkg in installed {
            let status_badge = if pkg.is_protected {
                "[built-in, protected]"
            } else {
                "[installed]"
            };
            let color = if pkg.is_protected {
                Color::TEXT_MUTED
            } else {
                Color::SUCCESS_GREEN
            };

            lines.push(TerminalLine::new(
                &format!(
                    "{}/{} {} {} - {}",
                    pkg.id,
                    pkg.category.as_str().to_lowercase(),
                    pkg.version,
                    status_badge,
                    pkg.name
                ),
                color,
            ));
        }
        lines.push(TerminalLine::new(
            &format!("Total installed: {} packages", db.installed().len()),
            Color::TEXT_MUTED,
        ));
    } else {
        lines.push(TerminalLine::new(
            "Listing all catalogue packages...",
            Color::ACCENT_CYAN,
        ));
        for pkg in db.all() {
            let (status_str, color) = match pkg.status {
                InstallStatus::Builtin => ("[built-in]", Color::TEXT_MUTED),
                InstallStatus::Installed => ("[installed]", Color::SUCCESS_GREEN),
                InstallStatus::Available => ("[available]", Color::ACCENT_AZURE),
                InstallStatus::Unavailable => ("[unavailable]", Color::WARNING_AMBER),
            };

            lines.push(TerminalLine::new(
                &format!(
                    "{:24} {:10} {:14} {}",
                    pkg.id,
                    pkg.version,
                    status_str,
                    pkg.name
                ),
                color,
            ));
        }
        lines.push(TerminalLine::new(
            &format!("Total catalogue entries: {} packages", db.len()),
            Color::TEXT_MUTED,
        ));
    }

    lines
}

/// `apt search <term>`
fn handle_search(args: &[&str]) -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    if args.is_empty() {
        lines.push(TerminalLine::new(
            "E: Search term missing. Usage: apt search <keyword>",
            Color::DANGER_RED,
        ));
        return lines;
    }

    let query = args.join(" ");
    let db = PACKAGE_DB.lock();
    let matches = db.search(&query);

    lines.push(TerminalLine::new(
        &format!("Sorting... Done\nFull Text Search... Done (found {})", matches.len()),
        Color::TEXT_MUTED,
    ));

    if matches.is_empty() {
        lines.push(TerminalLine::new(
            &format!("No packages found matching '{}'.", query),
            Color::WARNING_AMBER,
        ));
        return lines;
    }

    for pkg in matches {
        let status_desc = match pkg.status {
            InstallStatus::Builtin => "[built-in]",
            InstallStatus::Installed => "[installed]",
            InstallStatus::Available => "[available]",
            InstallStatus::Unavailable => "[unavailable]",
        };

        lines.push(TerminalLine::new(
            &format!("{}/{} {} {}", pkg.id, pkg.category.as_str().to_lowercase(), pkg.version, status_desc),
            Color::ACCENT_CYAN,
        ));
        lines.push(TerminalLine::new(
            &format!("  {} - {}", pkg.name, pkg.description),
            Color::TEXT_LIGHT,
        ));
    }

    lines
}

/// `apt info <package>`
fn handle_info(args: &[&str]) -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    if args.is_empty() {
        lines.push(TerminalLine::new(
            "E: Package name missing. Usage: apt info <package>",
            Color::DANGER_RED,
        ));
        return lines;
    }

    let query = args[0];
    let db = PACKAGE_DB.lock();

    match db.get(query) {
        Some(pkg) => {
            lines.push(TerminalLine::new(
                &format!("Package: {}", pkg.id),
                Color::ACCENT_CYAN,
            ));
            lines.push(TerminalLine::new(
                &format!("Name: {}", pkg.name),
                Color::WHITE,
            ));
            lines.push(TerminalLine::new(
                &format!("Version: {}", pkg.version),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Category: {}", pkg.category.as_str()),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Status: {}", pkg.status.as_str()),
                match pkg.status {
                    InstallStatus::Builtin | InstallStatus::Installed => Color::SUCCESS_GREEN,
                    InstallStatus::Available => Color::ACCENT_AZURE,
                    InstallStatus::Unavailable => Color::WARNING_AMBER,
                },
            ));
            lines.push(TerminalLine::new(
                &format!("Installed-Size: {}", pkg.formatted_size()),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Source: {}", pkg.source.as_str()),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Checksum: {}", pkg.checksum),
                Color::TEXT_MUTED,
            ));

            let deps = if pkg.dependencies.is_empty() {
                "none".to_owned()
            } else {
                pkg.dependencies.join(", ")
            };
            lines.push(TerminalLine::new(
                &format!("Depends: {}", deps),
                Color::TEXT_LIGHT,
            ));

            lines.push(TerminalLine::new(
                &format!("Protected: {}", if pkg.is_protected { "yes" } else { "no" }),
                Color::TEXT_MUTED,
            ));
            lines.push(TerminalLine::new(
                &format!("Description:\n  {}", pkg.description),
                Color::TEXT_LIGHT,
            ));
        }
        None => {
            lines.push(TerminalLine::new(
                &format!("E: Unable to locate package '{}'", query),
                Color::DANGER_RED,
            ));
        }
    }

    lines
}

/// `apt update`
fn handle_update() -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    let mut repo = OfficialRepository::new();

    lines.push(TerminalLine::new(
        &format!("Hit:1 {}/{} InRelease", repo.url(), repo.source().branch),
        Color::TEXT_MUTED,
    ));

    match repo.sync_index() {
        Ok(count) => {
            lines.push(TerminalLine::new(
                &format!("Reading package lists... Done ({} packages)", count),
                Color::SUCCESS_GREEN,
            ));
        }
        Err(PackageError::RepositoryOffline) => {
            lines.push(TerminalLine::new(
                &format!("Err:1 {}/{} InRelease", repo.url(), repo.source().branch),
                Color::WARNING_AMBER,
            ));
            lines.push(TerminalLine::new(
                "  Could not connect to packages.microos.org: Network offline",
                Color::WARNING_AMBER,
            ));
            lines.push(TerminalLine::new(
                "W: Realtek RTL8852BE / Realtek GbE hardware detected on PCI bus.",
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                "W: Network stack and device driver implementation planned for Phase 7.",
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                "W: Package indices could not be refreshed from remote server.",
                Color::TEXT_MUTED,
            ));
            lines.push(TerminalLine::new(
                "Reading local package database... Done.",
                Color::SUCCESS_GREEN,
            ));
        }
        Err(e) => {
            lines.push(TerminalLine::new(
                &format!("E: Failed to update package index: {:?}", e),
                Color::DANGER_RED,
            ));
        }
    }

    lines
}

/// `apt install <package>`
fn handle_install(args: &[&str]) -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    if args.is_empty() {
        lines.push(TerminalLine::new(
            "E: Package name missing. Usage: apt install <package>",
            Color::DANGER_RED,
        ));
        return lines;
    }

    let query = args[0];
    let mut db = PACKAGE_DB.lock();

    lines.push(TerminalLine::new(
        "Reading package lists... Done",
        Color::TEXT_MUTED,
    ));
    lines.push(TerminalLine::new(
        "Building dependency tree... Done",
        Color::TEXT_MUTED,
    ));

    match installer::install_package(&mut db, query) {
        Ok(receipt) => {
            lines.push(TerminalLine::new(
                &format!("Selecting previously unselected package {}...", receipt.package_id),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Unpacking {} ({}) ...", receipt.name, receipt.version),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Setting up {} ({}) ...", receipt.name, receipt.version),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Success: {} installed successfully.", receipt.name),
                Color::SUCCESS_GREEN,
            ));
        }
        Err(PackageError::AlreadyInstalled(name)) => {
            lines.push(TerminalLine::new(
                &format!("{} is already the newest version.", name),
                Color::ACCENT_AZURE,
            ));
            lines.push(TerminalLine::new(
                "0 upgraded, 0 newly installed, 0 to remove.",
                Color::TEXT_MUTED,
            ));
        }
        Err(PackageError::PackageNotFound(pkg)) => {
            lines.push(TerminalLine::new(
                &format!("E: Unable to locate package '{}'", pkg),
                Color::DANGER_RED,
            ));
        }
        Err(PackageError::PayloadMissing(name)) => {
            lines.push(TerminalLine::new(
                &format!("The following NEW package will be installed:\n  {}", query),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Err: Payload for package '{}' is unavailable in local image.", name),
                Color::DANGER_RED,
            ));
            lines.push(TerminalLine::new(
                "E: Remote repository download failed: network hardware offline (Phase 7 driver pending).",
                Color::WARNING_AMBER,
            ));
            lines.push(TerminalLine::new(
                "E: Package marked as 'unavailable' in catalogue.",
                Color::TEXT_MUTED,
            ));
        }
        Err(PackageError::MissingDependency(dep)) => {
            lines.push(TerminalLine::new(
                &format!("E: Unmet dependency: package requires '{}' which is not available.", dep),
                Color::DANGER_RED,
            ));
        }
        Err(PackageError::CyclicDependency(cycle)) => {
            lines.push(TerminalLine::new(
                &format!("E: Cyclic dependency detected during resolution: {}", cycle),
                Color::DANGER_RED,
            ));
        }
        Err(other) => {
            lines.push(TerminalLine::new(
                &format!("E: Installation transaction aborted: {:?}", other),
                Color::DANGER_RED,
            ));
        }
    }

    lines
}

/// `apt remove <package>`
fn handle_remove(args: &[&str]) -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    if args.is_empty() {
        lines.push(TerminalLine::new(
            "E: Package name missing. Usage: apt remove <package>",
            Color::DANGER_RED,
        ));
        return lines;
    }

    let query = args[0];
    let mut db = PACKAGE_DB.lock();

    lines.push(TerminalLine::new(
        "Reading package lists... Done",
        Color::TEXT_MUTED,
    ));
    lines.push(TerminalLine::new(
        "Building dependency tree... Done",
        Color::TEXT_MUTED,
    ));

    match installer::remove_package(&mut db, query) {
        Ok(receipt) => {
            lines.push(TerminalLine::new(
                &format!("Removing {} ({}) ...", receipt.name, receipt.version),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Purging package state for {} ...", receipt.package_id),
                Color::TEXT_LIGHT,
            ));
            lines.push(TerminalLine::new(
                &format!("Success: {} removed successfully (freed {} bytes).", receipt.name, receipt.freed_bytes),
                Color::SUCCESS_GREEN,
            ));
        }
        Err(PackageError::CannotRemoveProtected(name)) => {
            lines.push(TerminalLine::new(
                &format!("E: Cannot remove protected system component '{}'", name),
                Color::DANGER_RED,
            ));
            lines.push(TerminalLine::new(
                "W: Essential system packages are required for operating system stability.",
                Color::WARNING_AMBER,
            ));
        }
        Err(PackageError::DependencyRequired(dependent, target)) => {
            lines.push(TerminalLine::new(
                &format!("E: Cannot remove '{}': '{}' depends on it.", target, dependent),
                Color::DANGER_RED,
            ));
            lines.push(TerminalLine::new(
                "W: Removing this package would break installed software.",
                Color::WARNING_AMBER,
            ));
        }
        Err(PackageError::PackageNotFound(msg)) => {
            lines.push(TerminalLine::new(
                &format!("E: {}", msg),
                Color::DANGER_RED,
            ));
        }
        Err(other) => {
            lines.push(TerminalLine::new(
                &format!("E: Package removal aborted: {:?}", other),
                Color::DANGER_RED,
            ));
        }
    }

    lines
}

/// `apt upgrade`
fn handle_upgrade() -> Vec<TerminalLine> {
    let mut lines = Vec::new();
    lines.push(TerminalLine::new(
        "Reading package lists... Done",
        Color::TEXT_MUTED,
    ));
    lines.push(TerminalLine::new(
        "Building dependency tree... Done",
        Color::TEXT_MUTED,
    ));
    lines.push(TerminalLine::new(
        "Calculating upgrade... Done",
        Color::TEXT_MUTED,
    ));
    lines.push(TerminalLine::new(
        "0 upgraded, 0 newly installed, 0 to remove and 0 not upgraded.",
        Color::SUCCESS_GREEN,
    ));
    lines.push(TerminalLine::new(
        "All installed packages are up to date with the local catalogue.",
        Color::TEXT_LIGHT,
    ));
    lines.push(TerminalLine::new(
        "Remote repository upgrades require network driver (Phase 7).",
        Color::TEXT_MUTED,
    ));
    lines
}
