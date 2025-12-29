//! Attachment Block (##AT) - embedded or referenced files.
//!
//! AT blocks store files embedded within the MDF file or references to external
//! files. Common uses include storing calibration data, documentation, or
//! configuration files alongside measurement data.

use super::AT_BLOCK_SIZE;
use crate::{
    Result,
    blocks::common::{BlockHeader, BlockParse, read_u16, read_u64, validate_buffer_size},
};
use alloc::string::String;
use alloc::vec::Vec;

/// Attachment flags bit definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttachmentFlags(u16);

impl AttachmentFlags {
    /// Bit 0: Data is embedded in this block (vs external file reference).
    pub const EMBEDDED: u16 = 0x0001;
    /// Bit 1: Embedded data is compressed (zlib).
    pub const COMPRESSED: u16 = 0x0002;
    /// Bit 2: MD5 checksum is valid.
    pub const MD5_VALID: u16 = 0x0004;

    /// Create flags from raw u16 value.
    pub fn from_u16(value: u16) -> Self {
        Self(value)
    }

    /// Get raw u16 value.
    pub fn as_u16(self) -> u16 {
        self.0
    }

    /// Check if data is embedded in this block.
    pub fn is_embedded(self) -> bool {
        self.0 & Self::EMBEDDED != 0
    }

    /// Check if embedded data is compressed.
    pub fn is_compressed(self) -> bool {
        self.0 & Self::COMPRESSED != 0
    }

    /// Check if MD5 checksum is valid.
    pub fn is_md5_valid(self) -> bool {
        self.0 & Self::MD5_VALID != 0
    }

    /// Create flags for embedded data.
    pub fn embedded() -> Self {
        Self(Self::EMBEDDED)
    }

    /// Create flags for embedded compressed data.
    pub fn embedded_compressed() -> Self {
        Self(Self::EMBEDDED | Self::COMPRESSED)
    }

    /// Create flags for external file reference.
    pub fn external() -> Self {
        Self(0)
    }
}

/// Attachment Block (##AT) - embedded or external file.
///
/// Stores files within the MDF or references to external files. Attachments
/// can be compressed using zlib and include MD5 checksums for verification.
///
/// # MDF4 Specification
///
/// The AT block has:
/// - 4 links: next AT, filename TX, MIME type TX, comment MD
/// - Flags indicating embedded/external and compression
/// - MD5 checksum of original (uncompressed) data
/// - Original and embedded data sizes
/// - Embedded data bytes (if embedded flag is set)
#[derive(Debug, Clone)]
pub struct AttachmentBlock<'a> {
    /// Standard block header.
    pub header: BlockHeader,

    // === Links (4) ===
    /// Link to next attachment block (0 = end of list).
    pub next_at_addr: u64,
    /// Link to TX block containing filename.
    pub filename_addr: u64,
    /// Link to TX block containing MIME type (e.g., "application/pdf").
    pub mimetype_addr: u64,
    /// Link to MD block containing comment.
    pub comment_addr: u64,

    // === Data Section ===
    /// Attachment flags.
    pub flags: AttachmentFlags,
    /// Index of creator FH block (0 = first FH).
    pub creator_index: u16,
    /// MD5 checksum of original (uncompressed) data (16 bytes).
    pub md5_checksum: [u8; 16],
    /// Original uncompressed data size in bytes.
    pub original_size: u64,
    /// Embedded data size in bytes (0 if external reference).
    pub embedded_size: u64,
    /// Embedded data bytes (empty if external reference).
    pub embedded_data: &'a [u8],
}

/// AT block header size (before embedded data).
/// 24 (header) + 32 (links) + 40 (fixed data) = 96 bytes.
pub const AT_HEADER_SIZE: usize = 96;

impl<'a> BlockParse<'a> for AttachmentBlock<'a> {
    const ID: &'static str = "##AT";

    fn from_bytes(bytes: &'a [u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        validate_buffer_size(bytes, AT_HEADER_SIZE)?;

        // Links section (4 x 8 = 32 bytes at offset 24)
        let next_at_addr = read_u64(bytes, 24);
        let filename_addr = read_u64(bytes, 32);
        let mimetype_addr = read_u64(bytes, 40);
        let comment_addr = read_u64(bytes, 48);

        // Data section (at offset 56)
        let flags = AttachmentFlags::from_u16(read_u16(bytes, 56));
        let creator_index = read_u16(bytes, 58);
        // bytes 60-63: reserved

        // MD5 checksum (16 bytes at offset 64)
        let mut md5_checksum = [0u8; 16];
        md5_checksum.copy_from_slice(&bytes[64..80]);

        let original_size = read_u64(bytes, 80);
        let embedded_size = read_u64(bytes, 88);

        // Embedded data starts at offset 96
        let embedded_data = if flags.is_embedded() && embedded_size > 0 {
            let data_end = AT_HEADER_SIZE + embedded_size as usize;
            validate_buffer_size(bytes, data_end)?;
            &bytes[AT_HEADER_SIZE..data_end]
        } else {
            &bytes[0..0] // Empty slice
        };

        Ok(Self {
            header,
            next_at_addr,
            filename_addr,
            mimetype_addr,
            comment_addr,
            flags,
            creator_index,
            md5_checksum,
            original_size,
            embedded_size,
            embedded_data,
        })
    }
}

