// src/blocks/header_block.rs
use super::HD_BLOCK_SIZE;
use crate::{
    Result,
    blocks::common::{
        BlockHeader, BlockParse, debug_assert_aligned, read_u8, read_u64, validate_block_id,
        validate_block_length, validate_buffer_size,
    },
};
use alloc::string::String;
use alloc::vec::Vec;

/// Header Block (##HD) - file-level metadata and links to data groups.
///
/// The header block is the entry point for all measurement data in an MDF file.
/// It contains links to data groups, file history, events, and attachments.
#[derive(Debug, Clone)]
pub struct HeaderBlock {
    pub header: BlockHeader,
    /// Link to first data group block.
    pub first_dg_addr: u64,
    /// Link to file history block.
    pub file_history_addr: u64,
    /// Link to channel hierarchy tree block.
    pub channel_tree_addr: u64,
    /// Link to first attachment block.
    pub first_attachment_addr: u64,
    /// Link to first event block.
    pub first_event_addr: u64,
    /// Link to comment text/metadata block.
    pub comment_addr: u64,
    /// Absolute start time in nanoseconds since Jan 1, 1970 (UTC).
    pub start_time_ns: u64,
    /// Timezone offset in minutes from UTC.
    pub tz_offset_min: i16,
    /// Daylight saving time offset in minutes.
    pub dst_offset_min: i16,
    /// Time flags (bit 0: local time, bit 1: offsets valid).
    pub time_flags: u8,
    /// Time quality class (0=unknown, 10=external sync, 16=local PC).
    pub time_quality: u8,
    /// Header flags.
    pub flags: u8,
    /// Start angle in radians (for angular synchronization).
    pub start_angle_rad: f64,
    /// Start distance in meters (for distance synchronization).
    pub start_distance_m: f64,
}

impl HeaderBlock {
    /// Serializes the HeaderBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        validate_block_id(&self.header, "##HD")?;
        validate_block_length(&self.header, HD_BLOCK_SIZE as u64)?;

        let mut buffer = Vec::with_capacity(HD_BLOCK_SIZE);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (48 bytes)
        buffer.extend_from_slice(&self.first_dg_addr.to_le_bytes());
        buffer.extend_from_slice(&self.file_history_addr.to_le_bytes());
        buffer.extend_from_slice(&self.channel_tree_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_attachment_addr.to_le_bytes());
        buffer.extend_from_slice(&self.first_event_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Time section (16 bytes)
        buffer.extend_from_slice(&self.start_time_ns.to_le_bytes());
        buffer.extend_from_slice(&self.tz_offset_min.to_le_bytes());
        buffer.extend_from_slice(&self.dst_offset_min.to_le_bytes());
        buffer.push(self.time_flags);
        buffer.push(self.time_quality);
        buffer.push(self.flags);
        buffer.push(0); // reserved

        // Angle/Distance section (16 bytes) - stored as raw u64 bit patterns
        buffer.extend_from_slice(&self.start_angle_rad.to_le_bytes());
        buffer.extend_from_slice(&self.start_distance_m.to_le_bytes());

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }
}

impl BlockParse<'_> for HeaderBlock {
    const ID: &'static str = "##HD";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        validate_buffer_size(bytes, HD_BLOCK_SIZE)?;

        // Read i16 values using helper pattern
        let tz_offset_min = i16::from_le_bytes([bytes[80], bytes[81]]);
        let dst_offset_min = i16::from_le_bytes([bytes[82], bytes[83]]);

        Ok(Self {
            header,
            // Links section (6 x u64 = 48 bytes at offset 24)
            first_dg_addr: read_u64(bytes, 24),
            file_history_addr: read_u64(bytes, 32),
            channel_tree_addr: read_u64(bytes, 40),
            first_attachment_addr: read_u64(bytes, 48),
            first_event_addr: read_u64(bytes, 56),
            comment_addr: read_u64(bytes, 64),
            // Time section at offset 72
            start_time_ns: read_u64(bytes, 72),
            tz_offset_min,
            dst_offset_min,
            time_flags: read_u8(bytes, 84),
            time_quality: read_u8(bytes, 85),
            flags: read_u8(bytes, 86),
            // byte 87: reserved (skipped)
            // Angle/Distance section at offset 88 (stored as f64)
            start_angle_rad: f64::from_le_bytes([
                bytes[88], bytes[89], bytes[90], bytes[91], bytes[92], bytes[93], bytes[94],
                bytes[95],
            ]),
            start_distance_m: f64::from_le_bytes([
                bytes[96], bytes[97], bytes[98], bytes[99], bytes[100], bytes[101], bytes[102],
                bytes[103],
            ]),
        })
    }
}

impl Default for HeaderBlock {
    fn default() -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##HD"),
                reserved: 0,
                length: HD_BLOCK_SIZE as u64,
                link_count: 6,
            },
            first_dg_addr: 0,
            file_history_addr: 0,
            channel_tree_addr: 0,
            first_attachment_addr: 0,
            first_event_addr: 0,
            comment_addr: 0,
            start_time_ns: 0,
            tz_offset_min: 0,
            dst_offset_min: 0,
            time_flags: 0,
            time_quality: 0,
            flags: 0,
            start_angle_rad: 0.0,
            start_distance_m: 0.0,
        }
    }
}
