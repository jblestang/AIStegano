//! Slack space backend abstraction for cross-platform support.
//!
//! This module defines the trait for accessing true file system slack space
//! via raw block device access.

use crate::error::Result;
use std::path::{Path, PathBuf};

/// Information about a file's slack space region.
#[derive(Debug, Clone)]
pub struct SlackRegion {
    /// Path to the raw block device (e.g., /dev/sda1 or /dev/rdisk2).
    pub device_path: PathBuf,
    /// Absolute byte offset on the block device where slack starts.
    pub offset: u64,
    /// Number of available slack bytes.
    pub available: u64,
    /// The file's logical size (for reference).
    pub logical_size: u64,
    /// Block size of the file system.
    pub block_size: u64,
}

/// Trait for platform-specific slack space access.
///
/// Implementations must provide raw block device access to read/write
/// the unused bytes in a file's final allocated block.
pub trait SlackBackend: Send + Sync {
    /// Get slack space information for a file.
    ///
    /// Returns the block device and offset where slack space begins,
    /// along with the available capacity.
    fn get_slack_info(&self, path: &Path) -> Result<SlackRegion>;

    /// Read bytes from a slack region.
    ///
    /// # Arguments
    /// * `region` - The slack region obtained from `get_slack_info`
    /// * `offset` - Offset within the slack region (not absolute)
    /// * `len` - Number of bytes to read
    fn read_slack(&self, region: &SlackRegion, offset: u64, len: usize) -> Result<Vec<u8>>;

    /// Write bytes to a slack region.
    ///
    /// # Arguments
    /// * `region` - The slack region obtained from `get_slack_info`
    /// * `offset` - Offset within the slack region (not absolute)
    /// * `data` - Data to write
    ///
    /// # Safety
    /// This writes directly to the block device. Incorrect offsets can
    /// corrupt the file system.
    fn write_slack(&self, region: &SlackRegion, offset: u64, data: &[u8]) -> Result<()>;

    /// Wipe slack space by overwriting with zeros or random data.
    fn wipe_slack(&self, region: &SlackRegion) -> Result<()>;

    /// Check if this backend is available on the current system.
    fn is_available(&self) -> bool;

    /// Get the name of this backend (for logging).
    fn name(&self) -> &'static str;
}

/// Create the appropriate slack backend for the current platform.
#[cfg(target_os = "linux")]
pub fn create_backend() -> Result<Box<dyn SlackBackend>> {
    use super::linux::LinuxSlackBackend;
    Ok(Box::new(LinuxSlackBackend::new()?))
}

#[cfg(target_os = "macos")]
pub fn create_backend() -> Result<Box<dyn SlackBackend>> {
    use super::macos::MacSlackBackend;
    Ok(Box::new(MacSlackBackend::new()?))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn create_backend() -> Result<Box<dyn SlackBackend>> {
    Err(crate::error::Error::Unsupported(
        "Block device slack access not supported on this platform".to_string(),
    ))
}
