//! File History Block (##FH) - tracks file modification history.
//!
//! Each FH block represents a single entry in the file's modification history,
//! recording when and by what tool the file was created or modified.

use super::FH_BLOCK_SIZE;
use crate::{
    Result,
    blocks::common::{
        BlockHeader, BlockParse, debug_assert_aligned, read_u8, read_u64, validate_buffer_size,
    },
};
use alloc::string::String;
use alloc::vec::Vec;

/// File History Block (##FH) - records file modification history.
///
/// File history blocks form a linked list starting from the header block's
/// `file_history_addr`. Each entry records when and by what tool the file
/// was created or modified.
///
/// # MDF4 Specification
///
/// The FH block has:
/// - 2 links: next FH block, comment MD block
/// - Timestamp of the modification
/// - Timezone information
/// - Time quality flags
#[derive(Debug, Clone)]
pub struct FileHistoryBlock {
    /// Standard block header.
    pub header: BlockHeader,
    /// Link to next file history block (0 = end of list).
    pub next_fh_addr: u64,
    /// Link to MD block containing tool info and comment (XML format).
    pub comment_addr: u64,
    /// Absolute time of modification in nanoseconds since Jan 1, 1970 (UTC).
    pub time_ns: u64,
    /// Timezone offset from UTC in minutes.
    pub tz_offset_min: i16,
    /// Daylight saving time offset in minutes.
    pub dst_offset_min: i16,
    /// Time flags:
    /// - Bit 0: Local time (vs UTC)
    /// - Bit 1: Time offsets are valid
    pub time_flags: u8,
}

impl BlockParse<'_> for FileHistoryBlock {
    const ID: &'static str = "##FH";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        validate_buffer_size(bytes, FH_BLOCK_SIZE)?;

        Ok(Self {
            header,
            // Links section (2 x u64 = 16 bytes at offset 24)
            next_fh_addr: read_u64(bytes, 24),
            comment_addr: read_u64(bytes, 32),
            // Data section at offset 40
            time_ns: read_u64(bytes, 40),
            tz_offset_min: i16::from_le_bytes([bytes[48], bytes[49]]),
            dst_offset_min: i16::from_le_bytes([bytes[50], bytes[51]]),
            time_flags: read_u8(bytes, 52),
            // bytes 53-55: reserved
        })
    }
}

impl FileHistoryBlock {
    /// Creates a new FileHistoryBlock with the given timestamp.
    ///
    /// # Arguments
    /// * `time_ns` - Timestamp in nanoseconds since Jan 1, 1970 (UTC)
    pub fn new(time_ns: u64) -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##FH"),
                reserved: 0,
                length: FH_BLOCK_SIZE as u64,
                link_count: 2,
            },
            next_fh_addr: 0,
            comment_addr: 0,
            time_ns,
            tz_offset_min: 0,
            dst_offset_min: 0,
            time_flags: 0,
        }
    }

    /// Creates a new FileHistoryBlock with the current system time.
    #[cfg(feature = "std")]
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self::new(time_ns)
    }

    /// Serializes the FileHistoryBlock to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(FH_BLOCK_SIZE);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (16 bytes)
        buffer.extend_from_slice(&self.next_fh_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Data section (16 bytes)
        buffer.extend_from_slice(&self.time_ns.to_le_bytes());
        buffer.extend_from_slice(&self.tz_offset_min.to_le_bytes());
        buffer.extend_from_slice(&self.dst_offset_min.to_le_bytes());
        buffer.push(self.time_flags);
        buffer.extend_from_slice(&[0u8; 3]); // reserved

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }
}

impl Default for FileHistoryBlock {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let original = FileHistoryBlock {
            header: BlockHeader {
                id: String::from("##FH"),
                reserved: 0,
                length: FH_BLOCK_SIZE as u64,
                link_count: 2,
            },
            next_fh_addr: 0x1000,
            comment_addr: 0x2000,
            time_ns: 1704067200_000_000_000, // 2024-01-01 00:00:00 UTC
            tz_offset_min: 60,
            dst_offset_min: 60,
            time_flags: 0x03,
        };

        let bytes = original.to_bytes().unwrap();
        assert_eq!(bytes.len(), FH_BLOCK_SIZE);

        let parsed = FileHistoryBlock::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.next_fh_addr, original.next_fh_addr);
        assert_eq!(parsed.comment_addr, original.comment_addr);
        assert_eq!(parsed.time_ns, original.time_ns);
        assert_eq!(parsed.tz_offset_min, original.tz_offset_min);
        assert_eq!(parsed.dst_offset_min, original.dst_offset_min);
        assert_eq!(parsed.time_flags, original.time_flags);
    }

    #[test]
    fn default_values() {
        let block = FileHistoryBlock::default();
        assert_eq!(block.header.id, "##FH");
        assert_eq!(block.header.link_count, 2);
        assert_eq!(block.next_fh_addr, 0);
        assert_eq!(block.comment_addr, 0);
    }
}
