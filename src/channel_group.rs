use crate::{
    Result,
    blocks::read_string_block,
    channel::Channel,
    parsing::{RawChannelGroup, RawDataGroup, SourceInfo},
};

/// High level wrapper for a channel group.
///
/// The struct references raw channel group data and provides ergonomic access
/// to its metadata and channels without decoding any actual samples.
pub struct ChannelGroup<'a> {
    raw_data_group: &'a RawDataGroup,
    raw_channel_group: &'a RawChannelGroup,
    mmap: &'a [u8],
}

impl<'a> ChannelGroup<'a> {
    /// Create a new [`ChannelGroup`] referencing the underlying raw blocks.
    ///
    /// # Arguments
    /// * `raw_data_group` - Parent data group containing this channel group
    /// * `raw_channel_group` - The raw channel group block
    /// * `mmap` - Memory mapped file backing all data
    ///
    /// # Returns
    /// A [`ChannelGroup`] handle with no decoded data.
    pub fn new(
        raw_data_group: &'a RawDataGroup,
        raw_channel_group: &'a RawChannelGroup,
        mmap: &'a [u8],
    ) -> Self {
        ChannelGroup {
            raw_data_group,
            raw_channel_group,
            mmap,
        }
    }

    /// Retrieve the human readable group name.
    pub fn name(&self) -> Result<Option<String>> {
        read_string_block(self.mmap, self.raw_channel_group.block.acq_name_addr)
    }

    /// Retrieve the group comment if present.
    pub fn comment(&self) -> Result<Option<String>> {
        read_string_block(self.mmap, self.raw_channel_group.block.comment_addr)
    }

    /// Get the acquisition source information if available.
    pub fn source(&self) -> Result<Option<SourceInfo>> {
        let addr = self.raw_channel_group.block.acq_source_addr;
        SourceInfo::from_mmap(self.mmap, addr)
    }

    /// Build all [`Channel`] objects for this group.
    ///
    /// No channel data is decoded; the returned channels simply reference the
    /// raw blocks.
    pub fn channels(&self) -> Vec<Channel<'a>> {
        let mut channels = Vec::new();
        for raw_channel in &self.raw_channel_group.raw_channels {
            let channel = Channel::new(
                &raw_channel.block,
                self.raw_data_group,
                self.raw_channel_group,
                raw_channel,
                self.mmap,
            );
            channels.push(channel);
        }

        channels
    }

    /// Get the raw data group (for internal use)
    pub fn raw_data_group(&self) -> &RawDataGroup {
        self.raw_data_group
    }

    /// Get the raw channel group (for internal use)
    pub fn raw_channel_group(&self) -> &RawChannelGroup {
        self.raw_channel_group
    }

    /// Get the memory mapped data (for internal use)
    pub fn mmap(&self) -> &[u8] {
        self.mmap
    }
}
