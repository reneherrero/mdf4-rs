// blocks/common.rs
//! Common types, traits, and helper functions for MDF block parsing.
//!
//! This module provides:
//! - [`BlockHeader`]: The 24-byte header present in all MDF blocks
//! - [`BlockParse`]: Trait for parsing blocks from bytes
//! - [`DataType`]: Enum representing MDF data types
//! - Byte parsing helper functions to reduce code duplication

use crate::{
    Error, Result,
    blocks::{metadata_block::MetadataBlock, text_block::TextBlock},
};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

// ============================================================================
// Byte Parsing Helpers
// ============================================================================

/// Read a u64 from a byte slice at the given offset (little-endian).
///
/// # Panics
/// Panics if `offset + 8 > bytes.len()`. Use `read_u64_checked` for fallible version.
#[inline]
pub fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// Read a u32 from a byte slice at the given offset (little-endian).
#[inline]
pub fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Read a u16 from a byte slice at the given offset (little-endian).
#[inline]
pub fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// Read an f64 from a byte slice at the given offset (little-endian).
#[inline]
pub fn read_f64(bytes: &[u8], offset: usize) -> f64 {
    f64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// Read a u8 from a byte slice at the given offset.
#[inline]
pub fn read_u8(bytes: &[u8], offset: usize) -> u8 {
    bytes[offset]
}

// ============================================================================
// Validation Helpers
// ============================================================================

/// Validate that a buffer has at least `expected` bytes.
///
/// Returns `Err(TooShortBuffer)` if the buffer is too small.
#[inline]
pub fn validate_buffer_size(bytes: &[u8], expected: usize) -> Result<()> {
    if bytes.len() < expected {
        return Err(Error::TooShortBuffer {
            actual: bytes.len(),
            expected,
            file: file!(),
            line: line!(),
        });
    }
    Ok(())
}

/// Validate that a block header has the expected ID.
#[inline]
pub fn validate_block_id(header: &BlockHeader, expected_id: &str) -> Result<()> {
    if header.id != expected_id {
        return Err(Error::BlockSerializationError(format!(
            "Block must have ID '{}', found '{}'",
            expected_id, header.id
        )));
    }
    Ok(())
}

/// Validate that a block header has the expected length.
#[inline]
pub fn validate_block_length(header: &BlockHeader, expected: u64) -> Result<()> {
    if header.length != expected {
        return Err(Error::BlockSerializationError(format!(
            "Block must have length={}, found {}",
            expected, header.length
        )));
    }
    Ok(())
}

/// Assert that a buffer size is 8-byte aligned (debug builds only).
#[inline]
pub fn debug_assert_aligned(size: usize) {
    debug_assert_eq!(size % 8, 0, "Block size {} is not 8-byte aligned", size);
}

/// Calculate padding needed to reach 8-byte alignment.
#[inline]
pub const fn padding_to_align_8(size: usize) -> usize {
    (8 - (size % 8)) % 8
}

/// Safely convert a u64 offset/address to usize for indexing.
///
/// On 64-bit systems, this is always safe. On 32-bit systems, returns an error
/// if the value exceeds `usize::MAX`, preventing potential overflow issues.
///
/// # Arguments
/// * `value` - The u64 value to convert (typically a file offset or address).
/// * `context` - Description of what the value represents (for error messages).
///
/// # Returns
/// The value as `usize`, or an error if conversion would overflow.
#[inline]
pub fn u64_to_usize(value: u64, context: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        Error::BlockSerializationError(format!(
            "{} value {} exceeds maximum addressable size on this platform",
            context, value
        ))
    })
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    /// 4-byte block type identifier (e.g., "##HD", "##DG").
    pub id: String,
    /// Reserved field, always 0.
    pub reserved: u32,
    /// Total length of the block in bytes, including this header.
    pub length: u64,
    /// Number of link fields in this block.
    pub link_count: u64,
}

impl Default for BlockHeader {
    /// Returns a BlockHeader with id 'UNSET' and length 0 as a placeholder.
    /// This is not a valid MDF block header and should be replaced before writing.
    fn default() -> Self {
        BlockHeader {
            id: String::from("UNSET"),
            reserved: 0,
            length: 0,
            link_count: 0,
        }
    }
}

