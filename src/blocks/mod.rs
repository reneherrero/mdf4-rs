// src/blocks/mod.rs

// ============================================================================
// Block Size Constants (internal use only)
// ============================================================================
// Fixed sizes for MDF 4.x block structures. Variable-length blocks (TX, MD, DT,
// SD, DL) don't have fixed sizes and are determined by their header.length.

/// Identification block size (64 bytes) - file format identifier at offset 0.
pub(crate) const ID_BLOCK_SIZE: usize = 64;

/// Header block size (104 bytes) - file-level metadata after identification.
pub(crate) const HD_BLOCK_SIZE: usize = 104;

/// Data group block size (64 bytes) - groups channel groups sharing data.
pub(crate) const DG_BLOCK_SIZE: usize = 64;

/// Channel group block size (104 bytes) - groups channels with common time base.
pub(crate) const CG_BLOCK_SIZE: usize = 104;

/// Channel block size (160 bytes) - defines a single measurement channel.
pub(crate) const CN_BLOCK_SIZE: usize = 160;

/// Source block size (56 bytes) - describes data acquisition source.
pub(crate) const SI_BLOCK_SIZE: usize = 56;

// ============================================================================
// Submodules
// ============================================================================

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
pub use common::{BlockHeader, BlockParse, DataType};
// Internal-only exports (std only - used by MdfFile parsing)
#[cfg(feature = "std")]
pub(crate) use common::read_string_block;

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
#[cfg(feature = "std")]
pub(crate) use source_block::read_source_block;
pub use source_block::{BusType, SourceBlock, SourceType};
pub use text_block::TextBlock;

// Re-export conversion types
pub use conversion::{ConversionBlock, ConversionType};
