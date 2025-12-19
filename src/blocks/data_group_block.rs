use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse},
};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(Debug)]
pub struct DataGroupBlock {
    pub header: BlockHeader, // Common header
    pub next_dg_addr: u64,
    pub first_cg_addr: u64,
    pub data_block_addr: u64,
    pub comment_addr: u64,
    pub record_id_len: u8,
    pub reserved1: String,
}

impl BlockParse<'_> for DataGroupBlock {
    const ID: &'static str = "##DG";
    /// Parse a `DataGroupBlock` from a 64 byte slice.
    ///
    /// # Arguments
    /// * `bytes` - Byte slice beginning at the DG block header.
    ///
    /// # Returns
    /// The populated [`DataGroupBlock`] on success or an [`Error`] if the
    /// slice is too small or malformed.
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let expected_bytes = 64;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }

        Ok(Self {
            header,
            next_dg_addr: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            first_cg_addr: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            data_block_addr: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            comment_addr: u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
            record_id_len: bytes[56],
            reserved1: String::from_utf8_lossy(&bytes[57..64]).to_string(),
        })
    }
}

impl DataGroupBlock {
    /// Serializes the DataGroupBlock to bytes according to MDF 4.1 specification.
    ///
    /// # Structure (64 bytes total):
    /// - BlockHeader (24 bytes): Standard block header with id="##DG"
    /// - next_dg_addr (8 bytes): Link to next data group block
    /// - first_cg_addr (8 bytes): Link to first channel group block
    /// - data_block_addr (8 bytes): Link to the data block
    /// - comment_addr (8 bytes): Link to comment text block
    /// - record_id_len (1 byte): Record ID length
    /// - reserved1 (7 bytes): Reserved space
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized data group block
    /// - `Err(MdfError)` if serialization fails
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Validate the header before serializing
        if self.header.id != "##DG" {
            return Err(Error::BlockSerializationError(format!(
                "DataGroupBlock must have ID '##DG', found '{}'",
                self.header.id
            )));
        }

        if self.header.block_len != 64 {
            return Err(Error::BlockSerializationError(format!(
                "DataGroupBlock must have block_len=64, found {}",
                self.header.block_len
            )));
        }

        // Create a buffer with exact capacity for efficiency
        let mut buffer = Vec::with_capacity(64);

        // 1. Write the block header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // 2. Write the link addresses (32 bytes total)
        buffer.extend_from_slice(&self.next_dg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_cg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.data_block_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // 3. Write record ID length (1 byte)
        buffer.push(self.record_id_len);

        // 4. Write reserved space (7 bytes)
        // The reserved field is stored as a String for reading, but for writing
        // we just write 7 bytes of zeros as per spec
        buffer.extend_from_slice(&[0u8; 7]);

        // Verify the buffer is exactly 64 bytes
        if buffer.len() != 64 {
            return Err(Error::BlockSerializationError(format!(
                "DataGroupBlock must be exactly 64 bytes, got {}",
                buffer.len()
            )));
        }

        // Ensure 8-byte alignment (should always be true since 64 is divisible by 8)
        debug_assert_eq!(
            buffer.len() % 8,
            0,
            "DataGroupBlock size is not 8-byte aligned"
        );

        Ok(buffer)
    }
}

impl Default for DataGroupBlock {
    fn default() -> Self {
        let header = BlockHeader {
            id: String::from("##DG"),
            reserved0: 0,
            block_len: 64,
            links_nr: 4,
        };

        DataGroupBlock {
            header,
            next_dg_addr: 0,
            first_cg_addr: 0,
            data_block_addr: 0,
            comment_addr: 0,
            record_id_len: 0,
            reserved1: String::new(),
        }
    }
}
