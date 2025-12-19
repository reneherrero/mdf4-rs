// blocks/common.rs
use crate::{
    Error, Result,
    blocks::{metadata_block::MetadataBlock, text_block::TextBlock},
};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    pub id: String,     // 4-byte string
    pub reserved0: u32, // 4 bytes
    pub block_len: u64, // 8 bytes
    pub links_nr: u64,  // 8 bytes
}

impl Default for BlockHeader {
    /// Returns a BlockHeader with id 'UNSET' and block_len 0 as a placeholder.
    /// This is not a valid MDF block header and should be replaced before writing.
    fn default() -> Self {
        BlockHeader {
            id: String::from("UNSET"), // Placeholder, must be set by user
            reserved0: 0,
            block_len: 0, // Placeholder, must be set by user
            links_nr: 0,
        }
    }
}

impl BlockHeader {
    /// Serializes the BlockHeader to bytes according to MDF 4.1 specification.
    ///
    /// The BlockHeader is always 24 bytes and consists of:
    /// - id: 4 bytes (ASCII characters, must be exactly 4 bytes)
    /// - reserved0: 4 bytes (always 0)
    /// - block_len: 8 bytes (total length of the block including this header)
    /// - links_nr: 8 bytes (number of links in this block)
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized block header
    /// - `Err(MdfError)` if serialization fails
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Create a buffer with exactly 24 bytes capacity
        let mut buffer = Vec::with_capacity(24);

        // 1. Write the ID field (4 bytes)
        // The ID must be exactly 4 bytes - truncate or pad as needed
        let id_bytes = self.id.as_bytes();
        let mut id_field = [0u8; 4]; // Initialize with zeros for padding

        // Copy either all bytes or first 4 bytes of ID
        let id_len = core::cmp::min(id_bytes.len(), 4);
        id_field[..id_len].copy_from_slice(&id_bytes[..id_len]);
        buffer.extend_from_slice(&id_field);

        // 2. Write reserved0 field (4 bytes)
        buffer.extend_from_slice(&self.reserved0.to_le_bytes());

        // 3. Write block_len field (8 bytes)
        buffer.extend_from_slice(&self.block_len.to_le_bytes());

        // 4. Write links_nr field (8 bytes)
        buffer.extend_from_slice(&self.links_nr.to_le_bytes());

        // Verify buffer is exactly 24 bytes
        if buffer.len() != 24 {
            return Err(Error::BlockSerializationError(format!(
                "BlockHeader must be exactly 24 bytes, got {}",
                buffer.len()
            )));
        }

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
        let expected_bytes = 24;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }
        Ok(Self {
            id: String::from_utf8_lossy(&bytes[0..4]).to_string(),
            reserved0: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            block_len: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            links_nr: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

    let offset = address as usize;
    let header = BlockHeader::from_bytes(&mmap[offset..offset + 24])?;

    match header.id.as_str() {
        "##TX" => Ok(Some(TextBlock::from_bytes(&mmap[offset..])?.text)),
        "##MD" => Ok(Some(MetadataBlock::from_bytes(&mmap[offset..])?.xml)),
        _ => Ok(None),
    }
}
