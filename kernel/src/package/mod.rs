//! MicroOS APT Package Management Subsystem.
//!
//! Provides a native Rust package manager inspired by Debian APT:
//! - In-memory bounded package database with categories and metadata
//! - Dependency graph resolution, topological ordering, and cycle detection
//! - Transactional installer with rollback safety and built-in protection
//! - Repository source definitions and honest offline error handling
//! - Terminal `apt` CLI suite (`help`, `list`, `search`, `info`, `update`, `install`, `remove`, `upgrade`)
//! - Integrated synchronization with the desktop Software Store.

pub mod commands;
pub mod database;
pub mod installer;
pub mod repository;
pub mod resolver;
pub mod tests;

/// Initializes the global Package Database with canonical packages and core system components.
pub fn init() {
    database::PACKAGE_DB.lock().init_canonical_packages();
}

/// Runs the automated in-kernel self-test suite for Phase 4E.7.
pub fn run_self_tests() {
    tests::run_self_tests();
}
