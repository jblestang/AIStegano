//! RaptorQ erasure coding for data resilience.
//!
//! This module provides encoding and decoding using RaptorQ (RFC 6330),
//! allowing data recovery even when parts are lost or corrupted.

mod decoder;
mod encoder;

pub use decoder::{can_decode, decode, DecodingProgress};
pub use encoder::{encode, EncodedData, EncodingSymbol};
