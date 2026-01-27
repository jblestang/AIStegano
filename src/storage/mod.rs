//! Storage layer for slack space operations.
//!
//! This module handles:
//! - Reading/writing data to slack space
//! - Managing host files
//! - Persisting minimal bootstrap metadata

mod host_manager;
pub(crate) mod metadata;
mod slack;

pub use host_manager::{HostFile, HostManager, SymbolLocation};
pub use metadata::{SlackMetadata, SuperblockLocation};
pub use slack::{get_slack_capacity, read_slack, wipe_slack, write_slack};
