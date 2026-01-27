//! Configuration constants and types for Slack VFS.

use serde::{Deserialize, Serialize};

/// Default block size (4KB, common for most file systems).
pub const DEFAULT_BLOCK_SIZE: u64 = 4096;

/// Default symbol size for RaptorQ encoding.
pub const DEFAULT_SYMBOL_SIZE: u16 = 1024;

/// Default redundancy ratio (50% extra symbols).
pub const DEFAULT_REDUNDANCY_RATIO: f32 = 0.5;

/// Minimum redundancy ratio.
pub const MIN_REDUNDANCY_RATIO: f32 = 0.1;

/// Maximum redundancy ratio.
pub const MAX_REDUNDANCY_RATIO: f32 = 2.0;

/// VFS magic number: "SVFS" in bytes.
pub const VFS_MAGIC: [u8; 4] = [0x53, 0x56, 0x46, 0x53];

/// Current VFS version.
pub const VFS_VERSION: u32 = 1;

/// Argon2id parameters for key derivation.
pub mod argon2_params {
    /// Memory cost in KiB (64 MB).
    pub const MEMORY_COST: u32 = 65536;

    /// Time cost (iterations).
    pub const TIME_COST: u32 = 3;

    /// Parallelism factor.
    pub const PARALLELISM: u32 = 4;

    /// Output length in bytes (256 bits).
    pub const OUTPUT_LENGTH: usize = 32;

    /// Salt length in bytes.
    pub const SALT_LENGTH: usize = 32;
}

/// Secure wipe parameters.
pub mod wipe_params {
    /// Number of random overwrite passes.
    pub const RANDOM_PASSES: u8 = 3;

    /// Number of zero overwrite passes.
    pub const ZERO_PASSES: u8 = 1;
}

/// Configuration for VFS initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsConfig {
    /// Block size for slack space calculation.
    pub block_size: u64,

    /// Symbol size for RaptorQ encoding.
    pub symbol_size: u16,

    /// Redundancy ratio (0.0 to 2.0).
    /// 0.5 means 50% extra repair symbols.
    pub redundancy_ratio: f32,
}

impl Default for VfsConfig {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
            symbol_size: DEFAULT_SYMBOL_SIZE,
            redundancy_ratio: DEFAULT_REDUNDANCY_RATIO,
        }
    }
}

impl VfsConfig {
    /// Create a new VFS configuration with custom settings.
    pub fn new(block_size: u64, symbol_size: u16, redundancy_ratio: f32) -> Self {
        Self {
            block_size,
            symbol_size,
            redundancy_ratio: redundancy_ratio.clamp(MIN_REDUNDANCY_RATIO, MAX_REDUNDANCY_RATIO),
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.block_size == 0 || !self.block_size.is_power_of_two() {
            return Err("Block size must be a power of 2".to_string());
        }
        if self.symbol_size == 0 {
            return Err("Symbol size must be greater than 0".to_string());
        }
        if self.redundancy_ratio < MIN_REDUNDANCY_RATIO
            || self.redundancy_ratio > MAX_REDUNDANCY_RATIO
        {
            return Err(format!(
                "Redundancy ratio must be between {} and {}",
                MIN_REDUNDANCY_RATIO, MAX_REDUNDANCY_RATIO
            ));
        }
        Ok(())
    }
}

/// Encoding configuration derived from VfsConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingConfig {
    /// Size of each symbol in bytes.
    pub symbol_size: u16,

    /// Ratio of repair symbols to source symbols.
    pub redundancy_ratio: f32,
}

impl From<&VfsConfig> for EncodingConfig {
    fn from(config: &VfsConfig) -> Self {
        Self {
            symbol_size: config.symbol_size,
            redundancy_ratio: config.redundancy_ratio,
        }
    }
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            symbol_size: DEFAULT_SYMBOL_SIZE,
            redundancy_ratio: DEFAULT_REDUNDANCY_RATIO,
        }
    }
}
