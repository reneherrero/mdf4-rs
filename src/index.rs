//! MDF File Indexing System
//!
//! This module provides functionality to create lightweight indexes of MDF files
//! that can be serialized to JSON and used later to read specific channel data
//! without parsing the entire file structure.
//!
//! # Performance-Optimized Reading
//!
//! The index system enables efficient reading of large MDF files:
//!
//! ```no_run
//! use mdf4_rs::{MdfIndex, FileRangeReader, Result};
//!
//! fn read_efficiently() -> Result<()> {
//!     // Option 1: Create index with streaming (minimal memory)
//!     let index = MdfIndex::from_file_streaming("large_file.mf4")?;
//!
//!     // Save for later use
//!     index.save_to_file("large_file.index")?;
//!
//!     // Option 2: Load pre-built index (instant)
//!     let index = MdfIndex::load_from_file("large_file.index")?;
//!
//!     // Read only the channel you need
//!     let mut reader = FileRangeReader::new("large_file.mf4")?;
//!     let values = index.read_channel_values_by_name("Temperature", &mut reader)?;
//!
//!     Ok(())
//! }
//! ```

use crate::{
    Error, MDF, Result,
    blocks::{
        BlockHeader, BlockParse, ChannelBlock, ChannelGroupBlock, ConversionBlock, ConversionType,
        DataGroupBlock, DataListBlock, DataType, HeaderBlock, IdentificationBlock, TextBlock,
    },
    parsing::decoder::{DecodedValue, decode_channel_value_with_validity},
};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

/// Represents the location and metadata of data blocks in the file
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DataBlockInfo {
    /// File offset where the data block starts
    pub file_offset: u64,
    /// Size of the data block in bytes
    pub size: u64,
    /// Whether this is a compressed block (DZ)
    pub is_compressed: bool,
}

/// Channel metadata needed for decoding values
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IndexedChannel {
    /// Channel name
    pub name: Option<String>,
    /// Physical unit
    pub unit: Option<String>,
    /// Data type of the channel
    pub data_type: DataType,
    /// Byte offset within each record
    pub byte_offset: u32,
    /// Bit offset within the byte
    pub bit_offset: u8,
    /// Number of bits for this channel
    pub bit_count: u32,
    /// Channel type (0=data, 1=VLSD, 2=master, etc.)
    pub channel_type: u8,
    /// Channel flags (includes invalidation bit flags)
    pub flags: u32,
    /// Position of invalidation bit within invalidation bytes
    pub pos_invalidation_bit: u32,
    /// Conversion block for unit conversion (if any)
    pub conversion: Option<ConversionBlock>,
    /// For VLSD channels: address of signal data blocks
    pub vlsd_data_address: Option<u64>,
}

/// Channel group metadata and layout information
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IndexedChannelGroup {
    /// Group name
    pub name: Option<String>,
    /// Comment
    pub comment: Option<String>,
    /// Size of record ID in bytes
    pub record_id_len: u8,
    /// Total size of each record in bytes (excluding record ID and invalidation bytes)
    pub record_size: u32,
    /// Number of invalidation bytes per record
    pub invalidation_bytes: u32,
    /// Number of records in this group
    pub record_count: u64,
    /// Channels in this group
    pub channels: Vec<IndexedChannel>,
    /// Data block locations for this channel group
    pub data_blocks: Vec<DataBlockInfo>,
}

/// Complete MDF file index
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MdfIndex {
    /// File size for validation
    pub file_size: u64,
    /// Channel groups in the file
    pub channel_groups: Vec<IndexedChannelGroup>,
}

/// Trait for reading byte ranges from different sources (files, HTTP, etc.)
pub trait ByteRangeReader {
    type Error;

    /// Read bytes from the specified range
    /// Returns the requested bytes or an error
    fn read_range(
        &mut self,
        offset: u64,
        length: u64,
    ) -> core::result::Result<Vec<u8>, Self::Error>;
}

/// Local file reader implementation
pub struct FileRangeReader {
    file: std::fs::File,
}

impl FileRangeReader {
    pub fn new(file_path: &str) -> Result<Self> {
        let file = std::fs::File::open(file_path).map_err(Error::IOError)?;
        Ok(Self { file })
    }
}

impl ByteRangeReader for FileRangeReader {
    type Error = Error;

    fn read_range(
        &mut self,
        offset: u64,
        length: u64,
    ) -> core::result::Result<Vec<u8>, Self::Error> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(Error::IOError)?;

        let mut buffer = vec![0u8; length as usize];
        self.file.read_exact(&mut buffer).map_err(Error::IOError)?;

        Ok(buffer)
    }
}

/// Buffered file reader with read-ahead caching for better I/O performance.
///
/// This reader maintains an internal buffer and prefetches data to minimize
/// system calls when reading many small ranges sequentially.
pub struct BufferedRangeReader {
    file: std::fs::File,
    buffer: Vec<u8>,
    buffer_start: u64,
    buffer_end: u64,
    buffer_capacity: usize,
}

impl BufferedRangeReader {
    /// Create a new buffered reader with the default buffer size (64 KB).
    pub fn new(file_path: &str) -> Result<Self> {
        Self::with_capacity(file_path, 64 * 1024)
    }