impl BlockHeader {
    /// Serializes the BlockHeader to bytes according to MDF 4.1 specification.
    ///
    /// The BlockHeader is always 24 bytes and consists of:
    /// - id: 4 bytes (ASCII characters, must be exactly 4 bytes)
    /// - reserved: 4 bytes (always 0)
    /// - length: 8 bytes (total length of the block including this header)
    /// - link_count: 8 bytes (number of links in this block)
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized block header
    /// - `Err(Error)` if serialization fails
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(24);

        // 1. Write the ID field (4 bytes)
        let id_bytes = self.id.as_bytes();
        let mut id_field = [0u8; 4];
        let id_len = core::cmp::min(id_bytes.len(), 4);
        id_field[..id_len].copy_from_slice(&id_bytes[..id_len]);
        buffer.extend_from_slice(&id_field);

        // 2. Write reserved field (4 bytes)
        buffer.extend_from_slice(&self.reserved.to_le_bytes());

        // 3. Write length field (8 bytes)
        buffer.extend_from_slice(&self.length.to_le_bytes());

        // 4. Write link_count field (8 bytes)
        buffer.extend_from_slice(&self.link_count.to_le_bytes());

        debug_assert_eq!(buffer.len(), 24);
        Ok(buffer)
    }

    /// Parse a block header from the first 24 bytes of `bytes`.
    ///
    /// # Arguments
    /// * `bytes` - Slice containing at least 24 bytes from the MDF file.
    ///
    /// # Returns
    /// A [`BlockHeader`] on success or [`Error::TooShortBuffer`] when the
    /// slice is smaller than 24 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        validate_buffer_size(bytes, 24)?;

        let id = match core::str::from_utf8(&bytes[0..4]) {
            Ok(s) => String::from(s),
            Err(_) => String::from_utf8_lossy(&bytes[0..4]).into_owned(),
        };

        Ok(Self {
            id,
            reserved: read_u32(bytes, 4),
            length: read_u64(bytes, 8),
            link_count: read_u64(bytes, 16),
        })
    }
}

pub trait BlockParse<'a>: Sized {
    const ID: &'static str;

    fn parse_header(bytes: &[u8]) -> Result<BlockHeader> {
        let header = BlockHeader::from_bytes(&bytes[0..24])?;
        if header.id != Self::ID {
            return Err(Error::BlockIDError {
                actual: header.id.clone(),
                expected: Self::ID.to_string(),
            });
        }
        Ok(header)
    }

    fn from_bytes(bytes: &'a [u8]) -> Result<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DataType {
    UnsignedIntegerLE,
    UnsignedIntegerBE,
    SignedIntegerLE,
    SignedIntegerBE,
    FloatLE,
    FloatBE,
    StringLatin1,
    StringUtf8,
    StringUtf16LE,
    StringUtf16BE,
    ByteArray,
    MimeSample,
    MimeStream,
    CanOpenDate,
    CanOpenTime,
    ComplexLE,
    ComplexBE,
    Unknown(()),
}

impl DataType {
    /// Converts the DataType enum value to its corresponding u8 representation
    /// according to the MDF 4.1 specification.
    ///
    /// # Returns
    /// The u8 value corresponding to this DataType
    ///
    /// # Note
    /// For ComplexLE, ComplexBE, and Unknown variants, we use values that match
    /// the MDF 4.1 specification (15, 16) or a default (0) for Unknown.
    pub fn to_u8(&self) -> u8 {
        match self {
            DataType::UnsignedIntegerLE => 0,
            DataType::UnsignedIntegerBE => 1,
            DataType::SignedIntegerLE => 2,
            DataType::SignedIntegerBE => 3,
            DataType::FloatLE => 4,
            DataType::FloatBE => 5,
            DataType::StringLatin1 => 6,
            DataType::StringUtf8 => 7,
            DataType::StringUtf16LE => 8,
            DataType::StringUtf16BE => 9,
            DataType::ByteArray => 10,
            DataType::MimeSample => 11,
            DataType::MimeStream => 12,
            DataType::CanOpenDate => 13,
            DataType::CanOpenTime => 14,
            DataType::ComplexLE => 15, // Complex numbers, little-endian
            DataType::ComplexBE => 16, // Complex numbers, big-endian
            DataType::Unknown(_) => 0, // Default to 0 for unknown types
        }
    }

