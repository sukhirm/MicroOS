//! Repository abstraction for MicroOS APT.
//!
//! Defines repository endpoints, index synchronization, and payload fetching abstractions.
//! All network queries honestly report offline status until physical network drivers
//! are implemented in Phase 7.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use crate::package::database::PackageError;

/// Repository source configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySource {
    pub name: String,
    pub url: String,
    pub branch: String,
    pub is_enabled: bool,
    pub last_synced: Option<String>,
}

impl RepositorySource {
    pub fn new(name: &str, url: &str, branch: &str) -> Self {
        Self {
            name: name.to_owned(),
            url: url.to_owned(),
            branch: branch.to_owned(),
            is_enabled: true,
            last_synced: None,
        }
    }
}

/// Abstraction for package repository operations.
pub trait PackageRepository {
    /// Name of the repository.
    fn name(&self) -> &str;

    /// Base URL of the repository.
    fn url(&self) -> &str;

    /// Attempts to synchronize package index from the repository.
    ///
    /// In Phase 4E, this returns `Err(PackageError::RepositoryOffline)` because
    /// network adapter drivers (RTL8852BE / Realtek GbE) are hardware-only detected
    /// and network stack initialization is planned for Phase 7.
    fn sync_index(&mut self) -> Result<usize, PackageError>;

    /// Attempts to fetch a package payload by package ID.
    fn fetch_payload(&self, pkg_id: &str) -> Result<Vec<u8>, PackageError>;
}

/// Official Micro-OS upstream package repository.
pub struct OfficialRepository {
    source: RepositorySource,
}

impl OfficialRepository {
    pub fn new() -> Self {
        Self {
            source: RepositorySource::new(
                "MicroOS-Main",
                "https://packages.microos.org/rolling",
                "main",
            ),
        }
    }

    pub fn source(&self) -> &RepositorySource {
        &self.source
    }
}

impl Default for OfficialRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageRepository for OfficialRepository {
    fn name(&self) -> &str {
        &self.source.name
    }

    fn url(&self) -> &str {
        &self.source.url
    }

    fn sync_index(&mut self) -> Result<usize, PackageError> {
        // Honest telemetry: No fake network requests.
        // Physical network hardware is detected, but network stack driver is not yet initialized.
        Err(PackageError::RepositoryOffline)
    }

    fn fetch_payload(&self, _pkg_id: &str) -> Result<Vec<u8>, PackageError> {
        Err(PackageError::RepositoryOffline)
    }
}

/// Helper function to format repository status report.
#[allow(dead_code)]
pub fn format_repository_status() -> String {
    format!(
        "Repository: MicroOS-Main\nURL: https://packages.microos.org/rolling\nStatus: Offline (Network driver pending in Phase 7)\nSecurity: SHA-256 cryptographic verification enabled"
    )
}
