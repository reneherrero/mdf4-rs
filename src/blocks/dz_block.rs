//! DZ Block - Compressed Data Block
//!
//! The DZ block contains zlib-compressed data that represents another block type
//! (typically DT or SD). Decompression requires the `compression` feature.

use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse, read_u8, read_u32, read_u64, validate_buffer_size},
};
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

/// Compression algorithm used in DZ block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DzCompressionType {
    /// Deflate only (zlib).
    Deflate = 0,
    /// Transposition followed by deflate.
    TranspositionDeflate = 1,
}

impl DzCompressionType {
    /// Convert from raw u8 value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Deflate),
            1 => Some(Self::TranspositionDeflate),
            _ => None,
        }
    }
}

/// DZ Block - Zlib compressed data block.
///
/// Contains compressed data representing another block (DT, SD, etc.).
/// Use [`decompress()`](Self::decompress) to get the original uncompressed data.
///
/// # MDF4 Specification
///
/// The DZ block header (after the standard 24-byte block header):
/// - Offset 24-25: Original block type (2 bytes, e.g., "DT")
/// - Offset 26: Compression type (1 byte)
/// - Offset 27: Reserved (1 byte)
/// - Offset 28-31: Zip parameter (4 bytes, column count for transposition)
/// - Offset 32-39: Original data length (8 bytes)
/// - Offset 40-47: Compressed data length (8 bytes)
/// - Offset 48+: Compressed data
#[derive(Debug, Clone)]
pub struct DzBlock<'a> {
    /// Standard block header.
    pub header: BlockHeader,
    /// Original block type identifier (e.g., "DT", "SD").
    pub original_block_type: [u8; 2],
    /// Compression algorithm (0=deflate, 1=transposition+deflate).
    pub zip_type: DzCompressionType,
    /// For transposition: number of columns.
    pub zip_parameter: u32,
    /// Original uncompressed data size in bytes.
    pub original_data_length: u64,
    /// Compressed data size in bytes (should match data.len()).
    pub compressed_data_length: u64,
    /// Compressed data bytes (zlib/deflate format).
    pub data: &'a [u8],
}

/// DZ block header size (standard 24 + DZ-specific 24 = 48 bytes).
pub const DZ_HEADER_SIZE: usize = 48;

impl<'a> BlockParse<'a> for DzBlock<'a> {
    const ID: &'static str = "##DZ";

    fn from_bytes(bytes: &'a [u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        // DZ block header is 48 bytes total (24 standard + 24 DZ-specific)
        validate_buffer_size(bytes, DZ_HEADER_SIZE)?;

        let original_block_type = [bytes[24], bytes[25]];
        let zip_type_raw = read_u8(bytes, 26);
        let zip_type = DzCompressionType::from_u8(zip_type_raw).ok_or_else(|| {
            Error::BlockSerializationError(alloc::format!(
                "Unknown DZ compression type: {}",
                zip_type_raw
            ))
        })?;
        // bytes[27] is reserved
        let zip_parameter = read_u32(bytes, 28);
        let original_data_length = read_u64(bytes, 32);
        let compressed_data_length = read_u64(bytes, 40);

        let data_end = DZ_HEADER_SIZE + compressed_data_length as usize;
        validate_buffer_size(bytes, data_end)?;

        let data = &bytes[DZ_HEADER_SIZE..data_end];

        Ok(Self {
            header,
            original_block_type,
            zip_type,
            zip_parameter,
            original_data_length,
            compressed_data_length,
            data,
        })
    }
}

