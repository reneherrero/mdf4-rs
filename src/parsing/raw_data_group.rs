use super::RawChannelGroup;
use crate::{
    Error, Result,
    blocks::{
        DataBlock, DataGroupBlock, DataListBlock, {BlockHeader, BlockParse},
    },
};
use alloc::string::ToString;
use alloc::vec::Vec;

#[cfg(feature = "compression")]
use crate::blocks::DzBlock;

/// Either a reference to existing data or owned decompressed data.
#[derive(Debug)]
pub enum DataBlockData<'a> {
    /// Reference to memory-mapped data (uncompressed DT/DV blocks).
    Borrowed(&'a [u8]),
    /// Owned decompressed data (from DZ blocks).
    #[cfg(feature = "compression")]
    Owned(Vec<u8>),
}

impl<'a> DataBlockData<'a> {
    /// Get the data as a slice.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            DataBlockData::Borrowed(s) => s,
            #[cfg(feature = "compression")]
            DataBlockData::Owned(v) => v.as_slice(),
        }
    }

    /// Get the length of the data.
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Check if the data is empty.
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

/// A data block that may contain borrowed or owned data.
///
/// This allows handling both regular DT/DV blocks (zero-copy) and
/// decompressed DZ blocks (owned data) with a unified interface.
#[derive(Debug)]
pub struct ResolvedDataBlock<'a> {
    /// Original block type ID (e.g., "##DT", "##DV").
    pub block_id: &'static str,
    /// The data contents (may be borrowed or owned).
    pub data: DataBlockData<'a>,
}

impl<'a> ResolvedDataBlock<'a> {
    /// Iterate over raw records of fixed size.
    ///
    /// # Arguments
    /// * `record_size` - Size in bytes of one record (including record ID)
    ///
    /// # Returns
    /// An iterator yielding each raw record slice.
    pub fn records(&self, record_size: usize) -> impl Iterator<Item = &[u8]> {
        self.data.as_slice().chunks_exact(record_size)
    }
}

