use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse},
};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct TextBlock {
    pub header: BlockHeader,
    pub text: String,
}

impl BlockParse<'_> for TextBlock {
    const ID: &'static str = "##TX";
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let data_len = (header.block_len as usize).saturating_sub(24);
        let expected_bytes = 24 + data_len;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }
        let data = &bytes[24..24 + data_len];

        let text = String::from_utf8_lossy(data)
            .trim_matches('\0') // Trim all leading and trailing null characters.
            .to_string();

        Ok(Self { header, text })
    }
}

impl TextBlock {
    /// Creates a new TextBlock with the provided text content.
    /// This will automatically calculate the correct block size based on the text length.
    ///
    /// # Arguments
    /// * `text` - The text content to store in this block
    ///
    /// # Returns
    /// A new TextBlock with properly initialized header and text content
    pub fn new(text: &str) -> Self {
        // Calculate required block size: 24 bytes for header + text length
        // The block size must be a multiple of 8 bytes for alignment
        let text_bytes = text.as_bytes();
        let needs_null = text_bytes.is_empty() || *text_bytes.last().unwrap() != 0;
        let text_size = text_bytes.len() + if needs_null { 1 } else { 0 };
        let unpadded_size = 24 + text_size;
        let padding_bytes = (8 - (unpadded_size % 8)) % 8;
        let block_len = unpadded_size + padding_bytes;

        // Create header with proper ID, size, and link count (0 for TextBlock)
        let header = BlockHeader {
            id: String::from("##TX"),
            reserved0: 0,
            block_len: block_len as u64,
            links_nr: 0, // TextBlock has no links
        };

        TextBlock {
            header,
            text: text.to_string(),
        }
    }

    /// Creates an empty TextBlock with a minimal valid size.
    ///
    /// # Returns
    /// A new TextBlock with an empty text string
    pub fn new_empty() -> Self {
        Self::new("")
    }

    /// Serializes the TextBlock to bytes according to MDF 4.1 specification.
    ///
    /// TextBlock structure consists of:
    /// - BlockHeader (24 bytes)
    /// - Text content (variable length, null-terminated)
    /// - Optional padding to maintain 8-byte alignment
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized text block
    /// - `Err(MdfError)` if serialization fails
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Validate the block ID
        if self.header.id != "##TX" {
            return Err(Error::BlockSerializationError(format!(
                "TextBlock must have ID '##TX', found '{}'",
                self.header.id
            )));
        }

        // Get the text as bytes
        let text_bytes = self.text.as_bytes();
        let needs_null = text_bytes.is_empty() || *text_bytes.last().unwrap() != 0;
        let text_size = text_bytes.len() + if needs_null { 1 } else { 0 };

        // Calculate total size including header, text (with null) and padding
        let unpadded_size = 24 + text_size;
        let padding_bytes = (8 - (unpadded_size % 8)) % 8;
        let total_size = unpadded_size + padding_bytes;

        // Verify block_len in header matches calculated size
        if self.header.block_len as usize != total_size {
            return Err(Error::BlockSerializationError(format!(
                "TextBlock header.block_len ({}) does not match calculated size ({})",
                self.header.block_len, total_size
            )));
        }

        // Create a buffer with exact capacity
        let mut buffer = Vec::with_capacity(total_size);

        // 1. Write the header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // 2. Write the text bytes
        buffer.extend_from_slice(text_bytes);

        // 3. Add null terminator if not already present
        if needs_null {
            buffer.push(0);
        }

        // 4. Add padding to maintain 8-byte alignment
        let current_size = buffer.len();
        let remaining_padding = total_size - current_size;
        if remaining_padding > 0 {
            buffer.extend(vec![0u8; remaining_padding]);
        }

        // Verify the final size is correct and 8-byte aligned
        if buffer.len() != total_size {
            return Err(Error::BlockSerializationError(format!(
                "TextBlock has incorrect final size: expected {}, got {}",
                total_size,
                buffer.len()
            )));
        }

        debug_assert_eq!(buffer.len() % 8, 0, "TextBlock size is not 8-byte aligned");

        Ok(buffer)
    }
}

impl Default for TextBlock {
    fn default() -> Self {
        Self::new("")
    }
}