    /// Create a new buffered reader with a custom buffer size.
    pub fn with_capacity(file_path: &str, capacity: usize) -> Result<Self> {
        let file = std::fs::File::open(file_path).map_err(Error::IOError)?;
        Ok(Self {
            file,
            buffer: Vec::with_capacity(capacity),
            buffer_start: 0,
            buffer_end: 0,
            buffer_capacity: capacity,
        })
    }

    /// Fill the internal buffer starting at the given offset.
    fn fill_buffer(&mut self, offset: u64) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(Error::IOError)?;

        self.buffer.clear();
        self.buffer.resize(self.buffer_capacity, 0);

        let bytes_read = self.file.read(&mut self.buffer).map_err(Error::IOError)?;
        self.buffer.truncate(bytes_read);
        self.buffer_start = offset;
        self.buffer_end = offset + bytes_read as u64;

        Ok(())
    }
}

impl ByteRangeReader for BufferedRangeReader {
    type Error = Error;

    fn read_range(
        &mut self,
        offset: u64,
        length: u64,
    ) -> core::result::Result<Vec<u8>, Self::Error> {
        let end = offset + length;

        // Check if the requested range is fully within the buffer
        if offset >= self.buffer_start && end <= self.buffer_end {
            let start_idx = (offset - self.buffer_start) as usize;
            let end_idx = start_idx + length as usize;
            return Ok(self.buffer[start_idx..end_idx].to_vec());
        }

        // If the request is larger than our buffer, read directly
        if length as usize > self.buffer_capacity {
            self.file
                .seek(SeekFrom::Start(offset))
                .map_err(Error::IOError)?;
            let mut buffer = vec![0u8; length as usize];
            self.file.read_exact(&mut buffer).map_err(Error::IOError)?;
            return Ok(buffer);
        }

        // Fill buffer starting at the requested offset
        self.fill_buffer(offset)?;

        // Now read from buffer
        if end <= self.buffer_end {
            let start_idx = (offset - self.buffer_start) as usize;
            let end_idx = start_idx + length as usize;
            Ok(self.buffer[start_idx..end_idx].to_vec())
        } else {
            // Buffer didn't have enough data (near end of file)
            Err(Error::TooShortBuffer {
                actual: (self.buffer_end - offset) as usize,
                expected: length as usize,
                file: file!(),
                line: line!(),
            })
        }
    }
}

/// Example HTTP range reader (would be implemented in production)
/// ```rust,ignore
/// use mdf4_rs::index::ByteRangeReader;
/// use mdf4_rs::error::MdfError;
///
/// pub struct HttpRangeReader {
///     client: reqwest::blocking::Client,
///     url: String,
/// }
///
/// impl HttpRangeReader {
///     pub fn new(url: String) -> Self {
///         Self {
///             client: reqwest::blocking::Client::new(),
///             url,
///         }
///     }
/// }
///
/// impl ByteRangeReader for HttpRangeReader {
///     type Error = MdfError;
///     
///     fn read_range(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, Self::Error> {
///         let range_header = format!("bytes={}-{}", offset, offset + length - 1);
///         
///         let response = self.client
///             .get(&self.url)
///             .header("Range", range_header)
///             .send()
///             .map_err(|e| MdfError::BlockSerializationError(format!("HTTP error: {}", e)))?;
///         
///         if !response.status().is_success() {
///             return Err(MdfError::BlockSerializationError(
///                 format!("HTTP error: {}", response.status())
///             ));
///         }
///         
///         let bytes = response.bytes()
///             .map_err(|e| MdfError::BlockSerializationError(format!("Response error: {}", e)))?;
///         
///         Ok(bytes.to_vec())
///     }
/// }
/// ```
pub struct _HttpRangeReaderExample;

impl MdfIndex {
    /// Create an index from an MDF file
    pub fn from_file(file_path: &str) -> Result<Self> {
        let mdf = MDF::from_file(file_path)?;
        let file_size = std::fs::metadata(file_path).map_err(Error::IOError)?.len();

        let mut indexed_groups = Vec::new();

        for group in mdf.channel_groups() {
            let mut indexed_channels = Vec::new();
            let mmap = group.mmap(); // Get memory mapped file data for resolving conversions

            // Index each channel in the group
            for channel in group.channels() {
                let block = channel.block();

                // Clone and resolve conversion dependencies if present
                let resolved_conversion = if let Some(mut conversion) = block.conversion.clone() {
                    // Resolve all dependencies for this conversion block
                    if let Err(e) = conversion.resolve_all_dependencies(mmap) {
                        eprintln!(
                            "Warning: Failed to resolve conversion dependencies for channel '{}': {}",
                            block.name.as_deref().unwrap_or("<unnamed>"),
                            e
                        );
                    }
                    Some(conversion)
                } else {
                    None
                };

                let indexed_channel = IndexedChannel {
                    name: channel.name()?,
                    unit: channel.unit()?,
                    data_type: block.data_type,
                    byte_offset: block.byte_offset,
                    bit_offset: block.bit_offset,
                    bit_count: block.bit_count,
                    channel_type: block.channel_type,
                    flags: block.flags,
                    pos_invalidation_bit: block.pos_invalidation_bit,
                    conversion: resolved_conversion,
                    vlsd_data_address: if block.channel_type == 1 && block.data != 0 {
                        Some(block.data)
                    } else {
                        None
                    },
                };
                indexed_channels.push(indexed_channel);
            }

            // Get data block information
            let data_blocks = Self::extract_data_blocks(&group)?;

            let indexed_group = IndexedChannelGroup {
                name: group.name()?,
                comment: group.comment()?,
                record_id_len: group.raw_data_group().block.record_id_len,
                record_size: group.raw_channel_group().block.samples_byte_nr,
                invalidation_bytes: group.raw_channel_group().block.invalidation_bytes_nr,
                record_count: group.raw_channel_group().block.cycles_nr,
                channels: indexed_channels,
                data_blocks,
            };
            indexed_groups.push(indexed_group);
        }

        Ok(MdfIndex {
            file_size,
            channel_groups: indexed_groups,
        })
    }

