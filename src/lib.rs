#![forbid(unsafe_code)]

//! # mdf4-rs
//!
//! A Rust library for reading and writing ASAM MDF 4 (Measurement Data Format) files.
//!
//! MDF4 is a binary file format standardized by ASAM for storing measurement data,
//! commonly used in automotive and industrial applications for recording sensor data,
//! CAN bus messages, and other time-series measurements.
//!
//! ## Features
//!
//! - **Reading**: Parse MDF4 files and access channel data with automatic value conversion
//! - **Writing**: Create new MDF4 files with multiple channel groups and data types
//! - **Indexing**: Generate lightweight JSON indexes for efficient partial file access
//! - **Cutting**: Extract time-based segments from recordings
//! - **Merging**: Combine multiple MDF files with matching channel layouts
//!
//! ## Supported MDF Version
//!
//! This crate targets MDF 4.1+ and implements a subset of the specification sufficient
//! for common measurement data workflows. Notably:
//!
//! - Standard data types (integers, floats, strings)
//! - Linear, rational, and algebraic value conversions
//! - Value-to-text and text-to-value mappings
//! - Invalidation bits for marking invalid samples
//! - Multiple channel groups and data blocks
//!
//! ## Quick Start
//!
//! ### Reading an MDF file
//!
//! ```no_run
//! use mdf4_rs::{MDF, Result};
//!
//! fn main() -> Result<()> {
//!     let mdf = MDF::from_file("recording.mf4")?;
//!
//!     for group in mdf.channel_groups() {
//!         println!("Group: {:?}", group.name()?);
//!
//!         for channel in group.channels() {
//!             let name = channel.name()?.unwrap_or_default();
//!             let values = channel.values()?;
//!             let valid_count = values.iter().filter(|v| v.is_some()).count();
//!             println!("  {}: {} valid samples", name, valid_count);
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ### Writing an MDF file
//!
//! ```no_run
//! use mdf4_rs::{MdfWriter, DataType, DecodedValue, Result};
//!
//! fn main() -> Result<()> {
//!     let mut writer = MdfWriter::new("output.mf4")?;
//!     writer.init_mdf_file()?;
//!
//!     // Create a channel group
//!     let cg = writer.add_channel_group(None, |_| {})?;
//!
//!     // Add a temperature channel
//!     writer.add_channel(&cg, None, |ch| {
//!         ch.data_type = DataType::FloatLE;
//!         ch.name = Some("Temperature".into());
//!         ch.bit_count = 64;
//!     })?;
//!
//!     // Write data records
//!     writer.start_data_block_for_cg(&cg, 0)?;
//!     for temp in [20.5, 21.0, 21.5, 22.0] {
//!         writer.write_record(&cg, &[DecodedValue::Float(temp)])?;
//!     }
//!     writer.finish_data_block(&cg)?;
//!     writer.finalize()?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Using the Index for Efficient Access
//!
//! ```no_run
//! use mdf4_rs::{MdfIndex, FileRangeReader, Result};
//!
//! fn main() -> Result<()> {
//!     // Create an index from a file
//!     let index = MdfIndex::from_file("recording.mf4")?;
//!
//!     // Save index for later use
//!     index.save_to_file("recording.mdf4.index")?;
//!
//!     // Load index and read specific channel
//!     let index = MdfIndex::load_from_file("recording.mdf4.index")?;
//!     let mut reader = FileRangeReader::new("recording.mf4")?;
//!
//!     let values = index.read_channel_values_by_name("Temperature", &mut reader)?;
//!     println!("Read {} values", values.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Module Overview
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`blocks`] | Low-level MDF block structures (for advanced use) |
//! | [`parsing`] | Internal parsing utilities and raw block types |
//! | [`writer`] | MDF file creation with [`MdfWriter`] |
//! | [`index`] | File indexing for efficient partial reads |
//! | [`cut`] | Time-based segment extraction |
//! | [`merge`] | File merging utilities |
//! | [`error`] | Error types and [`Result`] alias |
//!
//! ## Error Handling
//!
//! All fallible operations return [`Result<T>`], which is an alias for
//! `std::result::Result<T, Error>`. The [`Error`] enum covers I/O errors,
//! parsing failures, and invalid file structures.

pub mod blocks;
pub mod parsing;

mod channel;
mod channel_group;
mod mdf;

pub mod cut;
pub mod error;
pub mod index;
pub mod merge;
pub mod writer;

// Re-export commonly used types at the crate root
pub use blocks::DataType;
pub use channel::Channel;
pub use channel_group::ChannelGroup;
pub use cut::cut_mdf_by_time;
pub use error::{Error, Result};
pub use index::{BufferedRangeReader, ByteRangeReader, FileRangeReader, MdfIndex};
pub use mdf::MDF;
pub use merge::merge_files;
pub use parsing::decoder::DecodedValue;
pub use writer::MdfWriter;
