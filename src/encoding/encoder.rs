//! RaptorQ encoder for creating erasure-coded symbols.

use crate::config::EncodingConfig;
use crate::error::Result;
use raptorq::Encoder;
use serde::{Deserialize, Serialize};

/// A single encoded symbol with its identifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingSymbol {
    /// Unique identifier for this symbol (used for decoding).
    pub id: u32,
    /// The encoded data.
    pub data: Vec<u8>,
}

/// Result of encoding data with RaptorQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodedData {
    /// Original data length in bytes.
    pub original_length: u64,
    /// Number of source symbols.
    pub source_symbols: usize,
    /// Number of repair symbols.
    pub repair_symbols: usize,
    /// Symbol size in bytes.
    pub symbol_size: u16,
    /// All encoded symbols (source + repair).
    pub symbols: Vec<EncodingSymbol>,
}

impl EncodedData {
    /// Get the total number of symbols.
    pub fn total_symbols(&self) -> usize {
        self.symbols.len()
    }

    /// Get the minimum symbols needed to decode.
    pub fn min_symbols_needed(&self) -> usize {
        self.source_symbols
    }

    /// Check if we have enough symbols to decode.
    pub fn can_decode(&self) -> bool {
        self.symbols.len() >= self.source_symbols
    }
}

/// Encode data into RaptorQ symbols with configurable redundancy.
///
/// # Arguments
///
/// * `data` - The data to encode
/// * `config` - Encoding configuration (symbol size, redundancy ratio)
///
/// # Returns
///
/// Encoded data containing source and repair symbols.
///
/// # Example
///
/// ```
/// use slack_vfs::encoding::encode;
/// use slack_vfs::config::EncodingConfig;
///
/// let data = b"Hello, World!";
/// let config = EncodingConfig::default();
/// let encoded = encode(data, &config).unwrap();
///
/// // With 50% redundancy, we get extra repair symbols
/// assert!(encoded.repair_symbols > 0);
/// ```
pub fn encode(data: &[u8], config: &EncodingConfig) -> Result<EncodedData> {
    if data.is_empty() {
        return Ok(EncodedData {
            original_length: 0,
            source_symbols: 0,
            repair_symbols: 0,
            symbol_size: config.symbol_size,
            symbols: Vec::new(),
        });
    }

    let symbol_size = config.symbol_size as usize;

    // Create encoder with the data
    let encoder = Encoder::with_defaults(data, symbol_size as u16);

    // Get transmission info for later decoding
    let _oti = encoder.get_config();

    // Calculate number of source and repair symbols
    let source_symbols = (data.len() + symbol_size - 1) / symbol_size;
    let repair_symbols = ((source_symbols as f32) * config.redundancy_ratio).ceil() as usize;
    let total_symbols = source_symbols + repair_symbols;

    // Generate all symbols
    let mut symbols = Vec::with_capacity(total_symbols);
    let mut symbol_id = 0u32;

    // Get source symbols from each source block
    for block in encoder.get_block_encoders() {
        // Get source symbols
        for packet in block.source_packets() {
            symbols.push(EncodingSymbol {
                id: symbol_id,
                data: packet.data().to_vec(),
            });
            symbol_id += 1;
        }

        // Get repair symbols
        let repair_per_block = (repair_symbols + encoder.get_block_encoders().len() - 1)
            / encoder.get_block_encoders().len();
        for packet in block.repair_packets(0, repair_per_block as u32) {
            symbols.push(EncodingSymbol {
                id: symbol_id,
                data: packet.data().to_vec(),
            });
            symbol_id += 1;
        }
    }

    Ok(EncodedData {
        original_length: data.len() as u64,
        source_symbols,
        repair_symbols: symbols.len().saturating_sub(source_symbols),
        symbol_size: config.symbol_size,
        symbols,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_small_data() {
        let data = b"Hello, World!";
        let config = EncodingConfig::default();

        let encoded = encode(data, &config).unwrap();

        assert_eq!(encoded.original_length, data.len() as u64);
        assert!(encoded.source_symbols > 0);
        assert!(encoded.repair_symbols > 0);
    }

    #[test]
    fn test_encode_empty_data() {
        let data = b"";
        let config = EncodingConfig::default();

        let encoded = encode(data, &config).unwrap();

        assert_eq!(encoded.original_length, 0);
        assert_eq!(encoded.source_symbols, 0);
        assert_eq!(encoded.symbols.len(), 0);
    }

    #[test]
    fn test_encode_large_data() {
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let config = EncodingConfig::default();

        let encoded = encode(&data, &config).unwrap();

        assert_eq!(encoded.original_length, data.len() as u64);
        assert!(encoded.source_symbols > 0);
        assert!(encoded.total_symbols() > encoded.source_symbols);
    }

    #[test]
    fn test_symbol_ids_unique() {
        let data = b"Test data for encoding";
        let config = EncodingConfig::default();

        let encoded = encode(data, &config).unwrap();

        let mut ids: Vec<u32> = encoded.symbols.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();

        assert_eq!(ids.len(), encoded.symbols.len());
    }
}