    /// Create an index from a file using streaming reads (minimal memory usage).
    ///
    /// This method reads only the metadata blocks needed to build the index,
    /// without loading the entire file into memory. Ideal for large files.
    ///
    /// # Arguments
    /// * `file_path` - Path to the MDF file
    ///
    /// # Example
    /// ```no_run
    /// use mdf4_rs::MdfIndex;
    ///
    /// let index = MdfIndex::from_file_streaming("large_recording.mf4")?;
    /// # Ok::<(), mdf4_rs::Error>(())
    /// ```
    pub fn from_file_streaming(file_path: &str) -> Result<Self> {
        let file_size = std::fs::metadata(file_path).map_err(Error::IOError)?.len();
        let mut reader = BufferedRangeReader::new(file_path)?;
        Self::from_reader(&mut reader, file_size)
    }

    /// Create an index from any byte range reader.
    ///
    /// This is the most flexible method, allowing index creation from files,
    /// HTTP sources, or any other data source implementing `ByteRangeReader`.
    ///
    /// # Arguments
    /// * `reader` - Any implementation of `ByteRangeReader`
    /// * `file_size` - Total size of the file in bytes
    pub fn from_reader<R: ByteRangeReader<Error = Error>>(
        reader: &mut R,
        file_size: u64,
    ) -> Result<Self> {
        // Read and validate ID block (64 bytes at offset 0)
        let id_bytes = reader.read_range(0, 64)?;
        let _id_block = IdentificationBlock::from_bytes(&id_bytes)?;

        // Read HD block (104 bytes at offset 64)
        let hd_bytes = reader.read_range(64, 104)?;
        let header = HeaderBlock::from_bytes(&hd_bytes)?;

        let mut indexed_groups = Vec::new();

        // Follow the DG chain
        let mut dg_addr = header.first_dg_addr;
        while dg_addr != 0 {
            // Read DG block (64 bytes)
            let dg_bytes = reader.read_range(dg_addr, 64)?;
            let dg_block = DataGroupBlock::from_bytes(&dg_bytes)?;

            // Follow the CG chain within this DG
            let mut cg_addr = dg_block.first_cg_addr;
            while cg_addr != 0 {
                // Read CG block (104 bytes)
                let cg_bytes = reader.read_range(cg_addr, 104)?;
                let cg_block = ChannelGroupBlock::from_bytes(&cg_bytes)?;

                // Read CG name if present
                let cg_name = Self::read_text_block(reader, cg_block.acq_name_addr)?;
                let cg_comment = Self::read_text_block(reader, cg_block.comment_addr)?;

                // Follow the CN chain within this CG
                let mut indexed_channels = Vec::new();
                let mut cn_addr = cg_block.first_ch_addr;
                while cn_addr != 0 {
                    // Read CN block (160 bytes)
                    let cn_bytes = reader.read_range(cn_addr, 160)?;
                    let cn_block = ChannelBlock::from_bytes(&cn_bytes)?;

                    // Read channel name
                    let ch_name = Self::read_text_block(reader, cn_block.name_addr)?;

                    // Read unit
                    let ch_unit = Self::read_text_block(reader, cn_block.unit_addr)?;

                    // Read and resolve conversion block if present
                    let conversion =
                        Self::read_conversion_block_streaming(reader, cn_block.conversion_addr)?;

                    let indexed_channel = IndexedChannel {
                        name: ch_name,
                        unit: ch_unit,
                        data_type: cn_block.data_type,
                        byte_offset: cn_block.byte_offset,
                        bit_offset: cn_block.bit_offset,
                        bit_count: cn_block.bit_count,
                        channel_type: cn_block.channel_type,
                        flags: cn_block.flags,
                        pos_invalidation_bit: cn_block.pos_invalidation_bit,
                        conversion,
                        vlsd_data_address: if cn_block.channel_type == 1 && cn_block.data != 0 {
                            Some(cn_block.data)
                        } else {
                            None
                        },
                    };
                    indexed_channels.push(indexed_channel);

                    cn_addr = cn_block.next_ch_addr;
                }

                // Extract data block info for this CG
                let data_blocks =
                    Self::extract_data_blocks_streaming(reader, dg_block.data_block_addr)?;

                let indexed_group = IndexedChannelGroup {
                    name: cg_name,
                    comment: cg_comment,
                    record_id_len: dg_block.record_id_len,
                    record_size: cg_block.samples_byte_nr,
                    invalidation_bytes: cg_block.invalidation_bytes_nr,
                    record_count: cg_block.cycles_nr,
                    channels: indexed_channels,
                    data_blocks,
                };
                indexed_groups.push(indexed_group);

                cg_addr = cg_block.next_cg_addr;
            }

            dg_addr = dg_block.next_dg_addr;
        }

        Ok(MdfIndex {
            file_size,
            channel_groups: indexed_groups,
        })
    }