    /// Convert a numeric representation to the corresponding `DataType`.
    /// Values outside the known range yield `DataType::Unknown`.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => DataType::UnsignedIntegerLE,
            1 => DataType::UnsignedIntegerBE,
            2 => DataType::SignedIntegerLE,
            3 => DataType::SignedIntegerBE,
            4 => DataType::FloatLE,
            5 => DataType::FloatBE,
            6 => DataType::StringLatin1,
            7 => DataType::StringUtf8,
            8 => DataType::StringUtf16LE,
            9 => DataType::StringUtf16BE,
            10 => DataType::ByteArray,
            11 => DataType::MimeSample,
            12 => DataType::MimeStream,
            13 => DataType::CanOpenDate,
            14 => DataType::CanOpenTime,
            15 => DataType::ComplexLE,
            16 => DataType::ComplexBE,
            _ => DataType::Unknown(()),
        }
    }

    /// Returns a typical bit width for this data type.
    /// This is used when creating channels without an explicit bit count.
    pub fn default_bits(&self) -> u32 {
        match self {
            DataType::UnsignedIntegerLE
            | DataType::UnsignedIntegerBE
            | DataType::SignedIntegerLE
            | DataType::SignedIntegerBE => 32,
            DataType::FloatLE | DataType::FloatBE => 32,
            DataType::StringLatin1
            | DataType::StringUtf8
            | DataType::StringUtf16LE
            | DataType::StringUtf16BE
            | DataType::ByteArray
            | DataType::MimeSample
            | DataType::MimeStream => 8,
            DataType::CanOpenDate | DataType::CanOpenTime => 64,
            DataType::ComplexLE | DataType::ComplexBE => 64,
            DataType::Unknown(_) => 8,
        }
    }
}

impl core::fmt::Display for DataType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DataType::UnsignedIntegerLE => write!(f, "uint (LE)"),
            DataType::UnsignedIntegerBE => write!(f, "uint (BE)"),
            DataType::SignedIntegerLE => write!(f, "int (LE)"),
            DataType::SignedIntegerBE => write!(f, "int (BE)"),
            DataType::FloatLE => write!(f, "float (LE)"),
            DataType::FloatBE => write!(f, "float (BE)"),
            DataType::StringLatin1 => write!(f, "string (Latin-1)"),
            DataType::StringUtf8 => write!(f, "string (UTF-8)"),
            DataType::StringUtf16LE => write!(f, "string (UTF-16 LE)"),
            DataType::StringUtf16BE => write!(f, "string (UTF-16 BE)"),
            DataType::ByteArray => write!(f, "byte array"),
            DataType::MimeSample => write!(f, "MIME sample"),
            DataType::MimeStream => write!(f, "MIME stream"),
            DataType::CanOpenDate => write!(f, "CANopen date"),
            DataType::CanOpenTime => write!(f, "CANopen time"),
            DataType::ComplexLE => write!(f, "complex (LE)"),
            DataType::ComplexBE => write!(f, "complex (BE)"),
            DataType::Unknown(_) => write!(f, "unknown"),
        }
    }
}

/// Read a text or metadata block at `address` and return its contents.
///
/// # Arguments
/// * `mmap` - The full memory mapped MDF file.
/// * `address` - Offset of the target block; use `0` for no block.
///
/// # Returns
/// The block's string contents if present or `Ok(None)` if `address` is zero or
/// the block type is not text or metadata.
pub fn read_string_block(mmap: &[u8], address: u64) -> Result<Option<String>> {
    if address == 0 {
        return Ok(None);
    }

    let offset = u64_to_usize(address, "block address")?;
    validate_buffer_size(mmap, offset + 24)?;
    let header = BlockHeader::from_bytes(&mmap[offset..offset + 24])?;

    match header.id.as_str() {
        "##TX" => Ok(Some(TextBlock::from_bytes(&mmap[offset..])?.text)),
        "##MD" => Ok(Some(MetadataBlock::from_bytes(&mmap[offset..])?.xml)),
        _ => Ok(None),
    }
}