impl AttachmentBlock<'_> {
    /// Creates a new AttachmentBlock for an external file reference.
    ///
    /// # Arguments
    /// * `original_size` - Size of the external file in bytes
    pub fn external(original_size: u64) -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##AT"),
                reserved: 0,
                length: AT_BLOCK_SIZE as u64,
                link_count: 4,
            },
            next_at_addr: 0,
            filename_addr: 0,
            mimetype_addr: 0,
            comment_addr: 0,
            flags: AttachmentFlags::external(),
            creator_index: 0,
            md5_checksum: [0u8; 16],
            original_size,
            embedded_size: 0,
            embedded_data: &[],
        }
    }

    /// Creates a new AttachmentBlock with embedded data.
    ///
    /// # Arguments
    /// * `data` - The data to embed
    pub fn embedded(data: &[u8]) -> AttachmentBlock<'_> {
        AttachmentBlock {
            header: BlockHeader {
                id: String::from("##AT"),
                reserved: 0,
                length: (AT_HEADER_SIZE + data.len()) as u64,
                link_count: 4,
            },
            next_at_addr: 0,
            filename_addr: 0,
            mimetype_addr: 0,
            comment_addr: 0,
            flags: AttachmentFlags::embedded(),
            creator_index: 0,
            md5_checksum: [0u8; 16],
            original_size: data.len() as u64,
            embedded_size: data.len() as u64,
            embedded_data: data,
        }
    }

    /// Returns the embedded data if available.
    ///
    /// Returns `None` for compressed or external attachments.
    /// For compressed attachments, enable the `compression` feature and use
    /// the `decompress()` method instead.
    pub fn data(&self) -> Option<&[u8]> {
        if self.flags.is_embedded() && !self.flags.is_compressed() {
            Some(self.embedded_data)
        } else {
            None
        }
    }

    /// Decompresses embedded data if compressed.
    ///
    /// Returns the original uncompressed data. For uncompressed embedded data,
    /// this returns a copy of the data. For external references, returns None.
    #[cfg(feature = "compression")]
    pub fn decompress(&self) -> Result<Option<Vec<u8>>> {
        use crate::Error;
        use miniz_oxide::inflate::decompress_to_vec_zlib;

        if !self.flags.is_embedded() {
            return Ok(None);
        }

        if self.flags.is_compressed() {
            let decompressed = decompress_to_vec_zlib(self.embedded_data).map_err(|e| {
                Error::BlockSerializationError(alloc::format!("AT decompression failed: {:?}", e))
            })?;

            if decompressed.len() != self.original_size as usize {
                return Err(Error::BlockSerializationError(alloc::format!(
                    "AT decompressed size mismatch: expected {}, got {}",
                    self.original_size,
                    decompressed.len()
                )));
            }

            Ok(Some(decompressed))
        } else {
            Ok(Some(self.embedded_data.to_vec()))
        }
    }

    /// Serializes the AttachmentBlock to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let total_size = AT_HEADER_SIZE + self.embedded_data.len();
        let mut buffer = Vec::with_capacity(total_size);

        // Update header with correct size
        let mut header = self.header.clone();
        header.length = total_size as u64;
        buffer.extend_from_slice(&header.to_bytes()?);

        // Links (32 bytes)
        buffer.extend_from_slice(&self.next_at_addr.to_le_bytes());
        buffer.extend_from_slice(&self.filename_addr.to_le_bytes());
        buffer.extend_from_slice(&self.mimetype_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Data section
        buffer.extend_from_slice(&self.flags.as_u16().to_le_bytes());
        buffer.extend_from_slice(&self.creator_index.to_le_bytes());
        buffer.extend_from_slice(&[0u8; 4]); // reserved
        buffer.extend_from_slice(&self.md5_checksum);
        buffer.extend_from_slice(&self.original_size.to_le_bytes());
        buffer.extend_from_slice(&self.embedded_size.to_le_bytes());

        // Embedded data
        buffer.extend_from_slice(self.embedded_data);

        Ok(buffer)
    }
}