    /// Read a text block at the given address, returning None if address is 0.
    fn read_text_block<R: ByteRangeReader<Error = Error>>(
        reader: &mut R,
        addr: u64,
    ) -> Result<Option<String>> {
        if addr == 0 {
            return Ok(None);
        }

        // First read the header to get block length (24 bytes)
        let header_bytes = reader.read_range(addr, 24)?;
        let header = BlockHeader::from_bytes(&header_bytes)?;

        // Now read the full block
        let block_bytes = reader.read_range(addr, header.block_len)?;
        let text_block = TextBlock::from_bytes(&block_bytes)?;

        Ok(Some(text_block.text))
    }

    /// Read and parse a conversion block at the given address.
    fn read_conversion_block_streaming<R: ByteRangeReader<Error = Error>>(
        reader: &mut R,
        addr: u64,
    ) -> Result<Option<ConversionBlock>> {
        if addr == 0 {
            return Ok(None);
        }

        // First read the header to get block length
        let header_bytes = reader.read_range(addr, 24)?;
        let header = BlockHeader::from_bytes(&header_bytes)?;

        // Read the full conversion block
        let block_bytes = reader.read_range(addr, header.block_len)?;
        let mut conv_block = ConversionBlock::from_bytes(&block_bytes)?;

        // Resolve references based on conversion type
        Self::resolve_conversion_refs(reader, &mut conv_block)?;

        Ok(Some(conv_block))
    }

    /// Resolve references in a conversion block based on its type.
    fn resolve_conversion_refs<R: ByteRangeReader<Error = Error>>(
        reader: &mut R,
        conv: &mut ConversionBlock,
    ) -> Result<()> {
        match conv.cc_type {
            // Algebraic conversion - first cc_ref is formula text
            ConversionType::Algebraic => {
                if let Some(&formula_addr) = conv.cc_ref.first() {
                    if formula_addr != 0 {
                        conv.formula = Self::read_text_block(reader, formula_addr)?;
                    }
                }
            }
            // Text-based conversions - resolve text references
            ConversionType::ValueToText
            | ConversionType::RangeToText
            | ConversionType::TextToValue
            | ConversionType::TextToText
            | ConversionType::BitfieldText => {
                let mut resolved = BTreeMap::new();
                for (idx, &ref_addr) in conv.cc_ref.iter().enumerate() {
                    if ref_addr != 0 {
                        // Check if this is a text block or nested conversion
                        let header_bytes = reader.read_range(ref_addr, 24)?;
                        let header = BlockHeader::from_bytes(&header_bytes)?;

                        if header.id == "##TX" || header.id == "##MD" {
                            if let Ok(Some(text)) = Self::read_text_block(reader, ref_addr) {
                                resolved.insert(idx, text);
                            }
                        }
                        // Skip nested conversions for now - they're complex
                    }
                }
                if !resolved.is_empty() {
                    conv.resolved_texts = Some(resolved);
                }
            }
            // Linear and other numeric conversions don't need text resolution
            _ => {}
        }

        Ok(())
    }

    /// Extract data block information using streaming reads.
    fn extract_data_blocks_streaming<R: ByteRangeReader<Error = Error>>(
        reader: &mut R,
        data_addr: u64,
    ) -> Result<Vec<DataBlockInfo>> {
        let mut data_blocks = Vec::new();
        let mut current_addr = data_addr;

        while current_addr != 0 {
            // Read block header (24 bytes)
            let header_bytes = reader.read_range(current_addr, 24)?;
            let header = BlockHeader::from_bytes(&header_bytes)?;

            match header.id.as_str() {
                "##DT" | "##DV" => {
                    data_blocks.push(DataBlockInfo {
                        file_offset: current_addr,
                        size: header.block_len,
                        is_compressed: false,
                    });
                    current_addr = 0;
                }
                "##DZ" => {
                    data_blocks.push(DataBlockInfo {
                        file_offset: current_addr,
                        size: header.block_len,
                        is_compressed: true,
                    });
                    current_addr = 0;
                }
                "##DL" => {
                    // Read the full DL block
                    let dl_bytes = reader.read_range(current_addr, header.block_len)?;
                    let dl_block = DataListBlock::from_bytes(&dl_bytes)?;

                    // Process each fragment
                    for &fragment_addr in &dl_block.data_links {
                        if fragment_addr != 0 {
                            let frag_header_bytes = reader.read_range(fragment_addr, 24)?;
                            let frag_header = BlockHeader::from_bytes(&frag_header_bytes)?;

                            data_blocks.push(DataBlockInfo {
                                file_offset: fragment_addr,
                                size: frag_header.block_len,
                                is_compressed: frag_header.id == "##DZ",
                            });
                        }
                    }

                    current_addr = dl_block.next;
                }
                _ => {
                    // Unknown block type, stop
                    current_addr = 0;
                }
            }
        }

        Ok(data_blocks)
    }

