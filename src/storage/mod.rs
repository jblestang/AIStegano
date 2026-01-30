//! Storage layer for slack space operations.
//!
//! This module handles:
//! - Reading/writing data to slack space
//! - Managing host files
//! - Persisting minimal bootstrap metadata
//!
//! ## Block Device Slack Access
//!
//! For true steganographic storage, this module provides raw block device
//! access to file slack space (the unused bytes within allocated blocks).
//! This requires elevated privileges (sudo) and is platform-specific.

mod host_manager;
pub(crate) mod metadata;
mod slack;
pub mod slack_backend;

// Platform-specific implementations
#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

pub use host_manager::{HostFile, HostManager, SymbolLocation};
pub use metadata::SlackMetadata;
pub use slack::{get_slack_capacity, read_slack, wipe_slack, write_slack};
pub use slack_backend::{create_backend, SlackBackend, SlackRegion};

