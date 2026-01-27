//! RaptorQ decoder for recovering data from symbols.

use crate::encoding::encoder::EncodedData;
use crate::error::{Error, Result};
use raptorq::{Decoder, EncodingPacket, ObjectTransmissionInformation, PayloadId};

/// Progress information for decoding.
#[derive(Debug, Clone)]
pub struct DecodingProgress {
    /// Number of symbols received.
    pub received: usize,
    /// Minimum number of symbols needed.
    pub required: usize,
    /// Whether decoding is possible with current symbols.
    pub can_decode: bool,
    /// Percentage of required symbols received.
    pub progress_percent: f32,
}

impl DecodingProgress {
    /// Create new progress tracker.
    pub fn new(received: usize, required: usize) -> Self {
        Self {
            received,
            required,
            can_decode: received >= required,
            progress_percent: (received as f32 / required as f32 * 100.0).min(100.0),
        }
    }
}

/// Check if we have enough symbols to decode.
pub fn can_decode(received: usize, required: usize) -> bool {
    received >= required
}

/// Decode data from collected symbols.
///
/// RaptorQ can recover the original data from any K symbols where K is
/// approximately equal to the number of source symbols. With the overhead
/// of RaptorQ, you need slightly more than K symbols for guaranteed recovery.
///
/// # Arguments
///
/// * `encoded` - The encoded data structure with available symbols
///
/// # Returns
///
/// The original data if enough symbols are available, or an error.
///
/// # Example
///
/// ```
/// use slack_vfs::encoding::{encode, decode};
/// use slack_vfs::config::EncodingConfig;
///
/// let data = b"Hello, World!";
/// let config = EncodingConfig::default();
///
/// let encoded = encode(data, &config).unwrap();
/// let decoded = decode(&encoded).unwrap();
///
/// assert_eq!(decoded, data);
/// ```
pub fn decode(encoded: &EncodedData) -> Result<Vec<u8>> {
    if encoded.original_length == 0 {
        return Ok(Vec::new());
    }

    if encoded.symbols.is_empty() {
        return Err(Error::InsufficientSymbols {
            required: encoded.source_symbols,
            received: 0,
        });
    }

    // Create decoder configuration
    // We need to reconstruct the OTI from encoded data
    let symbol_size = encoded.symbol_size as u64;
    let data_length = encoded.original_length;

    let config = ObjectTransmissionInformation::with_defaults(data_length, symbol_size as u16);

    let mut decoder = Decoder::new(config);

    // Add each symbol to the decoder
    for symbol in &encoded.symbols {
        // Create encoding packet from symbol data
        // The symbol ID needs to be converted to a proper PayloadId
        let packet = EncodingPacket::new(PayloadId::new(0, symbol.id), symbol.data.clone());

        if let Some(result) = decoder.decode(packet) {
            // Successfully decoded
            return Ok(result);
        }
    }

    // Not enough symbols
    Err(Error::InsufficientSymbols {
        required: encoded.source_symbols,
        received: encoded.symbols.len(),
    })
}

/// Decode from a subset of symbols (simulating data loss).
///
/// This is useful for testing resilience.
pub fn decode_partial(encoded: &EncodedData, available_symbol_ids: &[u32]) -> Result<Vec<u8>> {
    if encoded.original_length == 0 {
        return Ok(Vec::new());
    }

    // Filter symbols to only those available
    let available_symbols: Vec<_> = encoded
        .symbols
        .iter()
        .filter(|s| available_symbol_ids.contains(&s.id))
        .cloned()
        .collect();

    if available_symbols.is_empty() {
        return Err(Error::InsufficientSymbols {
            required: encoded.source_symbols,
            received: 0,
        });
    }

    // Create a modified encoded data with only available symbols
    let partial_encoded = EncodedData {
        original_length: encoded.original_length,
        source_symbols: encoded.source_symbols,
        repair_symbols: encoded.repair_symbols,
        symbol_size: encoded.symbol_size,
        symbols: available_symbols,
    };

    decode(&partial_encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EncodingConfig;
    use crate::encoding::encode;

    #[test]
    fn test_decode_all_symbols() {
        let data = b"Hello, World! This is test data for RaptorQ encoding.";
        let config = EncodingConfig::default();

        let encoded = encode(data, &config).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_empty() {
        let data = b"";
        let config = EncodingConfig::default();

        let encoded = encode(data, &config).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_with_partial_loss() {
        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let config = EncodingConfig {
            symbol_size: 512,
            redundancy_ratio: 0.5, // 50% extra symbols
        };

        let encoded = encode(&data, &config).unwrap();

        // Simulate losing 20% of symbols
        let keep_count = (encoded.symbols.len() as f32 * 0.8) as usize;
        let available_ids: Vec<u32> = encoded.symbols[..keep_count].iter().map(|s| s.id).collect();

        let decoded = decode_partial(&encoded, &available_ids).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_with_30_percent_loss() {
        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let config = EncodingConfig {
            symbol_size: 512,
            redundancy_ratio: 0.5,
        };

        let encoded = encode(&data, &config).unwrap();

        // Simulate losing 30% of symbols - should still work with 50% redundancy
        let keep_count = (encoded.symbols.len() as f32 * 0.7) as usize;
        let available_ids: Vec<u32> = encoded.symbols[..keep_count].iter().map(|s| s.id).collect();

        let decoded = decode_partial(&encoded, &available_ids).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_progress_tracking() {
        let progress = DecodingProgress::new(8, 10);

        assert!(!progress.can_decode);
        assert_eq!(progress.progress_percent, 80.0);

        let progress2 = DecodingProgress::new(12, 10);
        assert!(progress2.can_decode);
        assert_eq!(progress2.progress_percent, 100.0);
    }
}
