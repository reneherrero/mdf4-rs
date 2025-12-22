use super::SI_BLOCK_SIZE;
#[cfg(feature = "std")]
use crate::blocks::common::u64_to_usize;
use crate::{
    Result,
    blocks::common::{
        BlockHeader, BlockParse, debug_assert_aligned, read_u8, read_u64, validate_buffer_size,
    },
};

/// Source Information Block (##SI) - describes the source of acquired data.
///
/// A source block identifies where data comes from (ECU, bus, I/O device, etc.)
/// and is typically linked from channel groups or channels.
#[derive(Debug, Clone)]
pub struct SourceBlock {
    pub header: BlockHeader,
    /// Link to text block containing the source name.
    pub name_addr: u64,
    /// Link to text block containing a tool-specific path.
    pub path_addr: u64,
    /// Link to text/metadata block with extended comment.
    pub comment_addr: u64,
    /// Source type (see [`SourceType`]).
    pub source_type: u8,
    /// Bus type (see [`BusType`]).
    pub bus_type: u8,
    /// Flags (bit 0 = simulated source).
    pub flags: u8,
}

impl BlockParse<'_> for SourceBlock {
    const ID: &'static str = "##SI";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let link_count = header.link_count as usize;
        let data_start = 24 + link_count * 8;
        validate_buffer_size(bytes, data_start + 3)?;

        // Read links (up to 3)
        let name_addr = if link_count > 0 {
            read_u64(bytes, 24)
        } else {
            0
        };
        let path_addr = if link_count > 1 {
            read_u64(bytes, 32)
        } else {
            0
        };
        let comment_addr = if link_count > 2 {
            read_u64(bytes, 40)
        } else {
            0
        };

        Ok(Self {
            header,
            name_addr,
            path_addr,
            comment_addr,
            source_type: read_u8(bytes, data_start),
            bus_type: read_u8(bytes, data_start + 1),
            flags: read_u8(bytes, data_start + 2),
        })
    }
}

/// Source type constants for SourceBlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SourceType {
    /// Other source type
    Other = 0,
    /// Electronic Control Unit
    ECU = 1,
    /// Bus (CAN, LIN, etc.)
    Bus = 2,
    /// I/O device
    IO = 3,
    /// Tool
    Tool = 4,
    /// User-defined
    User = 5,
}

/// Bus type constants for SourceBlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BusType {
    /// No bus
    None = 0,
    /// Other bus type
    Other = 1,
    /// CAN bus
    CAN = 2,
    /// LIN bus
    LIN = 3,
    /// MOST bus
    MOST = 4,
    /// FlexRay
    FlexRay = 5,
    /// K-Line
    KLine = 6,
    /// Ethernet
    Ethernet = 7,
    /// USB
    USB = 8,
}

impl SourceBlock {
    /// Creates a new SourceBlock with the specified source and bus types.
    pub fn new(source_type: SourceType, bus_type: BusType) -> Self {
        Self {
            header: BlockHeader {
                id: alloc::string::String::from("##SI"),
                reserved: 0,
                length: SI_BLOCK_SIZE as u64,
                link_count: 3,
            },
            name_addr: 0,
            path_addr: 0,
            comment_addr: 0,
            source_type: source_type as u8,
            bus_type: bus_type as u8,
            flags: 0,
        }
    }

    /// Creates a new SourceBlock for a CAN ECU.
    pub fn can_ecu() -> Self {
        Self::new(SourceType::ECU, BusType::CAN)
    }

    /// Creates a new SourceBlock for a CAN bus.
    pub fn can_bus() -> Self {
        Self::new(SourceType::Bus, BusType::CAN)
    }

    /// Serializes the SourceBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<alloc::vec::Vec<u8>> {
        use alloc::vec::Vec;

        let mut buffer = Vec::with_capacity(SI_BLOCK_SIZE);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (24 bytes)
        buffer.extend_from_slice(&self.name_addr.to_le_bytes());
        buffer.extend_from_slice(&self.path_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Data section (8 bytes)
        buffer.push(self.source_type);
        buffer.push(self.bus_type);
        buffer.push(self.flags);
        buffer.extend_from_slice(&[0u8; 5]); // reserved

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }
}

impl Default for SourceBlock {
    fn default() -> Self {
        Self::can_ecu()
    }
}

/// Read an [`SIBLOCK`](SourceBlock) from the memory mapped file.
///
/// # Arguments
/// * `mmap` - The entire MDF file mapped into memory.
/// * `address` - File offset of the `##SI` block.
///
/// # Returns
/// The parsed [`SourceBlock`] or an [`Error`] if decoding fails.
#[cfg(feature = "std")]
pub fn read_source_block(mmap: &[u8], address: u64) -> Result<SourceBlock> {
    let start = u64_to_usize(address, "source block address")?;
    validate_buffer_size(mmap, start + 24)?;
    let header = BlockHeader::from_bytes(&mmap[start..start + 24])?;
    let total_len = u64_to_usize(header.length, "source block length")?;
    validate_buffer_size(mmap, start + total_len)?;
    let slice = &mmap[start..start + total_len];
    SourceBlock::from_bytes(slice)
}
