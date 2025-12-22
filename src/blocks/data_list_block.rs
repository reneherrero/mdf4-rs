use crate::{
    Error, Result,
    blocks::common::{
        BlockHeader, BlockParse, debug_assert_aligned, read_u8, read_u32, read_u64,
        validate_block_id, validate_buffer_size,
    },
};
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Data List Block (##DL) - ordered list of data blocks.
///
/// A data list block provides a way to split large data into multiple fragments.
/// It contains links to data blocks (DT, DZ, etc.) and optional offset information.
#[derive(Debug, Clone)]
pub struct DataListBlock {
    pub header: BlockHeader,
    /// Link to next data list block (0 if last).
    pub next_dl_addr: u64,
    /// Links to data block fragments (DT, DZ, DV, RV, etc.).
    pub data_block_addrs: Vec<u64>,
    /// Flags (bit 0: equal length blocks).
    pub flags: u8,
    /// Number of data blocks referenced.
    pub data_block_count: u32,
    /// Length of each data block (only if flags bit 0 is set).
    pub equal_length: Option<u64>,
    /// Cumulative byte offsets for each block (only if flags bit 0 is NOT set).
    pub block_offsets: Option<Vec<u64>>,
}

impl BlockParse<'_> for DataListBlock {
    const ID: &'static str = "##DL";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let link_count = header.link_count as usize;
        let min_len = 24 + (link_count * 8) + 8; // header + links + data section minimum
        validate_buffer_size(bytes, min_len)?;

        // Parse links: first is 'next', then data block addresses
        let next_dl_addr = read_u64(bytes, 24);

        let mut data_block_addrs = Vec::with_capacity(link_count.saturating_sub(1));
        for i in 1..link_count {
            data_block_addrs.push(read_u64(bytes, 24 + i * 8));
        }

        let data_offset = 24 + link_count * 8;
        let flags = read_u8(bytes, data_offset);
        // bytes [data_offset+1..data_offset+4] are reserved
        let data_block_count = read_u32(bytes, data_offset + 4);

        let (equal_length, block_offsets) = if flags & 1 != 0 {
            // Equal length mode
            validate_buffer_size(bytes, data_offset + 16)?;
            let len = read_u64(bytes, data_offset + 8);
            (Some(len), None)
        } else {
            // Variable length mode with offsets
            let offsets_start = data_offset + 8;
            let offsets_len = data_block_count as usize * 8;
            validate_buffer_size(bytes, offsets_start + offsets_len)?;

            let mut offsets = Vec::with_capacity(data_block_count as usize);
            for i in 0..data_block_count as usize {
                offsets.push(read_u64(bytes, offsets_start + i * 8));
            }
            (None, Some(offsets))
        };

        Ok(Self {
            header,
            next_dl_addr,
            data_block_addrs,
            flags,
            data_block_count,
            equal_length,
            block_offsets,
        })
    }
}

impl DataListBlock {
    /// Creates a new DataListBlock for equal-length data blocks.
    ///
    /// Use this when all referenced data blocks have the same size.
    pub fn new_equal_length(data_block_addrs: Vec<u64>, block_length: u64) -> Self {
        let link_count = data_block_addrs.len() as u64 + 1; // +1 for 'next'
        let length = 24 + link_count * 8 + 16; // header + links + data section

        Self {
            header: BlockHeader {
                id: "##DL".to_string(),
                reserved: 0,
                length,
                link_count,
            },
            next_dl_addr: 0,
            data_block_count: data_block_addrs.len() as u32,
            data_block_addrs,
            flags: 1, // Equal length flag
            equal_length: Some(block_length),
            block_offsets: None,
        }
    }

    /// Serializes the DataListBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        validate_block_id(&self.header, "##DL")?;

        let link_count = self.data_block_addrs.len() as u64 + 1;
        let data_section_size = if self.flags & 1 != 0 {
            16 // flags(1) + reserved(3) + count(4) + length(8)
        } else {
            8 + (self.data_block_count as usize * 8) // flags(1) + reserved(3) + count(4) + offsets
        };
        let expected_length = 24 + link_count * 8 + data_section_size as u64;

        if self.header.link_count != link_count {
            return Err(Error::BlockSerializationError(format!(
                "DataListBlock link_count mismatch: header {} vs actual {}",
                self.header.link_count, link_count
            )));
        }
        if self.header.length != expected_length {
            return Err(Error::BlockSerializationError(format!(
                "DataListBlock length mismatch: header {} vs actual {}",
                self.header.length, expected_length
            )));
        }

        let mut buffer = Vec::with_capacity(expected_length as usize);

        // Header
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links
        buffer.extend_from_slice(&self.next_dl_addr.to_le_bytes());
        for addr in &self.data_block_addrs {
            buffer.extend_from_slice(&addr.to_le_bytes());
        }

        // Data section
        buffer.push(self.flags);
        buffer.extend_from_slice(&[0u8; 3]); // reserved
        buffer.extend_from_slice(&self.data_block_count.to_le_bytes());

        if self.flags & 1 != 0 {
            buffer.extend_from_slice(&self.equal_length.unwrap_or(0).to_le_bytes());
        } else if let Some(offsets) = &self.block_offsets {
            for offset in offsets {
                buffer.extend_from_slice(&offset.to_le_bytes());
            }
        }

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }
}
