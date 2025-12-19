// src/blocks/header_block.rs
use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse},
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct HeaderBlock {
    pub header: BlockHeader,        // Common header from the first 24 bytes
    pub first_dg_addr: u64,         // bytes[24..32]
    pub file_history_addr: u64,     // bytes[32..40]
    pub channel_tree_addr: u64,     // bytes[40..48]
    pub first_attachment_addr: u64, // bytes[48..56]
    pub first_event_addr: u64,      // bytes[56..64]
    pub comment_addr: u64,          // bytes[64..72]
    pub abs_time: u64,              // bytes[72..80]
    pub tz_offset: i16,             // bytes[80..82]
    pub daylight_save_time: i16,    // bytes[82..84]
    pub time_flags: u8,             // byte[84]
    pub time_quality: u8,           // byte[85]
    pub flags: u8,                  // byte[86]
    pub reserved1: u8,              // byte[87]
    pub start_angle: u64,           // bytes[88..96]
    pub start_distance: u64,        // bytes[96..104]
}

impl HeaderBlock {
    /// Serializes the HeaderBlock to bytes according to MDF 4.1 specification.
    ///
    /// # Structure (104 bytes total):
    /// - BlockHeader (24 bytes): Standard block header with id "##HD"
    /// - Link section (48 bytes): Six 8-byte links to other blocks
    ///   * first_dg_addr: Link to first Data Group block
    ///   * file_history_addr: Link to file history block
    ///   * channel_tree_addr: Link to channel hierarchy tree
    ///   * first_attachment_addr: Link to first attachment block
    ///   * first_event_addr: Link to first event block
    ///   * comment_addr: Link to comment block
    /// - Time section (16 bytes): Timestamp and timezone information
    /// - Angle/Distance section (16 bytes): Start values for angle and distance
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized header block
    /// - `Err(MdfError)` if serialization fails
    ///
    /// # Important
    /// - The header must have id="##HD" and block_len=104
    /// - Links will typically be updated after all blocks are written
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Validate the header before serializing
        if self.header.id != "##HD" {
            return Err(Error::BlockSerializationError(format!(
                "HeaderBlock must have ID '##HD', found '{}'",
                self.header.id
            )));
        }

        if self.header.block_len != 104 {
            return Err(Error::BlockSerializationError(format!(
                "HeaderBlock must have block_len=104, found {}",
                self.header.block_len
            )));
        }

        // Create a buffer with exact capacity for efficiency
        let mut buffer = Vec::with_capacity(104);

        // 1. Write the block header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // 2. Write the six link addresses (48 bytes total)
        // Each is a u64 in little-endian format
        buffer.extend_from_slice(&self.first_dg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.file_history_addr.to_le_bytes());
        buffer.extend_from_slice(&self.channel_tree_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_attachment_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_event_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // 3. Write the time section (16 bytes)
        buffer.extend_from_slice(&self.abs_time.to_le_bytes()); // 8 bytes - absolute time
        buffer.extend_from_slice(&self.tz_offset.to_le_bytes()); // 2 bytes - timezone offset
        buffer.extend_from_slice(&self.daylight_save_time.to_le_bytes()); // 2 bytes - DST offset
        buffer.push(self.time_flags); // 1 byte - time flags
        buffer.push(self.time_quality); // 1 byte - time quality
        buffer.push(self.flags); // 1 byte - general flags
        buffer.push(self.reserved1); // 1 byte - reserved

        // 4. Write the angle/distance section (16 bytes)
        buffer.extend_from_slice(&self.start_angle.to_le_bytes()); // 8 bytes - start angle
        buffer.extend_from_slice(&self.start_distance.to_le_bytes()); // 8 bytes - start distance

        // Verify the buffer is exactly 104 bytes
        if buffer.len() != 104 {
            return Err(Error::BlockSerializationError(format!(
                "HeaderBlock must be exactly 104 bytes, got {}",
                buffer.len()
            )));
        }

        // Ensure 8-byte alignment (should always be true since 104 is divisible by 8)
        debug_assert_eq!(
            buffer.len() % 8,
            0,
            "HeaderBlock size is not 8-byte aligned"
        );

        Ok(buffer)
    }
}

impl BlockParse<'_> for HeaderBlock {
    const ID: &'static str = "##HD";
    /// Creates a HeaderBlock from a 104-byte slice.
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
            first_dg_addr: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            file_history_addr: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            channel_tree_addr: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            first_attachment_addr: u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
            first_event_addr: u64::from_le_bytes(bytes[56..64].try_into().unwrap()),
            comment_addr: u64::from_le_bytes(bytes[64..72].try_into().unwrap()),
            abs_time: u64::from_le_bytes(bytes[72..80].try_into().unwrap()),
            tz_offset: i16::from_le_bytes(bytes[80..82].try_into().unwrap()),
            daylight_save_time: i16::from_le_bytes(bytes[82..84].try_into().unwrap()),
            time_flags: bytes[84],
            time_quality: bytes[85],
            flags: bytes[86],
            reserved1: bytes[87],
            start_angle: u64::from_le_bytes(bytes[88..96].try_into().unwrap()),
            start_distance: u64::from_le_bytes(bytes[96..104].try_into().unwrap()),
        })
    }
}

impl Default for HeaderBlock {
    fn default() -> Self {
        let header = BlockHeader {
            id: String::from("##HD"),
            reserved0: 0,
            block_len: 104,
            links_nr: 6,
        };

        HeaderBlock {
            header,
            first_dg_addr: 0,
            file_history_addr: 0,
            channel_tree_addr: 0,
            first_attachment_addr: 0,
            first_event_addr: 0,
            comment_addr: 0,
            abs_time: 2 * 3600 * 1000000000,
            tz_offset: 0,
            daylight_save_time: 0,
            time_flags: 0,
            time_quality: 0,
            flags: 0,
            reserved1: 0,
            start_angle: 0,
            start_distance: 0,
        }
    }
}
