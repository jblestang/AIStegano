//! Raw disk device access for macOS.
//!
//! Provides low-level read/write to /dev/rdiskN devices.

use crate::error::{Error, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Block size for aligned I/O.
const DISK_BLOCK_SIZE: usize = 4096;

/// Handle for raw disk device access.
pub struct RawDisk {
    file: File,
    writable: bool,
}

impl RawDisk {
    /// Open a raw disk device for reading.
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    Error::PermissionDenied(format!(
                        "Cannot open raw disk {}. Try running with sudo.",
                        path.display()
                    ))
                } else {
                    Error::Io(e)
                }
            })?;

        Ok(Self {
            file,
            writable: false,
        })
    }

    /// Open a raw disk device for reading and writing.
    pub fn open_write(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    Error::PermissionDenied(format!(
                        "Cannot open raw disk {} for writing. Try running with sudo.",
                        path.display()
                    ))
                } else {
                    Error::Io(e)
                }
            })?;

        Ok(Self {
            file,
            writable: true,
        })
    }

    /// Read bytes at a specific offset.
    pub fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        // macOS raw disks may require block-aligned access
        let align = DISK_BLOCK_SIZE as u64;
        let aligned_start = (offset / align) * align;
        let aligned_end = ((offset + len as u64 + align - 1) / align) * align;
        let aligned_len = (aligned_end - aligned_start) as usize;

        let mut aligned_buf = vec![0u8; aligned_len];

        let mut file = &self.file;
        file.seek(SeekFrom::Start(aligned_start))
            .map_err(|e| Error::Io(e))?;
        file.read_exact(&mut aligned_buf)
            .map_err(|e| Error::Io(e))?;

        // Extract requested portion
        let start_offset = (offset - aligned_start) as usize;
        Ok(aligned_buf[start_offset..start_offset + len].to_vec())
    }

    /// Write bytes at a specific offset.
    pub fn write_at(&self, offset: u64, data: &[u8]) -> Result<()> {
        if !self.writable {
            return Err(Error::PermissionDenied("Disk not opened for writing".to_string()));
        }

        // Read-modify-write for unaligned access
        let align = DISK_BLOCK_SIZE as u64;
        let aligned_start = (offset / align) * align;
        let aligned_end = ((offset + data.len() as u64 + align - 1) / align) * align;
        let aligned_len = (aligned_end - aligned_start) as usize;

        // Read existing block(s)
        let mut aligned_buf = self.read_at(aligned_start, aligned_len)?;
        aligned_buf.resize(aligned_len, 0);

        // Patch in new data
        let start_offset = (offset - aligned_start) as usize;
        aligned_buf[start_offset..start_offset + data.len()].copy_from_slice(data);

        // Write back
        let mut file = &self.file;
        file.seek(SeekFrom::Start(aligned_start))
            .map_err(|e| Error::Io(e))?;
        file.write_all(&aligned_buf)
            .map_err(|e| Error::Io(e))?;

        Ok(())
    }
}
