use super::CG_BLOCK_SIZE;
use crate::{
    Result,
    blocks::{
        channel_block::ChannelBlock,
        common::{
            BlockHeader, BlockParse, debug_assert_aligned, read_u16, read_u32, read_u64,
            validate_block_id, validate_block_length, validate_buffer_size,
        },
    },
};
use alloc::string::String;
use alloc::vec::Vec;

/// Channel Group Block (##CG) - groups channels that share a common time base.
///
/// A channel group contains one or more channels that are acquired together
/// and share the same number of cycles (records). Each data group can contain
/// multiple channel groups.
#[derive(Debug, Clone)]
pub struct ChannelGroupBlock {
    pub header: BlockHeader,
    /// Link to next channel group block (0 if last).
    pub next_cg_addr: u64,
    /// Link to first channel block in this group.
    pub first_ch_addr: u64,
    /// Link to acquisition name text block.
    pub acq_name_addr: u64,
    /// Link to acquisition source information block.
    pub acq_source_addr: u64,
    /// Link to first sample reduction block.
    pub first_sample_reduction_addr: u64,
    /// Link to comment text/metadata block.
    pub comment_addr: u64,
    /// Record ID for identifying records in unsorted data.
    pub record_id: u64,
    /// Number of cycles (records) in this channel group.
    pub cycle_count: u64,
    /// Channel group flags.
    pub flags: u16,
    /// Path separator character for hierarchical channel names.
    pub path_separator: u16,
    /// Size of each record in bytes (excluding invalidation bytes).
    pub record_size: u32,
    /// Number of invalidation bytes per record.
    pub invalidation_size: u32,
}

impl BlockParse<'_> for ChannelGroupBlock {
    const ID: &'static str = "##CG";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        validate_buffer_size(bytes, CG_BLOCK_SIZE)?;

        Ok(Self {
            header,
            // Links section (6 x u64 = 48 bytes at offset 24)
            next_cg_addr: read_u64(bytes, 24),
            first_ch_addr: read_u64(bytes, 32),
            acq_name_addr: read_u64(bytes, 40),
            acq_source_addr: read_u64(bytes, 48),
            first_sample_reduction_addr: read_u64(bytes, 56),
            comment_addr: read_u64(bytes, 64),
            // Data section at offset 72
            record_id: read_u64(bytes, 72),
            cycle_count: read_u64(bytes, 80),
            flags: read_u16(bytes, 88),
            path_separator: read_u16(bytes, 90),
            // bytes 92-96: reserved (skipped)
            record_size: read_u32(bytes, 96),
            invalidation_size: read_u32(bytes, 100),
        })
    }
}
impl ChannelGroupBlock {
    /// Serializes the ChannelGroupBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        validate_block_id(&self.header, "##CG")?;
        validate_block_length(&self.header, CG_BLOCK_SIZE as u64)?;

        let mut buffer = Vec::with_capacity(CG_BLOCK_SIZE);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (48 bytes)
        buffer.extend_from_slice(&self.next_cg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_ch_addr.to_le_bytes());
        buffer.extend_from_slice(&self.acq_name_addr.to_le_bytes());
        buffer.extend_from_slice(&self.acq_source_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_sample_reduction_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Data section (32 bytes)
        buffer.extend_from_slice(&self.record_id.to_le_bytes());
        buffer.extend_from_slice(&self.cycle_count.to_le_bytes());
        buffer.extend_from_slice(&self.flags.to_le_bytes());
        buffer.extend_from_slice(&self.path_separator.to_le_bytes());
        buffer.extend_from_slice(&[0u8; 4]); // reserved
        buffer.extend_from_slice(&self.record_size.to_le_bytes());
        buffer.extend_from_slice(&self.invalidation_size.to_le_bytes());

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }

    /// Read all channels linked to this channel group.
    ///
    /// # Arguments
    /// * `mmap` - Memory mapped MDF data used to follow the channel chain.
    ///
    /// # Returns
    /// A vector of fully parsed [`ChannelBlock`]s or an error if any
    /// channel cannot be decoded.
    pub fn read_channels(&mut self, mmap: &[u8]) -> Result<Vec<ChannelBlock>> {
        let mut channels = Vec::new();
        let mut current_ch_addr = self.first_ch_addr;

        while current_ch_addr != 0 {
            let ch_offset = current_ch_addr as usize;
            let mut channel = ChannelBlock::from_bytes(&mmap[ch_offset..])?;
            channel.resolve_conversion(mmap)?;
            current_ch_addr = channel.next_ch_addr;
            channels.push(channel);
        }

        Ok(channels)
    }
}

impl Default for ChannelGroupBlock {
    fn default() -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##CG"),
                reserved: 0,
                length: CG_BLOCK_SIZE as u64,
                link_count: 6,
            },
            next_cg_addr: 0,
            first_ch_addr: 0,
            acq_name_addr: 0,
            acq_source_addr: 0,
            first_sample_reduction_addr: 0,
            comment_addr: 0,
            record_id: 0,
            cycle_count: 0,
            flags: 0,
            path_separator: 0,
            record_size: 0,
            invalidation_size: 0,
        }
    }
}
