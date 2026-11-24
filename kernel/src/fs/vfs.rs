//! In-Memory Virtual File System (VFS).
//!
//! Provides a tree-structured, in-memory virtual filesystem supporting directories,
//! files, path normalization, safe hierarchical navigation, and CRUD operations.
//!
//! Note: This is an in-memory VFS designed for desktop shell applications and testing;
//! it does not provide persistent disk storage.

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Global spinlock-protected virtual filesystem instance initialized with sample desktop files.
pub static VFS: Mutex<Vfs> = Mutex::new(Vfs::new_with_samples());

/// Node type in the virtual filesystem tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsNodeKind {
    Directory,
    File,
}

/// Directory listing entry metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    pub kind: FsNodeKind,
    pub size: usize,
}

/// Errors that can occur during VFS operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    InvalidPath,
    CannotDeleteRoot,
}

/// An individual inode in the in-memory filesystem tree.
#[derive(Clone)]
pub struct FsNode {
    pub name: String,
    pub kind: FsNodeKind,
    pub content: Vec<u8>,
    pub children: Vec<FsNode>,
}

impl FsNode {
    /// Creates a new directory node.
    pub fn new_directory(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            kind: FsNodeKind::Directory,
            content: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Creates a new file node with the given byte content.
    pub fn new_file(name: &str, content: &[u8]) -> Self {
        Self {
            name: name.to_owned(),
            kind: FsNodeKind::File,
            content: content.to_vec(),
            children: Vec::new(),
        }
    }

    /// Finds a child node with the given name.
    pub fn find_child(&self, name: &str) -> Option<&FsNode> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Finds a mutable child node with the given name.
    pub fn find_child_mut(&mut self, name: &str) -> Option<&mut FsNode> {
        self.children.iter_mut().find(|c| c.name == name)
    }
}

/// In-memory Virtual Filesystem hierarchy.
pub struct Vfs {
    root: FsNode,
}

impl Vfs {
    /// Constructs an empty VFS containing only the root directory `/`.
    pub fn new_empty() -> Self {
        Self {
            root: FsNode::new_directory(""),
        }
    }

    /// Constructs a VFS populated with canonical Cosmic Slate desktop hierarchy and sample files.
    pub const fn new_with_samples() -> Self {
        // Const constructor with empty root; samples will be dynamically added or initialized
        Self {
            root: FsNode {
                name: String::new(),
                kind: FsNodeKind::Directory,
                content: Vec::new(),
                children: Vec::new(),
            },
        }
    }

