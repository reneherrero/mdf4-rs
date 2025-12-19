use super::RawChannel;
use crate::blocks::ChannelGroupBlock;

#[derive(Debug)]
pub struct RawChannelGroup {
    pub block: ChannelGroupBlock,
    pub raw_channels: Vec<RawChannel>,
}