impl Default for AttachmentBlock<'_> {
    fn default() -> Self {
        Self::external(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_at_bytes(
        flags: u16,
        original_size: u64,
        embedded_size: u64,
        embedded_data: &[u8],
    ) -> Vec<u8> {
        let total_len = AT_HEADER_SIZE as u64 + embedded_data.len() as u64;

        let mut bytes = Vec::with_capacity(total_len as usize);

        // Block header (24 bytes)
        bytes.extend_from_slice(b"##AT");
        bytes.extend_from_slice(&0u32.to_le_bytes()); // reserved
        bytes.extend_from_slice(&total_len.to_le_bytes()); // length
        bytes.extend_from_slice(&4u64.to_le_bytes()); // link_count

        // Links (32 bytes)
        bytes.extend_from_slice(&0u64.to_le_bytes()); // next_at
        bytes.extend_from_slice(&0x1000u64.to_le_bytes()); // filename
        bytes.extend_from_slice(&0x2000u64.to_le_bytes()); // mimetype
        bytes.extend_from_slice(&0u64.to_le_bytes()); // comment

        // Data section
        bytes.extend_from_slice(&flags.to_le_bytes()); // flags
        bytes.extend_from_slice(&0u16.to_le_bytes()); // creator_index
        bytes.extend_from_slice(&[0u8; 4]); // reserved
        bytes.extend_from_slice(&[0u8; 16]); // md5_checksum
        bytes.extend_from_slice(&original_size.to_le_bytes());
        bytes.extend_from_slice(&embedded_size.to_le_bytes());

        // Embedded data
        bytes.extend_from_slice(embedded_data);

        bytes
    }

    #[test]
    fn parse_external_reference() {
        let bytes = create_at_bytes(0, 1024, 0, &[]);

        let at = AttachmentBlock::from_bytes(&bytes).unwrap();
        assert!(!at.flags.is_embedded());
        assert!(!at.flags.is_compressed());
        assert_eq!(at.original_size, 1024);
        assert_eq!(at.embedded_size, 0);
        assert!(at.embedded_data.is_empty());
        assert_eq!(at.filename_addr, 0x1000);
        assert_eq!(at.mimetype_addr, 0x2000);
    }

    #[test]
    fn parse_embedded_uncompressed() {
        let data = b"Hello, MDF4!";
        let bytes = create_at_bytes(
            AttachmentFlags::EMBEDDED,
            data.len() as u64,
            data.len() as u64,
            data,
        );

        let at = AttachmentBlock::from_bytes(&bytes).unwrap();
        assert!(at.flags.is_embedded());
        assert!(!at.flags.is_compressed());
        assert_eq!(at.original_size, data.len() as u64);
        assert_eq!(at.embedded_size, data.len() as u64);
        assert_eq!(at.embedded_data, data);
        assert_eq!(at.data(), Some(data.as_slice()));
    }

    #[test]
    fn flags_operations() {
        let embedded = AttachmentFlags::embedded();
        assert!(embedded.is_embedded());
        assert!(!embedded.is_compressed());

        let compressed = AttachmentFlags::embedded_compressed();
        assert!(compressed.is_embedded());
        assert!(compressed.is_compressed());

        let external = AttachmentFlags::external();
        assert!(!external.is_embedded());

        let with_md5 =
            AttachmentFlags::from_u16(AttachmentFlags::EMBEDDED | AttachmentFlags::MD5_VALID);
        assert!(with_md5.is_embedded());
        assert!(with_md5.is_md5_valid());
    }

    #[test]
    fn roundtrip_external() {
        let original = AttachmentBlock::external(2048);
        let bytes = original.to_bytes().unwrap();
        let parsed = AttachmentBlock::from_bytes(&bytes).unwrap();

        assert!(!parsed.flags.is_embedded());
        assert_eq!(parsed.original_size, 2048);
        assert_eq!(parsed.embedded_size, 0);
    }

    #[test]
    fn roundtrip_embedded() {
        let data = b"Test attachment data for roundtrip";
        let original = AttachmentBlock::embedded(data);
        let bytes = original.to_bytes().unwrap();
        let parsed = AttachmentBlock::from_bytes(&bytes).unwrap();

        assert!(parsed.flags.is_embedded());
        assert_eq!(parsed.original_size, data.len() as u64);
        assert_eq!(parsed.embedded_data, data);
    }

    #[cfg(feature = "compression")]
    mod compression_tests {
        use super::*;
        use miniz_oxide::deflate::compress_to_vec_zlib;

        #[test]
        fn decompress_embedded() {
            let original_data =
                b"This is test data that will be compressed for the attachment block.";
            let compressed = compress_to_vec_zlib(original_data, 6);

            let bytes = create_at_bytes(
                AttachmentFlags::EMBEDDED | AttachmentFlags::COMPRESSED,
                original_data.len() as u64,
                compressed.len() as u64,
                &compressed,
            );

            let at = AttachmentBlock::from_bytes(&bytes).unwrap();
            assert!(at.flags.is_embedded());
            assert!(at.flags.is_compressed());

            let decompressed = at.decompress().unwrap().unwrap();
            assert_eq!(decompressed.as_slice(), original_data);
        }

        #[test]
        fn decompress_uncompressed_returns_copy() {
            let data = b"Uncompressed data";
            let bytes = create_at_bytes(
                AttachmentFlags::EMBEDDED,
                data.len() as u64,
                data.len() as u64,
                data,
            );

            let at = AttachmentBlock::from_bytes(&bytes).unwrap();
            let result = at.decompress().unwrap().unwrap();
            assert_eq!(result.as_slice(), data);
        }

        #[test]
        fn decompress_external_returns_none() {
            let bytes = create_at_bytes(0, 1024, 0, &[]);

            let at = AttachmentBlock::from_bytes(&bytes).unwrap();
            let result = at.decompress().unwrap();
            assert!(result.is_none());
        }
    }
}