    /// Extract data block information from a channel group
    fn extract_data_blocks(
        group: &crate::channel_group::ChannelGroup,
    ) -> Result<Vec<DataBlockInfo>> {
        let mut data_blocks = Vec::new();
        let raw_data_group = group.raw_data_group();
        let mmap = group.mmap();

        // Start at the group's primary data pointer
        let mut current_block_address = raw_data_group.block.data_block_addr;
        while current_block_address != 0 {
            let byte_offset = current_block_address as usize;

            // Read the block header
            let block_header = BlockHeader::from_bytes(&mmap[byte_offset..byte_offset + 24])?;

            match block_header.id.as_str() {
                "##DT" | "##DV" => {
                    // Single contiguous DataBlock
                    let data_block_info = DataBlockInfo {
                        file_offset: current_block_address,
                        size: block_header.block_len,
                        is_compressed: false,
                    };
                    data_blocks.push(data_block_info);
                    // No list to follow, we're done
                    current_block_address = 0;
                }
                "##DZ" => {
                    // Compressed data block
                    let data_block_info = DataBlockInfo {
                        file_offset: current_block_address,
                        size: block_header.block_len,
                        is_compressed: true,
                    };
                    data_blocks.push(data_block_info);
                    current_block_address = 0;
                }
                "##DL" => {
                    // Fragmented list of data blocks
                    let data_list_block = DataListBlock::from_bytes(&mmap[byte_offset..])?;

                    // Parse each fragment in this list
                    for &fragment_address in &data_list_block.data_links {
                        let fragment_offset = fragment_address as usize;
                        let fragment_header =
                            BlockHeader::from_bytes(&mmap[fragment_offset..fragment_offset + 24])?;

                        let is_compressed = fragment_header.id == "##DZ";
                        let data_block_info = DataBlockInfo {
                            file_offset: fragment_address,
                            size: fragment_header.block_len,
                            is_compressed,
                        };
                        data_blocks.push(data_block_info);
                    }

                    // Move to the next DLBLOCK in the chain (0 = end)
                    current_block_address = data_list_block.next;
                }

                unexpected_id => {
                    return Err(Error::BlockIDError {
                        actual: unexpected_id.to_string(),
                        expected: "##DT / ##DV / ##DL / ##DZ".to_string(),
                    });
                }
            }
        }

        Ok(data_blocks)
    }