#[derive(Debug, Clone)]
pub struct RawDataGroup {
    pub block: DataGroupBlock,
    pub channel_groups: Vec<RawChannelGroup>,
    /// Whether this data group is from an unfinalized MDF file.
    pub is_unfinalized: bool,
}
impl RawDataGroup {
    /// Collect all data blocks referenced by this data group.
    ///
    /// The returned vector contains the `DT` or `DV` blocks in the order they
    /// appear on disk, transparently following any `DL` list chains.
    ///
    /// For unfinalized MDF files, this will automatically use the correct
    /// parsing method based on the `is_unfinalized` flag.
    ///
    /// # Arguments
    /// * `mmap` - Memory mapped file containing the MDF data
    ///
    /// # Returns
    /// A vector of [`DataBlock`] objects or an [`Error`] if parsing fails.
    pub fn data_blocks<'a>(&self, mmap: &'a [u8]) -> Result<Vec<DataBlock<'a>>> {
        let mut collected_blocks = Vec::new();

        // Start at the group's primary data pointer
        let mut current_block_address = self.block.data_block_addr;
        while current_block_address != 0 {
            let byte_offset = current_block_address as usize;

            // Read the block header
            let block_header = BlockHeader::from_bytes(&mmap[byte_offset..byte_offset + 24])?;

            match block_header.id.as_str() {
                "##DT" | "##DV" => {
                    // Check if this is an empty block in an unfinalized file
                    // (block_len == 24 means header only, but data follows anyway)
                    let data_block = if self.is_unfinalized && block_header.length == 24 {
                        // Use unfinalized parsing - read until end of file
                        DataBlock::from_bytes_unfinalized(&mmap[byte_offset..])?
                    } else {
                        // Normal parsing
                        DataBlock::from_bytes(&mmap[byte_offset..])?
                    };
                    collected_blocks.push(data_block);
                    // No list to follow, we're done
                    current_block_address = 0;
                }
                "##DL" => {
                    // Fragmented list of data blocks
                    let data_list_block = DataListBlock::from_bytes(&mmap[byte_offset..])?;

                    // Parse each fragment in this list
                    for &fragment_address in &data_list_block.data_block_addrs {
                        let fragment_offset = fragment_address as usize;
                        let fragment_block = DataBlock::from_bytes(&mmap[fragment_offset..])?;

                        collected_blocks.push(fragment_block);
                    }

                    // Move to the next DLBLOCK in the chain (0 = end)
                    current_block_address = data_list_block.next_dl_addr;
                }

                unexpected_id => {
                    return Err(Error::BlockIDError {
                        actual: unexpected_id.to_string(),
                        expected: "##DT / ##DV / ##DL".to_string(),
                    });
                }
            }
        }

        Ok(collected_blocks)
    }

    /// Collect all data blocks, decompressing DZ blocks if needed.
    ///
    /// This method handles both regular DT/DV blocks and compressed DZ blocks.
    /// For DZ blocks, the data is decompressed and stored as owned data.
    ///
    /// # Arguments
    /// * `mmap` - Memory mapped file containing the MDF data
    ///
    /// # Returns
    /// A vector of [`ResolvedDataBlock`] objects or an [`Error`] if parsing fails.
    ///
    /// # Feature Requirements
    /// - Without `compression` feature: Returns error on DZ blocks
    /// - With `compression` feature: Decompresses DZ blocks transparently
    pub fn resolved_data_blocks<'a>(&self, mmap: &'a [u8]) -> Result<Vec<ResolvedDataBlock<'a>>> {
        let mut collected_blocks = Vec::new();

        let mut current_block_address = self.block.data_block_addr;
        while current_block_address != 0 {
            let byte_offset = current_block_address as usize;
            let block_header = BlockHeader::from_bytes(&mmap[byte_offset..byte_offset + 24])?;

            match block_header.id.as_str() {
                "##DT" | "##DV" => {
                    let data_block = if self.is_unfinalized && block_header.length == 24 {
                        DataBlock::from_bytes_unfinalized(&mmap[byte_offset..])?
                    } else {
                        DataBlock::from_bytes(&mmap[byte_offset..])?
                    };
                    collected_blocks.push(ResolvedDataBlock {
                        block_id: if block_header.id == "##DT" {
                            "##DT"
                        } else {
                            "##DV"
                        },
                        data: DataBlockData::Borrowed(data_block.data),
                    });
                    current_block_address = 0;
                }
                #[cfg(feature = "compression")]
                "##DZ" => {
                    let dz_block = DzBlock::from_bytes(&mmap[byte_offset..])?;
                    let decompressed = dz_block.decompress()?;
                    collected_blocks.push(ResolvedDataBlock {
                        block_id: "##DT", // DZ decompresses to DT-equivalent data
                        data: DataBlockData::Owned(decompressed),
                    });
                    current_block_address = 0;
                }
                #[cfg(not(feature = "compression"))]
                "##DZ" => {
                    return Err(Error::BlockSerializationError(
                        "DZ blocks require the 'compression' feature".to_string(),
                    ));
                }
                "##DL" => {
                    let data_list_block = DataListBlock::from_bytes(&mmap[byte_offset..])?;

                    for &fragment_address in &data_list_block.data_block_addrs {
                        let fragment_offset = fragment_address as usize;
                        let frag_header =
                            BlockHeader::from_bytes(&mmap[fragment_offset..fragment_offset + 24])?;

                        match frag_header.id.as_str() {
                            "##DT" | "##DV" => {
                                let fragment_block =
                                    DataBlock::from_bytes(&mmap[fragment_offset..])?;
                                collected_blocks.push(ResolvedDataBlock {
                                    block_id: if frag_header.id == "##DT" {
                                        "##DT"
                                    } else {
                                        "##DV"
                                    },
                                    data: DataBlockData::Borrowed(fragment_block.data),
                                });
                            }
                            #[cfg(feature = "compression")]
                            "##DZ" => {
                                let dz_block = DzBlock::from_bytes(&mmap[fragment_offset..])?;
                                let decompressed = dz_block.decompress()?;
                                collected_blocks.push(ResolvedDataBlock {
                                    block_id: "##DT",
                                    data: DataBlockData::Owned(decompressed),
                                });
                            }
                            #[cfg(not(feature = "compression"))]
                            "##DZ" => {
                                return Err(Error::BlockSerializationError(
                                    "DZ blocks require the 'compression' feature".to_string(),
                                ));
                            }
                            other => {
                                return Err(Error::BlockIDError {
                                    actual: other.to_string(),
                                    expected: "##DT / ##DV / ##DZ".to_string(),
                                });
                            }
                        }
                    }

                    current_block_address = data_list_block.next_dl_addr;
                }
                unexpected_id => {
                    return Err(Error::BlockIDError {
                        actual: unexpected_id.to_string(),
                        expected: "##DT / ##DV / ##DL / ##DZ".to_string(),
                    });
                }
            }
        }

        Ok(collected_blocks)
    }
}
