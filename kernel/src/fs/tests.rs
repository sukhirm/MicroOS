//! In-Kernel Unit and Self-Tests for the Virtual File System (VFS).
//!
//! Verifies directory listing, folder navigation, path canonicalization,
//! directory creation, file creation, file deletion, invalid paths, and empty directories.

use super::vfs::{FsError, FsNodeKind, Vfs};

/// Runs all in-kernel VFS unit tests.
pub fn run_self_tests() {
    log::info!("================================================================");
    log::info!("          Running Micro-OS Phase 4E VFS Self-Tests              ");
    log::info!("================================================================");

    test_vfs_initial_hierarchy();
    test_vfs_path_canonicalization();
    test_vfs_directory_navigation();
    test_vfs_mkdir_and_empty_dir();
    test_vfs_create_and_read_file();
    test_vfs_file_and_dir_deletion();
    test_vfs_invalid_paths_and_boundaries();

    log::info!("================================================================");
    log::info!("          [OK] All Phase 4E VFS Self-Tests Passed!              ");
    log::info!("================================================================");
}

/// 1. Verifies initial directory listing of sample files and hierarchy.
fn test_vfs_initial_hierarchy() {
    let mut vfs = Vfs::new_empty();
    vfs.init_samples();

    // Verify root listing
    let root_entries = vfs.list_dir("/").expect("Root listing must succeed");
    assert!(root_entries.iter().any(|e| e.name == "home" && e.kind == FsNodeKind::Directory));
    assert!(root_entries.iter().any(|e| e.name == "sys" && e.kind == FsNodeKind::Directory));
    assert!(root_entries.iter().any(|e| e.name == "etc" && e.kind == FsNodeKind::Directory));

    // Verify /home/user listing
    let user_entries = vfs.list_dir("/home/user").expect("/home/user listing must succeed");
    assert!(user_entries.iter().any(|e| e.name == "Documents" && e.kind == FsNodeKind::Directory));
    assert!(user_entries.iter().any(|e| e.name == "Downloads" && e.kind == FsNodeKind::Directory));
    assert!(user_entries.iter().any(|e| e.name == "Pictures" && e.kind == FsNodeKind::Directory));
    assert!(user_entries.iter().any(|e| e.name == "Music" && e.kind == FsNodeKind::Directory));
    assert!(user_entries.iter().any(|e| e.name == "Videos" && e.kind == FsNodeKind::Directory));

    // Verify /home/user/Documents listing
    let doc_entries = vfs.list_dir("/home/user/Documents").expect("Documents listing must succeed");
    assert!(doc_entries.iter().any(|e| e.name == "notes.txt" && e.kind == FsNodeKind::File));
    assert!(doc_entries.iter().any(|e| e.name == "roadmap.md" && e.kind == FsNodeKind::File));

    log::info!("  [PASS] 1. Initial VFS directory listing and sample tree test");
}

/// 2. Verifies path parsing, normalization, and canonicalization.
fn test_vfs_path_canonicalization() {
    assert_eq!(Vfs::canonicalize_path("/"), "/");
    assert_eq!(Vfs::canonicalize_path("///"), "/");
    assert_eq!(Vfs::canonicalize_path("/home/user"), "/home/user");
    assert_eq!(Vfs::canonicalize_path("/home/user/../user"), "/home/user");
    assert_eq!(Vfs::canonicalize_path("/home/user/./Documents/"), "/home/user/Documents");
    assert_eq!(Vfs::canonicalize_path("/../../"), "/");

    log::info!("  [PASS] 2. VFS path canonicalization & relative navigation test");
}

/// 3. Verifies navigating into subdirectories and resolving nodes.
fn test_vfs_directory_navigation() {
    let mut vfs = Vfs::new_empty();
    vfs.init_samples();

    assert!(vfs.is_dir("/"));
    assert!(vfs.is_dir("/home"));
    assert!(vfs.is_dir("/home/user"));
    assert!(vfs.is_dir("/home/user/Documents"));
    assert!(!vfs.is_dir("/home/user/Documents/notes.txt"));
    assert!(vfs.exists("/home/user/Documents/notes.txt"));

    log::info!("  [PASS] 3. VFS hierarchical directory navigation test");
}