    /// Populates sample folders and files into the VFS.
    pub fn init_samples(&mut self) {
        let _ = self.mkdir("/home");
        let _ = self.mkdir("/home/user");
        let _ = self.mkdir("/home/user/Documents");
        let _ = self.mkdir("/home/user/Downloads");
        let _ = self.mkdir("/home/user/Pictures");
        let _ = self.mkdir("/home/user/Music");
        let _ = self.mkdir("/home/user/Videos"); // Intentionally empty directory for testing

        let _ = self.mkdir("/sys");
        let _ = self.mkdir("/etc");

        let _ = self.create_file(
            "/home/user/Documents/notes.txt",
            b"Welcome to Micro-OS!\nAn educational, bare-metal operating system written in Rust.\nEnjoy the Obsidian Azure desktop environment.",
        );
        let _ = self.create_file(
            "/home/user/Documents/roadmap.md",
            b"# Micro-OS Development Roadmap\n- Phase 4E: Desktop Applications (Files, SysMon, Settings)\n- Phase 5: Bare-Metal Physical Boot on HP Victus\n- Phase 6: Storage Drivers & Persistent FAT32",
        );
        let _ = self.create_file(
            "/home/user/Downloads/kernel-debug.bin",
            &[0x7F, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00],
        );
        let _ = self.create_file(
            "/home/user/Pictures/wallpaper.bmp",
            b"Cosmic Slate & Obsidian Azure 1920x1080 Wallpaper Canvas",
        );
        let _ = self.create_file(
            "/home/user/Music/startup.wav",
            b"RIFF....WAVEfmt ....data....Cosmic Chime",
        );

        let _ = self.create_file(
            "/sys/version.txt",
            b"Micro-OS v0.11.17 (x86_64 Long Mode, UEFI GOP, 16 MiB Backbuffer)",
        );
        let _ = self.create_file(
            "/sys/memory.txt",
            b"Physical RAM: 497 MiB usable\nKernel Heap: 16384 KiB\nBackbuffer: 16384 KiB",
        );
        let _ = self.mkdir("/sys/drivers");
        let _ = self.create_file(
            "/sys/drivers/keyboard.sys",
            b"DRIVER=Standard PS/2 Keyboard\nVENDOR=Standard Keyboards\nLOCATION=Plugged into keyboard port\nPORTS=0x60, 0x64\nSTATUS=INSTALLED_ACTIVE\n",
        );
        let _ = self.create_file(
            "/sys/drivers/touchpad.sys",
            b"DRIVER=ELAN Input Device\nVENDOR=ELAN Microelectronics\nLOCATION=on I2C HID Device\nBUS=AMD DesignWare I2C (AMDI0010)\nSTATUS=INCLUDED_ACPI_PROBED\nNOTE=Physical I2C HID driver pending AML evaluation; Pointer navigation active via Super+Arrows\n",
        );
        let _ = self.create_file(
            "/sys/drivers/wifi.sys",
            b"DRIVER=Realtek 8852BE-VT Wireless LAN WiFi 6 PCI-E NIC\nVENDOR=Realtek Semiconductor\nBUS=PCI Express\nFIRMWARE=rtw8852b_fw.bin (Embedded)\nSTATUS=INSTALLED_INCLUDED\n",
        );
        let _ = self.create_file(
            "/sys/drivers/bluetooth.sys",
            b"DRIVER=Realtek Wireless Bluetooth Adapter\nVENDOR=Realtek Semiconductor\nDEVICE_ID=VID 0x0BDA\nINTERFACE=Bluetooth 5.2 HCI\nSTATUS=INSTALLED_INCLUDED\n",
        );
        let _ = self.create_file(
            "/etc/os-release",
            b"NAME=\"Micro-OS\"\nID=\"micro_os\"\nPRETTY_NAME=\"Micro-OS Obsidian Azure\"\nVERSION=\"0.11.17\"",
        );
    }

