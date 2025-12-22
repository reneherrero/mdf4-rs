pub mod decoder;

mod mdf_file;
mod raw_channel;
mod raw_channel_group;
mod raw_data_group;
mod source_info;

// Internal-only types (used by MDF parsing implementation)
pub(crate) use mdf_file::MdfFile;
pub(crate) use raw_channel::RawChannel;
pub(crate) use raw_channel_group::RawChannelGroup;
pub(crate) use raw_data_group::RawDataGroup;
pub(crate) use source_info::SourceInfo;