    /// Save the index to a JSON file.
    ///
    /// Requires the `serde` and `serde_json` features.
    #[cfg(feature = "serde_json")]
    pub fn save_to_file(&self, index_path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            Error::BlockSerializationError(format!("JSON serialization failed: {}", e))
        })?;

        std::fs::write(index_path, json).map_err(Error::IOError)?;

        Ok(())
    }

    /// Load an index from a JSON file.
    ///
    /// Requires the `serde` and `serde_json` features.
    #[cfg(feature = "serde_json")]
    pub fn load_from_file(index_path: &str) -> Result<Self> {
        let json = std::fs::read_to_string(index_path).map_err(Error::IOError)?;

        let index: MdfIndex = serde_json::from_str(&json).map_err(|e| {
            Error::BlockSerializationError(format!("JSON deserialization failed: {}", e))
        })?;

        Ok(index)
    }

    /// Read channel values using the index and a byte range reader
    ///
    /// # Returns
    /// A vector of `Option<DecodedValue>` where:
    /// - `Some(value)` represents a valid decoded value
    /// - `None` represents an invalid value (invalidation bit set or decoding failed)
    pub fn read_channel_values<R: ByteRangeReader<Error = Error>>(
        &self,
        group_index: usize,
        channel_index: usize,
        reader: &mut R,
    ) -> Result<Vec<Option<DecodedValue>>> {
        let group = self
            .channel_groups
            .get(group_index)
            .ok_or_else(|| Error::BlockSerializationError("Invalid group index".to_string()))?;

        let channel = group
            .channels
            .get(channel_index)
            .ok_or_else(|| Error::BlockSerializationError("Invalid channel index".to_string()))?;

        // Handle VLSD channels differently
        if channel.channel_type == 1 && channel.vlsd_data_address.is_some() {
            return self.read_vlsd_channel_values(group, channel, reader);
        }

        // For regular channels, read from data blocks
        self.read_regular_channel_values(group, channel, reader)
    }

    /// Read values for a regular (non-VLSD) channel using byte range reader
    fn read_regular_channel_values<R: ByteRangeReader<Error = Error>>(
        &self,
        group: &IndexedChannelGroup,
        channel: &IndexedChannel,
        reader: &mut R,
    ) -> Result<Vec<Option<DecodedValue>>> {
        // Record structure: record_id + data_bytes + invalidation_bytes
        let record_size = group.record_id_len as usize
            + group.record_size as usize
            + group.invalidation_bytes as usize;
        let mut values = Vec::new();

        // Read from each data block
        for data_block in &group.data_blocks {
            // Handle compression if needed
            if data_block.is_compressed {
                // TODO: Implement decompression for DZ blocks
                return Err(Error::BlockSerializationError(
                    "Compressed blocks not yet supported in index reader".to_string(),
                ));
            }

            // Read the block data (skip 24-byte block header)
            let block_data =
                reader.read_range(data_block.file_offset + 24, data_block.size - 24)?;

            // Process records in this block
            let record_count = block_data.len() / record_size;
            for i in 0..record_count {
                let record_start = i * record_size;
                let record_end = record_start + record_size;
                let record = &block_data[record_start..record_end];

                // Create a ChannelBlock for decoding
                let temp_channel_block = ChannelBlock {
                    header: BlockHeader {
                        id: "##CN".to_string(),
                        reserved0: 0,
                        block_len: 160,
                        links_nr: 8,
                    },
                    next_ch_addr: 0,
                    component_addr: 0,
                    name_addr: 0,
                    source_addr: 0,
                    conversion_addr: 0,
                    data: 0,
                    unit_addr: 0,
                    comment_addr: 0,
                    channel_type: channel.channel_type,
                    sync_type: 0,
                    data_type: channel.data_type,
                    bit_offset: channel.bit_offset,
                    byte_offset: channel.byte_offset,
                    bit_count: channel.bit_count,
                    flags: channel.flags,
                    pos_invalidation_bit: channel.pos_invalidation_bit,
                    precision: 0,
                    reserved1: 0,
                    attachment_nr: 0,
                    min_raw_value: 0.0,
                    max_raw_value: 0.0,
                    lower_limit: 0.0,
                    upper_limit: 0.0,
                    lower_ext_limit: 0.0,
                    upper_ext_limit: 0.0,
                    name: channel.name.clone(),
                    conversion: channel.conversion.clone(),
                };

                // Decode with validity checking
                if let Some(decoded) = decode_channel_value_with_validity(
                    record,
                    group.record_id_len as usize,
                    group.record_size,
                    &temp_channel_block,
                ) {
                    if decoded.is_valid {
                        // Apply conversion if present
                        let final_value = if let Some(conversion) = &channel.conversion {
                            conversion.apply_decoded(decoded.value, &[])?
                        } else {
                            decoded.value
                        };
                        values.push(Some(final_value));
                    } else {
                        // Invalid sample
                        values.push(None);
                    }
                } else {
                    // Decoding failed
                    values.push(None);
                }
            }
        }

        Ok(values)
    }

    /// Read values for a VLSD channel
    fn read_vlsd_channel_values<R: ByteRangeReader<Error = Error>>(
        &self,
        _group: &IndexedChannelGroup,
        _channel: &IndexedChannel,
        _reader: &mut R,
    ) -> Result<Vec<Option<DecodedValue>>> {
        // TODO: Implement VLSD channel reading
        Err(Error::BlockSerializationError(
            "VLSD channels not yet supported in index reader".to_string(),
        ))
    }

    /// Get channel information for a specific group and channel
    pub fn get_channel_info(
        &self,
        group_index: usize,
        channel_index: usize,
    ) -> Option<&IndexedChannel> {
        self.channel_groups
            .get(group_index)?
            .channels
            .get(channel_index)
    }

    /// List all channel groups with their basic information
    pub fn list_channel_groups(&self) -> Vec<(usize, &str, usize)> {
        self.channel_groups
            .iter()
            .enumerate()
            .map(|(i, group)| {
                (
                    i,
                    group.name.as_deref().unwrap_or("<unnamed>"),
                    group.channels.len(),
                )
            })
            .collect()
    }

    /// List all channels in a specific group
    pub fn list_channels(&self, group_index: usize) -> Option<Vec<(usize, &str, &DataType)>> {
        let group = self.channel_groups.get(group_index)?;
        Some(
            group
                .channels
                .iter()
                .enumerate()
                .map(|(i, ch)| (i, ch.name.as_deref().unwrap_or("<unnamed>"), &ch.data_type))
                .collect(),
        )
    }

    /// Get the exact byte ranges needed to read all data for a specific channel
    ///
    /// Returns a vector of (file_offset, length) tuples representing the byte ranges
    /// that need to be read from the file to get all data for the specified channel.
    ///
    /// # Arguments
    /// * `group_index` - Index of the channel group
    /// * `channel_index` - Index of the channel within the group
    ///
    /// # Returns
    /// * `Ok(Vec<(u64, u64)>)` - Vector of (offset, length) byte ranges
    /// * `Err(MdfError)` - If indices are invalid or channel type not supported
    pub fn get_channel_byte_ranges(
        &self,
        group_index: usize,
        channel_index: usize,
    ) -> Result<Vec<(u64, u64)>> {
        let group = self
            .channel_groups
            .get(group_index)
            .ok_or_else(|| Error::BlockSerializationError("Invalid group index".to_string()))?;

        let channel = group
            .channels
            .get(channel_index)
            .ok_or_else(|| Error::BlockSerializationError("Invalid channel index".to_string()))?;

        // Handle VLSD channels differently
        if channel.channel_type == 1 && channel.vlsd_data_address.is_some() {
            return Err(Error::BlockSerializationError(
                "VLSD channels not yet supported for byte range calculation".to_string(),
            ));
        }

        // For regular channels, calculate byte ranges from data blocks
        self.calculate_regular_channel_byte_ranges(group, channel)
    }

    /// Get the exact byte ranges for a specific record range of a channel
    ///
    /// This is useful when you only want to read a subset of records rather than all data.
    ///
    /// # Arguments
    /// * `group_index` - Index of the channel group
    /// * `channel_index` - Index of the channel within the group
    /// * `start_record` - Starting record index (0-based)
    /// * `record_count` - Number of records to read
    ///
    /// # Returns
    /// * `Ok(Vec<(u64, u64)>)` - Vector of (offset, length) byte ranges
    /// * `Err(MdfError)` - If indices are invalid, range is out of bounds, or channel type not supported
    pub fn get_channel_byte_ranges_for_records(
        &self,
        group_index: usize,
        channel_index: usize,
        start_record: u64,
        record_count: u64,
    ) -> Result<Vec<(u64, u64)>> {
        let group = self
            .channel_groups
            .get(group_index)
            .ok_or_else(|| Error::BlockSerializationError("Invalid group index".to_string()))?;

        let channel = group
            .channels
            .get(channel_index)
            .ok_or_else(|| Error::BlockSerializationError("Invalid channel index".to_string()))?;

        // Validate record range
        if start_record + record_count > group.record_count {
            return Err(Error::BlockSerializationError(format!(
                "Record range {}-{} exceeds total records {}",
                start_record,
                start_record + record_count - 1,
                group.record_count
            )));
        }

        // Handle VLSD channels differently
        if channel.channel_type == 1 && channel.vlsd_data_address.is_some() {
            return Err(Error::BlockSerializationError(
                "VLSD channels not yet supported for byte range calculation".to_string(),
            ));
        }

        self.calculate_channel_byte_ranges_for_records(group, channel, start_record, record_count)
    }

    /// Calculate byte ranges for a regular (non-VLSD) channel for all records
    fn calculate_regular_channel_byte_ranges(
        &self,
        group: &IndexedChannelGroup,
        channel: &IndexedChannel,
    ) -> Result<Vec<(u64, u64)>> {
        self.calculate_channel_byte_ranges_for_records(group, channel, 0, group.record_count)
    }

    /// Calculate byte ranges for a regular channel for a specific record range
    fn calculate_channel_byte_ranges_for_records(
        &self,
        group: &IndexedChannelGroup,
        channel: &IndexedChannel,
        start_record: u64,
        record_count: u64,
    ) -> Result<Vec<(u64, u64)>> {
        // Record structure: record_id + data_bytes + invalidation_bytes
        let record_size = group.record_id_len as usize
            + group.record_size as usize
            + group.invalidation_bytes as usize;
        let channel_offset_in_record = group.record_id_len as usize + channel.byte_offset as usize;

        // Calculate how many bytes this channel needs per record
        let channel_bytes_per_record = if matches!(
            channel.data_type,
            DataType::StringLatin1
                | DataType::StringUtf8
                | DataType::StringUtf16LE
                | DataType::StringUtf16BE
                | DataType::ByteArray
                | DataType::MimeSample
                | DataType::MimeStream
        ) {
            channel.bit_count as usize / 8
        } else {
            (channel.bit_offset as usize + channel.bit_count as usize)
                .div_ceil(8)
                .max(1)
        };

        let mut byte_ranges = Vec::new();
        let mut records_processed = 0u64;

        for data_block in &group.data_blocks {
            if data_block.is_compressed {
                return Err(Error::BlockSerializationError(
                    "Compressed blocks not supported for byte range calculation".to_string(),
                ));
            }

            let block_data_start = data_block.file_offset + 24; // Skip block header
            let block_data_size = data_block.size - 24;
            let records_in_block = block_data_size / record_size as u64;

            // Determine which records from this block we need
            let block_start_record = records_processed;
            let block_end_record = records_processed + records_in_block;

            let need_start = start_record.max(block_start_record);
            let need_end = (start_record + record_count).min(block_end_record);

            if need_start < need_end {
                // We need some records from this block
                let first_record_in_block = need_start - block_start_record;
                let last_record_in_block = need_end - block_start_record - 1;

                // Calculate byte range for the channel data in these records
                let first_channel_byte = block_data_start
                    + first_record_in_block * record_size as u64
                    + channel_offset_in_record as u64;

                let last_channel_byte = block_data_start
                    + last_record_in_block * record_size as u64
                    + channel_offset_in_record as u64
                    + channel_bytes_per_record as u64
                    - 1;

                let range_length = last_channel_byte - first_channel_byte + 1;
                byte_ranges.push((first_channel_byte, range_length));
            }

            records_processed = block_end_record;

            // Early exit if we've processed all needed records
            if records_processed >= start_record + record_count {
                break;
            }
        }

        Ok(byte_ranges)
    }

    /// Get a summary of byte ranges for a channel (total bytes, number of ranges)
    ///
    /// This is useful for understanding the I/O pattern before actually reading.
    ///
    /// # Returns
    /// * `(total_bytes, number_of_ranges)` - Total bytes to read and number of separate ranges
    pub fn get_channel_byte_summary(
        &self,
        group_index: usize,
        channel_index: usize,
    ) -> Result<(u64, usize)> {
        let ranges = self.get_channel_byte_ranges(group_index, channel_index)?;
        let total_bytes: u64 = ranges.iter().map(|(_, len)| len).sum();
        Ok((total_bytes, ranges.len()))
    }

    /// Find a channel group index by name
    ///
    /// # Arguments
    /// * `group_name` - Name of the channel group to find
    ///
    /// # Returns
    /// * `Some(group_index)` if found
    /// * `None` if not found
    pub fn find_channel_group_by_name(&self, group_name: &str) -> Option<usize> {
        self.channel_groups
            .iter()
            .enumerate()
            .find(|(_, group)| group.name.as_deref() == Some(group_name))
            .map(|(index, _)| index)
    }

    /// Find a channel index by name within a specific group
    ///
    /// # Arguments
    /// * `group_index` - Index of the channel group to search in
    /// * `channel_name` - Name of the channel to find
    ///
    /// # Returns
    /// * `Some(channel_index)` if found
    /// * `None` if group doesn't exist or channel not found
    pub fn find_channel_by_name(&self, group_index: usize, channel_name: &str) -> Option<usize> {
        let group = self.channel_groups.get(group_index)?;

        group
            .channels
            .iter()
            .enumerate()
            .find(|(_, channel)| channel.name.as_deref() == Some(channel_name))
            .map(|(index, _)| index)
    }

    /// Find a channel by name across all groups
    ///
    /// # Arguments
    /// * `channel_name` - Name of the channel to find
    ///
    /// # Returns
    /// * `Some((group_index, channel_index))` if found
    /// * `None` if not found
    pub fn find_channel_by_name_global(&self, channel_name: &str) -> Option<(usize, usize)> {
        for (group_index, group) in self.channel_groups.iter().enumerate() {
            for (channel_index, channel) in group.channels.iter().enumerate() {
                if channel.name.as_deref() == Some(channel_name) {
                    return Some((group_index, channel_index));
                }
            }
        }
        None
    }

    /// Find all channels with a given name across all groups
    ///
    /// This is useful when the same channel name appears in multiple groups.
    ///
    /// # Arguments
    /// * `channel_name` - Name of the channels to find
    ///
    /// # Returns
    /// * `Vec<(group_index, channel_index)>` - All matching channels
    pub fn find_all_channels_by_name(&self, channel_name: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();

        for (group_index, group) in self.channel_groups.iter().enumerate() {
            for (channel_index, channel) in group.channels.iter().enumerate() {
                if channel.name.as_deref() == Some(channel_name) {
                    matches.push((group_index, channel_index));
                }
            }
        }

        matches
    }

    /// Read channel values by name using a byte range reader
    ///
    /// Convenience method that finds the channel by name and reads its values.
    /// If multiple channels have the same name, uses the first one found.
    ///
    /// # Arguments
    /// * `channel_name` - Name of the channel to read
    /// * `reader` - Byte range reader implementation
    ///
    /// # Returns
    /// * `Ok(Vec<Option<DecodedValue>>)` - Channel values (None for invalid samples)
    /// * `Err(MdfError)` - If channel not found or reading fails
    pub fn read_channel_values_by_name<R: ByteRangeReader<Error = Error>>(
        &self,
        channel_name: &str,
        reader: &mut R,
    ) -> Result<Vec<Option<DecodedValue>>> {
        let (group_index, channel_index) = self
            .find_channel_by_name_global(channel_name)
            .ok_or_else(|| {
                Error::BlockSerializationError(format!("Channel '{}' not found", channel_name))
            })?;

        self.read_channel_values(group_index, channel_index, reader)
    }

    /// Get byte ranges for a channel by name
    ///
    /// # Arguments
    /// * `channel_name` - Name of the channel
    ///
    /// # Returns
    /// * `Ok(Vec<(u64, u64)>)` - Byte ranges as (offset, length) tuples
    /// * `Err(MdfError)` - If channel not found or calculation fails
    pub fn get_channel_byte_ranges_by_name(&self, channel_name: &str) -> Result<Vec<(u64, u64)>> {
        let (group_index, channel_index) = self
            .find_channel_by_name_global(channel_name)
            .ok_or_else(|| {
                Error::BlockSerializationError(format!("Channel '{}' not found", channel_name))
            })?;

        self.get_channel_byte_ranges(group_index, channel_index)
    }

    /// Get channel information by name
    ///
    /// # Arguments
    /// * `channel_name` - Name of the channel
    ///
    /// # Returns
    /// * `Some((group_index, channel_index, &IndexedChannel))` - Channel info if found
    /// * `None` - If channel not found
    pub fn get_channel_info_by_name(
        &self,
        channel_name: &str,
    ) -> Option<(usize, usize, &IndexedChannel)> {
        let (group_index, channel_index) = self.find_channel_by_name_global(channel_name)?;
        let channel = self.get_channel_info(group_index, channel_index)?;
        Some((group_index, channel_index, channel))
    }
}
