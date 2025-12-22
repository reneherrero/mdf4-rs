use super::DG_BLOCK_SIZE;
use crate::{
    Result,
    blocks::common::{
        BlockHeader, BlockParse, debug_assert_aligned, read_u8, read_u64, validate_block_id,
        validate_block_length, validate_buffer_size,
    },
};
use alloc::string::String;
use alloc::vec::Vec;

/// Data Group Block (##DG) - groups channel groups that share a data block.
///
/// A data group typically corresponds to one acquisition device. It contains
/// links to channel groups and the actual measurement data block.
#[derive(Debug, Clone)]
pub struct DataGroupBlock {
    pub header: BlockHeader,
    /// Link to next data group block (0 if last).
    pub next_dg_addr: u64,
    /// Link to first channel group block.
    pub first_cg_addr: u64,
    /// Link to data block (DT, DZ, DL, HL, etc.).
    pub data_block_addr: u64,
    /// Link to comment text/metadata block.
    pub comment_addr: u64,
    /// Size of record ID in bytes (0, 1, 2, 4, or 8).
    pub record_id_size: u8,
}

impl BlockParse<'_> for DataGroupBlock {
    const ID: &'static str = "##DG";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        validate_buffer_size(bytes, DG_BLOCK_SIZE)?;

        Ok(Self {
            header,
            // Links section (4 x u64 = 32 bytes at offset 24)
            next_dg_addr: read_u64(bytes, 24),
            first_cg_addr: read_u64(bytes, 32),
            data_block_addr: read_u64(bytes, 40),
            comment_addr: read_u64(bytes, 48),
            // Data section at offset 56
            record_id_size: read_u8(bytes, 56),
            // bytes 57-64: reserved (skipped)
        })
    }
}

impl DataGroupBlock {
    /// Serializes the DataGroupBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        validate_block_id(&self.header, "##DG")?;
        validate_block_length(&self.header, DG_BLOCK_SIZE as u64)?;

        let mut buffer = Vec::with_capacity(DG_BLOCK_SIZE);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (32 bytes)
        buffer.extend_from_slice(&self.next_dg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_cg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.data_block_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Data section (8 bytes)
        buffer.push(self.record_id_size);
        buffer.extend_from_slice(&[0u8; 7]); // reserved

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }
}

impl Default for DataGroupBlock {
    fn default() -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##DG"),
                reserved: 0,
                length: DG_BLOCK_SIZE as u64,
                link_count: 4,
            },
            next_dg_addr: 0,
            first_cg_addr: 0,
            data_block_addr: 0,
            comment_addr: 0,
            record_id_size: 0,
        }
    }
}
