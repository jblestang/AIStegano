//! Virtual File System for slack space storage.
//!
//! Provides a file system abstraction over encrypted, erasure-coded data
//! stored in the slack space of host files.

mod operations;
mod path;
pub(crate) mod superblock;
mod types;

pub use operations::{HealthReport, SlackVfs};
pub use path::VfsPath;
pub use superblock::{HostAllocation, Superblock, SymbolAllocation};
pub use types::{DirEntry, Inode, InodeId, InodeType};
