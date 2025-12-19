use super::RawChannelGroup;
use crate::{
    Error, Result,
    blocks::{
        DataBlock, DataGroupBlock, DataListBlock, {BlockHeader, BlockParse},
    },
};

#[derive(Debug)]
pub struct RawDataGroup {
    pub block: DataGroupBlock,
    pub channel_groups: Vec<RawChannelGroup>,
}
impl RawDataGroup {
    /// Collect all data blocks referenced by this data group.
    ///
    /// The returned vector contains the `DT` or `DV` blocks in the order they
    /// appear on disk, transparently following any `DL` list chains.
    ///
    /// # Arguments
    /// * `mmap` - Memory mapped file containing the MDF data
    ///
    /// # Returns
    /// A vector of [`DataBlock`] objects or an [`Error`] if parsing fails.
    pub fn data_blocks<'a>(&self, mmap: &'a [u8]) -> Result<Vec<DataBlock<'a>>> {
        let mut collected_blocks = Vec::new();

        // Start at the group’s primary data pointer
        let mut current_block_address = self.block.data_block_addr;
        while current_block_address != 0 {
            let byte_offset = current_block_address as usize;

            // Read the block header
            let block_header = BlockHeader::from_bytes(&mmap[byte_offset..byte_offset + 24])?;

            match block_header.id.as_str() {
                "##DT" | "##DV" => {
                    // Single contiguous DataBlock
                    let data_block = DataBlock::from_bytes(&mmap[byte_offset..])?;
                    collected_blocks.push(data_block);
                    // No list to follow, we’re done
                    current_block_address = 0;
                }
                "##DL" => {
                    // Fragmented list of data blocks
                    let data_list_block = DataListBlock::from_bytes(&mmap[byte_offset..])?;

                    // Parse each fragment in this list
                    for &fragment_address in &data_list_block.data_links {
                        let fragment_offset = fragment_address as usize;
                        let fragment_block = DataBlock::from_bytes(&mmap[fragment_offset..])?;

                        collected_blocks.push(fragment_block);
                    }

                    // Move to the next DLBLOCK in the chain (0 = end)
                    current_block_address = data_list_block.next;
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
}