/// 4. Verifies creating new directories and handling empty directory listings.
fn test_vfs_mkdir_and_empty_dir() {
    let mut vfs = Vfs::new_empty();

    // Create a new folder
    vfs.mkdir("/projects").expect("mkdir /projects must succeed");
    assert!(vfs.is_dir("/projects"));

    // Listing newly created empty directory must return an empty list without error
    let entries = vfs.list_dir("/projects").expect("Empty dir listing must succeed");
    assert_eq!(entries.len(), 0, "Empty directory must have 0 entries");

    // Creating nested folder
    vfs.mkdir("/projects/micro_os").expect("mkdir /projects/micro_os must succeed");
    assert!(vfs.is_dir("/projects/micro_os"));

    // Attempting to recreate existing directory must return AlreadyExists
    assert_eq!(vfs.mkdir("/projects"), Err(FsError::AlreadyExists));

    log::info!("  [PASS] 4. VFS directory creation & empty directory handling test");
}

/// 5. Verifies creating, reading, and overwriting virtual files.
fn test_vfs_create_and_read_file() {
    let mut vfs = Vfs::new_empty();
    vfs.mkdir("/data").expect("mkdir must succeed");

    let payload = b"Hello, Cosmic Slate Desktop!";
    vfs.create_file("/data/hello.txt", payload).expect("create_file must succeed");

    let read_back = vfs.read_file("/data/hello.txt").expect("read_file must succeed");
    assert_eq!(read_back, payload);

    // Overwriting file
    let updated = b"Updated content in virtual memory";
    vfs.create_file("/data/hello.txt", updated).expect("overwrite must succeed");
    assert_eq!(vfs.read_file("/data/hello.txt").unwrap(), updated);

    log::info!("  [PASS] 5. VFS file creation, reading, and overwriting test");
}

/// 6. Verifies deleting virtual files and directories safely.
fn test_vfs_file_and_dir_deletion() {
    let mut vfs = Vfs::new_empty();
    vfs.mkdir("/tmp").expect("mkdir must succeed");
    vfs.create_file("/tmp/junk.dat", b"1234").expect("create file must succeed");

    assert!(vfs.exists("/tmp/junk.dat"));

    // Delete file
    vfs.remove("/tmp/junk.dat").expect("remove file must succeed");
    assert!(!vfs.exists("/tmp/junk.dat"));

    // Delete folder
    vfs.remove("/tmp").expect("remove directory must succeed");
    assert!(!vfs.exists("/tmp"));

    // Attempting to delete non-existent path returns NotFound
    assert_eq!(vfs.remove("/tmp/nonexistent"), Err(FsError::NotFound));

    log::info!("  [PASS] 6. VFS safe file & directory deletion test");
}

/// 7. Verifies invalid path rejection and navigation boundaries.
fn test_vfs_invalid_paths_and_boundaries() {
    let mut vfs = Vfs::new_empty();
    vfs.init_samples();

    // Cannot delete root
    assert_eq!(vfs.remove("/"), Err(FsError::CannotDeleteRoot));

    // Non-existent path returns NotFound
    assert_eq!(vfs.list_dir("/nonexistent/path"), Err(FsError::NotFound));

    // Trying to list a file as a directory returns NotADirectory
    assert_eq!(
        vfs.list_dir("/home/user/Documents/notes.txt"),
        Err(FsError::NotADirectory)
    );

    // Trying to read a directory as a file returns IsADirectory
    assert_eq!(vfs.read_file("/home/user"), Err(FsError::IsADirectory));

    log::info!("  [PASS] 7. VFS boundary enforcement & invalid path rejection test");
}
