//! Virtual File System Subsystem for Micro-OS (Phase 4E).
//!
//! Provides in-memory hierarchical directory and file management for desktop applications.

pub mod tests;
pub mod vfs;

#[allow(unused_imports)]
pub use vfs::{FsEntry, FsError, FsNode, FsNodeKind, Vfs, VFS};
pub use tests::run_self_tests;

/// Initializes the global VFS with canonical sample desktop files.
pub fn init() {
    VFS.lock().init_samples();
}