#[cfg(feature = "compression")]
impl DzBlock<'_> {
    /// Decompress the block data.
    ///
    /// Returns the original uncompressed bytes. For transposition+deflate,
    /// this also applies the inverse transposition.
    ///
    /// # Errors
    ///
    /// Returns an error if decompression fails or the decompressed size
    /// doesn't match the expected original size.
    pub fn decompress(&self) -> Result<Vec<u8>> {
        use miniz_oxide::inflate::decompress_to_vec_zlib;

        // First decompress the zlib data
        let decompressed = decompress_to_vec_zlib(self.data).map_err(|e| {
            Error::BlockSerializationError(alloc::format!("DZ decompression failed: {:?}", e))
        })?;

        // Validate decompressed size matches expected
        if decompressed.len() != self.original_data_length as usize {
            return Err(Error::BlockSerializationError(alloc::format!(
                "DZ decompressed size mismatch: expected {}, got {}",
                self.original_data_length,
                decompressed.len()
            )));
        }

        // Apply inverse transposition if needed
        match self.zip_type {
            DzCompressionType::Deflate => Ok(decompressed),
            DzCompressionType::TranspositionDeflate => self.inverse_transpose(decompressed),
        }
    }

    /// Apply inverse transposition to convert from column-major to row-major order.
    ///
    /// The MDF spec transposition stores data column-by-column to improve compression.
    /// This reverses it back to row-major (record-by-record) order.
    fn inverse_transpose(&self, data: Vec<u8>) -> Result<Vec<u8>> {
        let columns = self.zip_parameter as usize;
        if columns == 0 {
            return Err(Error::BlockSerializationError(
                "DZ transposition: zip_parameter (columns) cannot be 0".to_string(),
            ));
        }

        let total_bytes = data.len();
        let rows = total_bytes.div_ceil(columns);

        let mut result = vec![0u8; total_bytes];

        // Transposed data is stored column-by-column
        // We need to restore row-by-row order
        for col in 0..columns {
            for row in 0..rows {
                let src_idx = col * rows + row;
                let dst_idx = row * columns + col;

                if src_idx < total_bytes && dst_idx < total_bytes {
                    result[dst_idx] = data[src_idx];
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_dz_header(
        original_type: &[u8; 2],
        zip_type: u8,
        zip_param: u32,
        original_len: u64,
        compressed_len: u64,
    ) -> Vec<u8> {
        let total_len = DZ_HEADER_SIZE as u64 + compressed_len;

        let mut bytes = Vec::with_capacity(DZ_HEADER_SIZE);

        // Block header (24 bytes)
        bytes.extend_from_slice(b"##DZ");
        bytes.extend_from_slice(&0u32.to_le_bytes()); // reserved
        bytes.extend_from_slice(&total_len.to_le_bytes()); // length
        bytes.extend_from_slice(&0u64.to_le_bytes()); // link_count

        // DZ-specific header (24 bytes)
        bytes.extend_from_slice(original_type); // original block type
        bytes.push(zip_type); // zip_type
        bytes.push(0); // reserved
        bytes.extend_from_slice(&zip_param.to_le_bytes()); // zip_parameter
        bytes.extend_from_slice(&original_len.to_le_bytes()); // original size
        bytes.extend_from_slice(&compressed_len.to_le_bytes()); // compressed size

        bytes
    }

    #[test]
    fn parse_dz_header() {
        let compressed_data = [
            0x78, 0x9c, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
        ]; // minimal zlib
        let mut bytes = create_dz_header(b"DT", 0, 0, 0, compressed_data.len() as u64);
        bytes.extend_from_slice(&compressed_data);

        let dz = DzBlock::from_bytes(&bytes).unwrap();
        assert_eq!(dz.original_block_type, *b"DT");
        assert_eq!(dz.zip_type, DzCompressionType::Deflate);
        assert_eq!(dz.zip_parameter, 0);
        assert_eq!(dz.original_data_length, 0);
        assert_eq!(dz.compressed_data_length, compressed_data.len() as u64);
    }

    #[test]
    fn parse_dz_transposition_type() {
        let compressed_data = [
            0x78, 0x9c, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
        ];
        let mut bytes = create_dz_header(b"SD", 1, 8, 0, compressed_data.len() as u64);
        bytes.extend_from_slice(&compressed_data);

        let dz = DzBlock::from_bytes(&bytes).unwrap();
        assert_eq!(dz.original_block_type, *b"SD");
        assert_eq!(dz.zip_type, DzCompressionType::TranspositionDeflate);
        assert_eq!(dz.zip_parameter, 8);
    }

    #[test]
    fn invalid_compression_type() {
        let compressed_data = [
            0x78, 0x9c, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
        ];
        let mut bytes = create_dz_header(b"DT", 99, 0, 0, compressed_data.len() as u64);
        bytes.extend_from_slice(&compressed_data);

        let result = DzBlock::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[cfg(feature = "compression")]
    mod compression_tests {
        use super::*;
        use miniz_oxide::deflate::compress_to_vec_zlib;

        #[test]
        fn decompress_deflate() {
            let original_data = b"Hello, MDF4 world! This is test data for compression.";
            let compressed = compress_to_vec_zlib(original_data, 6);

            let mut bytes = create_dz_header(
                b"DT",
                0,
                0,
                original_data.len() as u64,
                compressed.len() as u64,
            );
            bytes.extend_from_slice(&compressed);

            let dz = DzBlock::from_bytes(&bytes).unwrap();
            let decompressed = dz.decompress().unwrap();
            assert_eq!(decompressed.as_slice(), original_data);
        }

        #[test]
        fn decompress_transposition() {
            // Create data: 4 columns, 3 rows
            let original_data: Vec<u8> = vec![
                1, 2, 3, 4, // row 0
                5, 6, 7, 8, // row 1
                9, 10, 11, 12, // row 2
            ];

            // Transpose to column-major for compression
            let columns = 4usize;
            let rows = 3usize;
            let mut transposed = vec![0u8; 12];
            for col in 0..columns {
                for row in 0..rows {
                    transposed[col * rows + row] = original_data[row * columns + col];
                }
            }

            let compressed = compress_to_vec_zlib(&transposed, 6);

            let mut bytes = create_dz_header(
                b"DT",
                1,
                columns as u32,
                original_data.len() as u64,
                compressed.len() as u64,
            );
            bytes.extend_from_slice(&compressed);

            let dz = DzBlock::from_bytes(&bytes).unwrap();
            let decompressed = dz.decompress().unwrap();
            assert_eq!(decompressed, original_data);
        }

        #[test]
        fn decompress_size_mismatch() {
            let original_data = b"test";
            let compressed = compress_to_vec_zlib(original_data, 6);

            // Claim original size is different
            let mut bytes = create_dz_header(b"DT", 0, 0, 100, compressed.len() as u64);
            bytes.extend_from_slice(&compressed);

            let dz = DzBlock::from_bytes(&bytes).unwrap();
            let result = dz.decompress();
            assert!(result.is_err());
        }
    }
}