    /// Normalizes a path string into a canonical list of directory/file segments.
    /// Supports `/`, `//`, `.`, and `..` relative navigation.
    pub fn parse_segments<'a>(path: &'a str) -> Vec<&'a str> {
        let mut segments = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    segments.pop();
                }
                seg => {
                    segments.push(seg);
                }
            }
        }
        segments
    }

    /// Normalizes a path string into standard canonical format (e.g. `/home/user`).
    pub fn canonicalize_path(path: &str) -> String {
        let segments = Self::parse_segments(path);
        if segments.is_empty() {
            "/".to_string()
        } else {
            let mut out = String::new();
            for seg in segments {
                out.push('/');
                out.push_str(seg);
            }
            out
        }
    }

    /// Resolves an immutable reference to the node at the specified path.
    pub fn resolve_node(&self, path: &str) -> Result<&FsNode, FsError> {
        let segments = Self::parse_segments(path);
        let mut current = &self.root;

        for seg in segments {
            if current.kind != FsNodeKind::Directory {
                return Err(FsError::NotADirectory);
            }
            match current.find_child(seg) {
                Some(child) => current = child,
                None => return Err(FsError::NotFound),
            }
        }

        Ok(current)
    }

    /// Resolves a mutable reference to the node at the specified path.
    #[allow(dead_code)]
    pub fn resolve_node_mut(&mut self, path: &str) -> Result<&mut FsNode, FsError> {
        let segments = Self::parse_segments(path);
        let mut current = &mut self.root;

        for seg in segments {
            if current.kind != FsNodeKind::Directory {
                return Err(FsError::NotADirectory);
            }
            match current.find_child_mut(seg) {
                Some(child) => current = child,
                None => return Err(FsError::NotFound),
            }
        }

        Ok(current)
    }

    /// Lists the contents of the directory at `path`.
    ///
    /// Entries are sorted with directories first (alphabetically), followed by files (alphabetically).
    pub fn list_dir(&self, path: &str) -> Result<Vec<FsEntry>, FsError> {
        let node = self.resolve_node(path)?;
        if node.kind != FsNodeKind::Directory {
            return Err(FsError::NotADirectory);
        }

        let mut entries: Vec<FsEntry> = node
            .children
            .iter()
            .map(|c| FsEntry {
                name: c.name.clone(),
                kind: c.kind,
                size: if c.kind == FsNodeKind::File {
                    c.content.len()
                } else {
                    c.children.len()
                },
            })
            .collect();

        // Sort: directories first, then alphabetical by name
        entries.sort_by(|a, b| match (a.kind, b.kind) {
            (FsNodeKind::Directory, FsNodeKind::File) => core::cmp::Ordering::Less,
            (FsNodeKind::File, FsNodeKind::Directory) => core::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        Ok(entries)
    }

    /// Creates a new directory at `path`.
    pub fn mkdir(&mut self, path: &str) -> Result<(), FsError> {
        let segments = Self::parse_segments(path);
        if segments.is_empty() {
            return Err(FsError::AlreadyExists); // Root already exists
        }

        let dir_name = segments[segments.len() - 1];
        let parent_segments = &segments[..segments.len() - 1];

        let mut current = &mut self.root;
        for seg in parent_segments {
            if current.kind != FsNodeKind::Directory {
                return Err(FsError::NotADirectory);
            }
            match current.find_child_mut(seg) {
                Some(child) => current = child,
                None => return Err(FsError::NotFound),
            }
        }

        if current.kind != FsNodeKind::Directory {
            return Err(FsError::NotADirectory);
        }

        if current.find_child(dir_name).is_some() {
            return Err(FsError::AlreadyExists);
        }

        current.children.push(FsNode::new_directory(dir_name));
        Ok(())
    }

    /// Creates or overwrites a file at `path` with `data`.
    pub fn create_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsError> {
        let segments = Self::parse_segments(path);
        if segments.is_empty() {
            return Err(FsError::InvalidPath);
        }

        let file_name = segments[segments.len() - 1];
        let parent_segments = &segments[..segments.len() - 1];

        let mut current = &mut self.root;
        for seg in parent_segments {
            if current.kind != FsNodeKind::Directory {
                return Err(FsError::NotADirectory);
            }
            match current.find_child_mut(seg) {
                Some(child) => current = child,
                None => return Err(FsError::NotFound),
            }
        }

        if current.kind != FsNodeKind::Directory {
            return Err(FsError::NotADirectory);
        }

        if let Some(existing) = current.find_child_mut(file_name) {
            if existing.kind == FsNodeKind::Directory {
                return Err(FsError::IsADirectory);
            }
            existing.content = data.to_vec();
        } else {
            current.children.push(FsNode::new_file(file_name, data));
        }

        Ok(())
    }

    /// Reads the entire byte content of a file at `path`.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let node = self.resolve_node(path)?;
        if node.kind != FsNodeKind::File {
            return Err(FsError::IsADirectory);
        }
        Ok(node.content.clone())
    }

    /// Removes a file or directory at `path`.
    ///
    /// Refuses to delete the root directory `/`.
    pub fn remove(&mut self, path: &str) -> Result<(), FsError> {
        let segments = Self::parse_segments(path);
        if segments.is_empty() {
            return Err(FsError::CannotDeleteRoot);
        }

        let target_name = segments[segments.len() - 1];
        let parent_segments = &segments[..segments.len() - 1];

        let mut current = &mut self.root;
        for seg in parent_segments {
            if current.kind != FsNodeKind::Directory {
                return Err(FsError::NotADirectory);
            }
            match current.find_child_mut(seg) {
                Some(child) => current = child,
                None => return Err(FsError::NotFound),
            }
        }

        if current.kind != FsNodeKind::Directory {
            return Err(FsError::NotADirectory);
        }

        let pos = current
            .children
            .iter()
            .position(|c| c.name == target_name)
            .ok_or(FsError::NotFound)?;

        current.children.remove(pos);
        Ok(())
    }

    /// Checks if a node exists at `path`.
    pub fn exists(&self, path: &str) -> bool {
        self.resolve_node(path).is_ok()
    }

    /// Checks if the path refers to an existing directory.
    pub fn is_dir(&self, path: &str) -> bool {
        match self.resolve_node(path) {
            Ok(node) => node.kind == FsNodeKind::Directory,
            Err(_) => false,
        }
    }
}
