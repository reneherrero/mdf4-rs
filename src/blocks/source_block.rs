use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse},
};

/// Represents an SIBLOCK (“##SI”) from the MDF4 file.
///
/// - Links:
///   • si_tx_name    LINK → TXBLOCK (source name)  
///   • si_tx_path    LINK → TXBLOCK (tool-specific path)  
///   • si_md_comment LINK → TXBLOCK/MDBLOCK (additional XML/text)
/// - Data:
///   • si_type      UINT8 (0=OTHER,1=ECU,2=BUS,3=I/O,4=TOOL,5=USER)  
///   • si_bus_type  UINT8 (0=NONE,1=OTHER,2=CAN,3=LIN,…,8=USB)  
///   • si_flags     UINT8 (bit 0 = simulated)  
///   • si_reserved  BYTE\[5\] (padding)
#[derive(Debug, Clone)]
pub struct SourceBlock {
    pub header: BlockHeader,
    /// Link to a TXBLOCK containing the human-readable source name
    pub name_addr: u64,
    /// Link to a TXBLOCK containing a tool-specific path/namespace
    pub path_addr: u64,
    /// Link to TXBLOCK or MDBLOCK with extended comment/XML
    pub comment_addr: u64,

    pub source_type: u8,
    pub bus_type: u8,
    pub flags: u8,
    // 5 bytes reserved for 8-byte alignment
}

impl BlockParse<'_> for SourceBlock {
    const ID: &'static str = "##SI";
    /// Parse an SIBLOCK from its raw bytes (starting at the “##SI…” header).
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        // Link section: one LINK (u64 LE) per link_count (max 3 meaningful)
        let mut name_addr = 0;
        let mut path_addr = 0;
        let mut comment_addr = 0;
        let link_count = header.links_nr as usize;
        for i in 0..link_count.min(3) {
            let off = 24 + i * 8;
            let link = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            match i {
                0 => name_addr = link,
                1 => path_addr = link,
                2 => comment_addr = link,
                _ => {}
            }
        }

        // Data section immediately after all links:

        let data_start = 24 + link_count * 8;

        let expected_bytes = data_start + 2;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }
        let source_type = bytes[data_start];
        let bus_type = bytes[data_start + 1];
        let flags = bytes[data_start + 2];
        // bytes [data_start+3 .. data_start+8] are reserved/padding

        Ok(SourceBlock {
            header,
            name_addr,
            path_addr,
            comment_addr,
            source_type,
            bus_type,
            flags,
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
    Ecu = 1,
    /// Bus (CAN, LIN, etc.)
    Bus = 2,
    /// I/O device
    Io = 3,
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
    Can = 2,
    /// LIN bus
    Lin = 3,
    /// MOST bus
    Most = 4,
    /// FlexRay
    FlexRay = 5,
    /// K-Line
    KLine = 6,
    /// Ethernet
    Ethernet = 7,
    /// USB
    Usb = 8,
}

impl SourceBlock {
    /// Create a new SourceBlock for an ECU with CAN bus.
    ///
    /// # Arguments
    /// * `source_type` - Type of source (ECU, Bus, etc.)
    /// * `bus_type` - Type of bus (CAN, LIN, etc.)
    pub fn new(source_type: SourceType, bus_type: BusType) -> Self {
        Self {
            header: BlockHeader {
                id: alloc::string::String::from("##SI"),
                reserved0: 0,
                block_len: 56, // 24 header + 3*8 links + 8 data
                links_nr: 3,
            },
            name_addr: 0,
            path_addr: 0,
            comment_addr: 0,
            source_type: source_type as u8,
            bus_type: bus_type as u8,
            flags: 0,
        }
    }

    /// Create a new SourceBlock for a CAN ECU.
    pub fn can_ecu() -> Self {
        Self::new(SourceType::Ecu, BusType::Can)
    }

    /// Create a new SourceBlock for a CAN bus.
    pub fn can_bus() -> Self {
        Self::new(SourceType::Bus, BusType::Can)
    }

    /// Serialize the SourceBlock to bytes.
    pub fn to_bytes(&self) -> Result<alloc::vec::Vec<u8>> {
        use alloc::vec::Vec;

        let mut buffer = Vec::with_capacity(56);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (24 bytes = 3 * 8)
        buffer.extend_from_slice(&self.name_addr.to_le_bytes());
        buffer.extend_from_slice(&self.path_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Data section (8 bytes)
        buffer.push(self.source_type);
        buffer.push(self.bus_type);
        buffer.push(self.flags);
        buffer.extend_from_slice(&[0u8; 5]); // reserved padding

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
pub fn read_source_block(mmap: &[u8], address: u64) -> Result<SourceBlock> {
    let start = address as usize;
    let header = BlockHeader::from_bytes(&mmap[start..start + 24])?;
    // We know the total length from the header:
    let total_len = header.block_len as usize;
    let slice = &mmap[start..start + total_len];
    SourceBlock::from_bytes(slice)
}
