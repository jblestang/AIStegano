//! VFS types: inodes, directory entries, etc.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Unique identifier for an inode.
pub type InodeId = u64;

/// Root inode ID (always 0).
pub const ROOT_INODE_ID: InodeId = 0;

/// An inode representing a file or directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inode {
    /// Unique identifier.
    pub id: InodeId,
    /// Name of the file or directory.
    pub name: String,
    /// Type of inode (file or directory).
    pub inode_type: InodeType,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Creation timestamp (Unix epoch seconds).
    pub created: u64,
    /// Last modification timestamp (Unix epoch seconds).
    pub modified: u64,
    /// RaptorQ symbol IDs for this file's data.
    pub symbol_ids: Vec<u32>,
    /// Encoding metadata needed for decoding.
    pub encoding_info: Option<EncodingInfo>,
}

/// Encoding information stored with each file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingInfo {
    /// Original data length.
    pub original_length: u64,
    /// Number of source symbols.
    pub source_symbols: usize,
    /// Number of repair symbols.
    pub repair_symbols: usize,
    /// Symbol size in bytes.
    pub symbol_size: u16,
}

impl Inode {
    /// Create a new file inode.
    pub fn new_file(id: InodeId, name: String, size: u64) -> Self {
        let now = current_timestamp();
        Self {
            id,
            name,
            inode_type: InodeType::File,
            size,
            created: now,
            modified: now,
            symbol_ids: Vec::new(),
            encoding_info: None,
        }
    }

    /// Create a new directory inode.
    pub fn new_directory(id: InodeId, name: String) -> Self {
        let now = current_timestamp();
        Self {
            id,
            name,
            inode_type: InodeType::Directory {
                children: Vec::new(),
            },
            size: 0,
            created: now,
            modified: now,
            symbol_ids: Vec::new(),
            encoding_info: None,
        }
    }

    /// Create the root directory inode.
    pub fn root() -> Self {
        Self::new_directory(ROOT_INODE_ID, "/".to_string())
    }

    /// Check if this is a file.
    pub fn is_file(&self) -> bool {
        matches!(self.inode_type, InodeType::File)
    }

    /// Check if this is a directory.
    pub fn is_directory(&self) -> bool {
        matches!(self.inode_type, InodeType::Directory { .. })
    }

    /// Get children if this is a directory.
    pub fn children(&self) -> Option<&Vec<InodeId>> {
        match &self.inode_type {
            InodeType::Directory { children } => Some(children),
            InodeType::File => None,
        }
    }

    /// Get mutable children if this is a directory.
    pub fn children_mut(&mut self) -> Option<&mut Vec<InodeId>> {
        match &mut self.inode_type {
            InodeType::Directory { children } => Some(children),
            InodeType::File => None,
        }
    }

    /// Add a child to this directory.
    pub fn add_child(&mut self, child_id: InodeId) -> bool {
        if let Some(children) = self.children_mut() {
            if !children.contains(&child_id) {
                children.push(child_id);
                self.modified = current_timestamp();
                return true;
            }
        }
        false
    }

    /// Remove a child from this directory.
    pub fn remove_child(&mut self, child_id: InodeId) -> bool {
        if let Some(children) = self.children_mut() {
            if let Some(pos) = children.iter().position(|&id| id == child_id) {
                children.remove(pos);
                self.modified = current_timestamp();
                return true;
            }
        }
        false
    }

    /// Update modification time.
    pub fn touch(&mut self) {
        self.modified = current_timestamp();
    }
}

/// Type of inode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InodeType {
    /// A regular file.
    File,
    /// A directory with child inode IDs.
    Directory { children: Vec<InodeId> },
}

/// A directory entry for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// Name of the entry.
    pub name: String,
    /// Inode ID.
    pub inode_id: InodeId,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Size in bytes (for files).
    pub size: u64,
}

impl DirEntry {
    /// Create from an inode.
    pub fn from_inode(inode: &Inode) -> Self {
        Self {
            name: inode.name.clone(),
            inode_id: inode.id,
            is_dir: inode.is_directory(),
            size: inode.size,
        }
    }
}

/// Get current Unix timestamp.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_file() {
        let file = Inode::new_file(1, "test.txt".to_string(), 1024);

        assert_eq!(file.id, 1);
        assert_eq!(file.name, "test.txt");
        assert!(file.is_file());
        assert!(!file.is_directory());
        assert_eq!(file.size, 1024);
    }

    #[test]
    fn test_new_directory() {
        let dir = Inode::new_directory(2, "docs".to_string());

        assert_eq!(dir.id, 2);
        assert_eq!(dir.name, "docs");
        assert!(dir.is_directory());
        assert!(!dir.is_file());
        assert_eq!(dir.children().unwrap().len(), 0);
    }

    #[test]
    fn test_add_child() {
        let mut dir = Inode::new_directory(1, "parent".to_string());

        assert!(dir.add_child(2));
        assert!(dir.add_child(3));
        assert!(!dir.add_child(2)); // Duplicate

        assert_eq!(dir.children().unwrap().len(), 2);
    }

    #[test]
    fn test_remove_child() {
        let mut dir = Inode::new_directory(1, "parent".to_string());
        dir.add_child(2);
        dir.add_child(3);

        assert!(dir.remove_child(2));
        assert!(!dir.remove_child(2)); // Already removed

        assert_eq!(dir.children().unwrap().len(), 1);
    }
}
