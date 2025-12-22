//! Shared types used across the library.
//!
//! This module contains types that are available with just the `alloc` feature,
//! making them usable in both std and no_std environments.

use alloc::string::String;
use alloc::vec::Vec;

/// An enum representing the decoded value of a channel sample.
///
/// This type represents all possible values that can be stored in an MDF channel.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodedValue {
    /// Unsigned integer (up to 64 bits)
    UnsignedInteger(u64),
    /// Signed integer (up to 64 bits)
    SignedInteger(i64),
    /// Floating point value (32 or 64 bit)
    Float(f64),
    /// Text string (UTF-8 or converted from Latin-1)
    String(String),
    /// Raw byte array
    ByteArray(Vec<u8>),
    /// MIME sample data
    MimeSample(Vec<u8>),
    /// MIME stream data
    MimeStream(Vec<u8>),
    /// Unknown or unsupported data type
    Unknown,
}

impl DecodedValue {
    /// Returns true if this is an integer value (signed or unsigned).
    #[inline]
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            DecodedValue::UnsignedInteger(_) | DecodedValue::SignedInteger(_)
        )
    }

    /// Returns true if this is a floating point value.
    #[inline]
    pub fn is_float(&self) -> bool {
        matches!(self, DecodedValue::Float(_))
    }

    /// Returns true if this is a string value.
    #[inline]
    pub fn is_string(&self) -> bool {
        matches!(self, DecodedValue::String(_))
    }

    /// Returns true if this is a byte array value.
    #[inline]
    pub fn is_bytes(&self) -> bool {
        matches!(
            self,
            DecodedValue::ByteArray(_) | DecodedValue::MimeSample(_) | DecodedValue::MimeStream(_)
        )
    }

    /// Attempts to convert to f64, useful for numeric operations.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            DecodedValue::UnsignedInteger(v) => Some(*v as f64),
            DecodedValue::SignedInteger(v) => Some(*v as f64),
            DecodedValue::Float(v) => Some(*v),
            _ => None,
        }
    }
}

impl core::fmt::Display for DecodedValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodedValue::UnsignedInteger(v) => write!(f, "{}", v),
            DecodedValue::SignedInteger(v) => write!(f, "{}", v),
            DecodedValue::Float(v) => write!(f, "{}", v),
            DecodedValue::String(s) => write!(f, "{}", s),
            DecodedValue::ByteArray(b) => write!(f, "[{} bytes]", b.len()),
            DecodedValue::MimeSample(b) => write!(f, "[MIME sample: {} bytes]", b.len()),
            DecodedValue::MimeStream(b) => write!(f, "[MIME stream: {} bytes]", b.len()),
            DecodedValue::Unknown => write!(f, "<unknown>"),
        }
    }
}
