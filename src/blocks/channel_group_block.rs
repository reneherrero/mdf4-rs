use crate::{
    Error, Result,
    blocks::{
        channel_block::ChannelBlock,
        common::{BlockHeader, BlockParse},
    },
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct ChannelGroupBlock {
    pub header: BlockHeader,              // Common header
    pub next_cg_addr: u64,                // 8 bytes
    pub first_ch_addr: u64,               // 8 bytes
    pub acq_name_addr: u64,               // 8 bytes
    pub acq_source_addr: u64,             // 8 bytes
    pub first_sample_reduction_addr: u64, // 8 bytes
    pub comment_addr: u64,                // 8 bytes
    pub record_id: u64,                   // 8 bytes
    pub cycles_nr: u64,                   // 8 bytes
    pub flags: u16,                       // 2 bytes
    pub path_separator: u16,              // 2 bytes
    pub reserved1: u32,                   // 4 bytes
    pub samples_byte_nr: u32,             // 4 bytes
    pub invalidation_bytes_nr: u32,       // 4 bytes
}

impl BlockParse<'_> for ChannelGroupBlock {
    const ID: &'static str = "##CG";
    /// Creates a ChannelGroupBlock from a 104-byte slice.
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let expected_bytes = 104;
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
            next_cg_addr: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            first_ch_addr: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            acq_name_addr: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            acq_source_addr: u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
            first_sample_reduction_addr: u64::from_le_bytes(bytes[56..64].try_into().unwrap()),
            comment_addr: u64::from_le_bytes(bytes[64..72].try_into().unwrap()),
            record_id: u64::from_le_bytes(bytes[72..80].try_into().unwrap()),
            cycles_nr: u64::from_le_bytes(bytes[80..88].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[88..90].try_into().unwrap()),
            path_separator: u16::from_le_bytes(bytes[90..92].try_into().unwrap()),
            reserved1: u32::from_le_bytes(bytes[92..96].try_into().unwrap()),
            samples_byte_nr: u32::from_le_bytes(bytes[96..100].try_into().unwrap()),
            invalidation_bytes_nr: u32::from_le_bytes(bytes[100..104].try_into().unwrap()),
        })
    }
}
impl ChannelGroupBlock {
    /// Serializes the ChannelGroupBlock to bytes according to MDF 4.1 specification.
    ///
    /// # Structure (104 bytes total):
    /// - BlockHeader (24 bytes): Standard block header with id="##CG"
    /// - Link section (48 bytes): Six 8-byte links to other blocks
    ///   * next_cg_addr: Link to next channel group block
    ///   * first_ch_addr: Link to first channel block
    ///   * acq_name_addr: Link to acquisition name text block
    ///   * acq_source_addr: Link to acquisition source block
    ///   * first_sample_reduction_addr: Link to first sample reduction block
    ///   * comment_addr: Link to comment text block
    /// - Data section (32 bytes): Information about the channel group data
    ///   * record_id: Record ID (u64)
    ///   * cycles_nr: Number of cycles (u64)
    ///   * flags: Flags (u16)
    ///   * path_separator: Path separator character (u16)
    ///   * reserved1: Reserved space (u32)
    ///   * samples_byte_nr: Number of bytes for samples (u32)
    ///   * invalidation_bytes_nr: Number of bytes for invalidation bits (u32)
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized channel group block
    /// - `Err(MdfError)` if serialization fails
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Validate the header before serializing
        if self.header.id != "##CG" {
            return Err(Error::BlockSerializationError(format!(
                "ChannelGroupBlock must have ID '##CG', found '{}'",
                self.header.id
            )));
        }

        if self.header.block_len != 104 {
            return Err(Error::BlockSerializationError(format!(
                "ChannelGroupBlock must have block_len=104, found {}",
                self.header.block_len
            )));
        }

        // Create a buffer with exact capacity for efficiency
        let mut buffer = Vec::with_capacity(104);

        // 1. Write the block header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // 2. Write the link addresses (48 bytes total, 6 links at 8 bytes each)
        buffer.extend_from_slice(&self.next_cg_addr.to_le_bytes()); // Next channel group
        buffer.extend_from_slice(&self.first_ch_addr.to_le_bytes()); // First channel 
        buffer.extend_from_slice(&self.acq_name_addr.to_le_bytes()); // Acquisition name
        buffer.extend_from_slice(&self.acq_source_addr.to_le_bytes()); // Acquisition source
        buffer.extend_from_slice(&self.first_sample_reduction_addr.to_le_bytes()); // Sample reduction
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes()); // Comment

        // 3. Write the data section (32 bytes)
        buffer.extend_from_slice(&self.record_id.to_le_bytes()); // Record ID (8 bytes)
        buffer.extend_from_slice(&self.cycles_nr.to_le_bytes()); // Cycles count (8 bytes)
        buffer.extend_from_slice(&self.flags.to_le_bytes()); // Flags (2 bytes)
        buffer.extend_from_slice(&self.path_separator.to_le_bytes()); // Path separator (2 bytes)
        buffer.extend_from_slice(&self.reserved1.to_le_bytes()); // Reserved (4 bytes)
        buffer.extend_from_slice(&self.samples_byte_nr.to_le_bytes()); // Sample bytes (4 bytes)
        buffer.extend_from_slice(&self.invalidation_bytes_nr.to_le_bytes()); // Invalidation bytes (4 bytes)

        // Verify the buffer is exactly 104 bytes
        if buffer.len() != 104 {
            return Err(Error::BlockSerializationError(format!(
                "ChannelGroupBlock must be exactly 104 bytes, got {}",
                buffer.len()
            )));
        }

        // Ensure 8-byte alignment (should always be true since 104 is divisible by 8)
        debug_assert_eq!(
            buffer.len() % 8,
            0,
            "ChannelGroupBlock size is not 8-byte aligned"
        );

        Ok(buffer)
    }

    /// Read all channels linked to this channel group.
    ///
    /// # Arguments
    /// * `mmap` - Memory mapped MDF data used to follow the channel chain.
    ///
    /// # Returns
    /// A vector of fully parsed [`ChannelBlock`]s or an [`Error`] if any
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
        let header = BlockHeader {
            id: String::from("##CG"),
            reserved0: 0,
            block_len: 104,
            links_nr: 6,
        };

        ChannelGroupBlock {
            header,
            next_cg_addr: 0,
            first_ch_addr: 0,
            acq_name_addr: 0,
            acq_source_addr: 0,
            first_sample_reduction_addr: 0,
            comment_addr: 0,
            record_id: 0,
            cycles_nr: 0,
            flags: 0,
            path_separator: 0,
            reserved1: 0,
            samples_byte_nr: 0,
            invalidation_bytes_nr: 0,
        }
    }
}
