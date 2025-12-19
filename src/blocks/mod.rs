// src/blocks/mod.rs
mod channel_block;
mod channel_group_block;
mod common;
mod conversion;
mod data_block;
mod data_group_block;
mod data_list_block;
mod header_block;
mod identification_block;
mod metadata_block;
mod signal_data_block;
mod source_block;
mod text_block;

// Re-export common types
pub use common::{BlockHeader, BlockParse, DataType, read_string_block};

// Re-export block types
pub use channel_block::ChannelBlock;
pub use channel_group_block::ChannelGroupBlock;
pub use data_block::DataBlock;
pub use data_group_block::DataGroupBlock;
pub use data_list_block::DataListBlock;
pub use header_block::HeaderBlock;
pub use identification_block::IdentificationBlock;
pub use metadata_block::MetadataBlock;
pub use signal_data_block::SignalDataBlock;
pub use source_block::{SourceBlock, SourceType, BusType, read_source_block};
pub use text_block::TextBlock;

// Re-export conversion types
pub use conversion::{ConversionBlock, ConversionType};
