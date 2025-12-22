use super::{RawChannel, RawChannelGroup, RawDataGroup};
use crate::{
    Error, Result,
    blocks::{BlockParse, ChannelGroupBlock, DataGroupBlock, HeaderBlock, IdentificationBlock},
};
use std::fs::File;
use std::io::Read;

#[derive(Debug, Clone)]
pub struct MdfFile {
    pub identification: IdentificationBlock,
    pub header: HeaderBlock,
    pub data_groups: Vec<RawDataGroup>,
    /// File data buffer. Stored to guarantee lifetime for slices used during parsing.
    pub mmap: Vec<u8>,
    /// Whether this is an unfinalized MDF file (file_id == "UnFinMF ").
    pub is_unfinalized: bool,
}

impl MdfFile {
    /// Parse an MDF file from a given file path.
    ///
    /// # Arguments
    /// * `path` - Path to the `.mf4` file on disk.
    ///
    /// # Returns
    /// An [`MdfFile`] containing all parsed blocks or an [`crate::Error`] if the
    /// file could not be read or decoded.
    pub fn parse_from_file(path: &str) -> Result<Self> {
        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len() as usize;

        // Read entire file into memory
        let mut data = Vec::with_capacity(file_size);
        file.read_to_end(&mut data)?;

        Self::parse_from_bytes(data)
    }

    /// Parse an MDF file from a byte buffer.
    ///
    /// # Arguments
    /// * `data` - Complete file contents as a byte vector.
    ///
    /// # Returns
    /// An [`MdfFile`] containing all parsed blocks or an [`crate::Error`] if the
    /// data could not be decoded.
    pub fn parse_from_bytes(data: Vec<u8>) -> Result<Self> {
        // Validate minimum file size
        if data.len() < 64 + 104 {
            return Err(Error::TooShortBuffer {
                actual: data.len(),
                expected: 64 + 104,
                file: file!(),
                line: line!(),
            });
        }

        // Parse Identification block (first 64 bytes) and Header block (next 104 bytes)
        let identification = IdentificationBlock::from_bytes(&data[0..64])?;
        let header = HeaderBlock::from_bytes(&data[64..64 + 104])?;

        // Check if file is unfinalized
        let is_unfinalized = identification.file_id.trim() == "UnFinMF";

        // Parse Data Groups, assume a linked list of data groups.
        let mut data_groups = Vec::new();
        let mut dg_addr = header.first_dg_addr;
        while dg_addr != 0 {
            let dg_offset = dg_addr as usize;

            // Bounds check
            if dg_offset >= data.len() {
                return Err(Error::TooShortBuffer {
                    actual: data.len(),
                    expected: dg_offset + 1,
                    file: file!(),
                    line: line!(),
                });
            }

            let data_group_block = DataGroupBlock::from_bytes(&data[dg_offset..])?;
            // Save next dg address before moving data_group_block.
            let next_dg_addr = data_group_block.next_dg_addr;

            let mut next_cg_addr = data_group_block.first_cg_addr;
            let mut raw_channel_groups = Vec::new();
            while next_cg_addr != 0 {
                // Parse channel group
                let offset = next_cg_addr as usize;

                // Bounds check
                if offset >= data.len() {
                    return Err(Error::TooShortBuffer {
                        actual: data.len(),
                        expected: offset + 1,
                        file: file!(),
                        line: line!(),
                    });
                }

                let mut channel_group_block = ChannelGroupBlock::from_bytes(&data[offset..])?;
                next_cg_addr = channel_group_block.next_cg_addr;
                let channels = channel_group_block.read_channels(&data)?;

                let raw_channels: Vec<RawChannel> = channels
                    .into_iter()
                    .map(|channel_block| RawChannel {
                        block: channel_block,
                    })
                    .collect();

                let channel_group = RawChannelGroup {
                    block: channel_group_block,
                    raw_channels,
                };
                raw_channel_groups.push(channel_group);
            }
            let dg = RawDataGroup {
                block: data_group_block,
                channel_groups: raw_channel_groups,
                is_unfinalized,
            };
            data_groups.push(dg);

            dg_addr = next_dg_addr;
        }

        Ok(Self {
            identification,
            header,
            data_groups,
            mmap: data,
            is_unfinalized,
        })
    }
}
