use crate::{
    Error, Result,
    blocks::common::{
        BlockHeader, BlockParse, debug_assert_aligned, padding_to_align_8, validate_block_id,
        validate_buffer_size,
    },
};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Text Block (##TX) - stores plain text strings.
///
/// Text blocks are used to store names, comments, and other string data
/// throughout the MDF file. The text is stored as null-terminated UTF-8.
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub header: BlockHeader,
    /// The text content (without null terminator).
    pub text: String,
}

impl BlockParse<'_> for TextBlock {
    const ID: &'static str = "##TX";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let data_len = (header.length as usize).saturating_sub(24);
        validate_buffer_size(bytes, 24 + data_len)?;

        let data = &bytes[24..24 + data_len];

        // Parse text efficiently: try UTF-8 first, fall back to lossy conversion
        let text = match core::str::from_utf8(data) {
            Ok(s) => s.trim_matches('\0').to_string(),
            Err(_) => String::from_utf8_lossy(data).trim_matches('\0').to_string(),
        };

        Ok(Self { header, text })
    }
}

impl TextBlock {
    /// Creates a new TextBlock with the provided text content.
    ///
    /// Automatically calculates the correct block size based on the text length,
    /// ensuring proper 8-byte alignment.
    pub fn new(text: &str) -> Self {
        let block_len = Self::calculate_block_len(text);

        Self {
            header: BlockHeader {
                id: String::from("##TX"),
                reserved: 0,
                length: block_len as u64,
                link_count: 0,
            },
            text: String::from(text),
        }
    }

    /// Creates an empty TextBlock with a minimal valid size.
    pub fn new_empty() -> Self {
        Self::new("")
    }

    /// Calculates the block length for a given text string.
    fn calculate_block_len(text: &str) -> usize {
        let text_bytes = text.as_bytes();
        let needs_null = text_bytes.is_empty() || text_bytes.last() != Some(&0);
        let text_size = text_bytes.len() + if needs_null { 1 } else { 0 };
        let unpadded_size = 24 + text_size;
        unpadded_size + padding_to_align_8(unpadded_size)
    }

    /// Serializes the TextBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        validate_block_id(&self.header, "##TX")?;

        let text_bytes = self.text.as_bytes();
        let needs_null = text_bytes.is_empty() || text_bytes.last() != Some(&0);
        let total_size = Self::calculate_block_len(&self.text);

        if self.header.length as usize != total_size {
            return Err(Error::BlockSerializationError(format!(
                "TextBlock header.length ({}) does not match calculated size ({})",
                self.header.length, total_size
            )));
        }

        let mut buffer = Vec::with_capacity(total_size);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Text content
        buffer.extend_from_slice(text_bytes);
        if needs_null {
            buffer.push(0);
        }

        // Padding to 8-byte alignment
        buffer.resize(total_size, 0);

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }
}

impl Default for TextBlock {
    fn default() -> Self {
        Self::new("")
    }
}
