//! Virtual File System for slack space storage.
//!
//! Provides a file system abstraction over encrypted, erasure-coded data
//! stored in the slack space of host files.

mod operations;
mod path;
mod superblock;
mod types;

pub use operations::{HealthReport, SlackVfs};
pub use path::VfsPath;
pub use superblock::Superblock;
pub use types::{DirEntry, Inode, InodeId, InodeType};
