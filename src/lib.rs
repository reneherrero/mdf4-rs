#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

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
//! - **100% safe Rust** - `#![forbid(unsafe_code)]`
//! - **no_std support** - Works on embedded targets with `alloc`
//! - **Reading** (std only): Parse MDF4 files and access channel data
//! - **Writing**: Create new MDF4 files (works with `alloc` only)
//! - **Indexing** (std only): Generate lightweight JSON indexes
//! - **Cutting** (std only): Extract time-based segments from recordings
//! - **Merging** (std only): Combine multiple MDF files
//!
//! ## Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `std` | Yes | Full std library support. Enables file I/O, indexing, merging. |
//! | `alloc` | Yes | Heap allocation. Required for all functionality. |
//!
//! ## no_std Usage
//!
//! For embedded targets, disable default features and enable `alloc`:
//!
//! ```toml
//! [dependencies]
//! mdf4-rs = { version = "0.1", default-features = false, features = ["alloc"] }
//! ```
//!
//! With `alloc` only, you can:
//! - Create MDF files in memory using `MdfWriter::from_writer()`
//! - Serialize blocks to byte vectors
//! - Use all block types and data encoding
//!
//! ## Quick Start
//!
#![cfg_attr(feature = "std", doc = "### Reading an MDF file")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "```no_run")]
#![cfg_attr(feature = "std", doc = "use mdf4_rs::{MDF, Result};")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "fn main() -> Result<()> {")]
#![cfg_attr(
    feature = "std",
    doc = "    let mdf = MDF::from_file(\"recording.mf4\")?;"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    for group in mdf.channel_groups() {")]
#![cfg_attr(
    feature = "std",
    doc = "        println!(\"Group: {:?}\", group.name()?);"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "        for channel in group.channels() {")]
#![cfg_attr(
    feature = "std",
    doc = "            let name = channel.name()?.unwrap_or_default();"
)]
#![cfg_attr(feature = "std", doc = "            let values = channel.values()?;")]
#![cfg_attr(
    feature = "std",
    doc = "            let valid_count = values.iter().filter(|v| v.is_some()).count();"
)]
#![cfg_attr(
    feature = "std",
    doc = "            println!(\"  {}: {} valid samples\", name, valid_count);"
)]
#![cfg_attr(feature = "std", doc = "        }")]
#![cfg_attr(feature = "std", doc = "    }")]
#![cfg_attr(feature = "std", doc = "    Ok(())")]
#![cfg_attr(feature = "std", doc = "}")]
#![cfg_attr(feature = "std", doc = "```")]
//!
//! ### Writing an MDF file
//!
//! ```ignore
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
#![cfg_attr(feature = "std", doc = "### Using the Index for Efficient Access")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "```no_run")]
#![cfg_attr(
    feature = "std",
    doc = "use mdf4_rs::{MdfIndex, FileRangeReader, Result};"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "fn main() -> Result<()> {")]
#![cfg_attr(feature = "std", doc = "    // Create an index from a file")]
#![cfg_attr(
    feature = "std",
    doc = "    let index = MdfIndex::from_file(\"recording.mf4\")?;"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    // Save index for later use")]
#![cfg_attr(
    feature = "std",
    doc = "    index.save_to_file(\"recording.mdf4.index\")?;"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    // Load index and read specific channel")]
#![cfg_attr(
    feature = "std",
    doc = "    let index = MdfIndex::load_from_file(\"recording.mdf4.index\")?;"
)]
#![cfg_attr(
    feature = "std",
    doc = "    let mut reader = FileRangeReader::new(\"recording.mf4\")?;"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(
    feature = "std",
    doc = "    let values = index.read_channel_values_by_name(\"Temperature\", &mut reader)?;"
)]
#![cfg_attr(
    feature = "std",
    doc = "    println!(\"Read {} values\", values.len());"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    Ok(())")]
#![cfg_attr(feature = "std", doc = "}")]
#![cfg_attr(feature = "std", doc = "```")]
//!
//! ## Module Overview
//!
//! | Module | Description | Requires |
//! |--------|-------------|----------|
//! | [`blocks`] | Low-level MDF block structures | `alloc` |
//! | [`writer`] | MDF file creation | `alloc` |
//! | [`parsing`] | File parsing utilities | `std` |
//! | [`index`] | File indexing | `std` |
//! | [`cut`] | Time-based segment extraction | `std` |
//! | [`merge`] | File merging utilities | `std` |
//! | [`error`] | Error types and [`Result`] alias | `alloc` |
//!
//! ## Error Handling
//!
//! All fallible operations return [`Result<T>`], which is an alias for
//! `core::result::Result<T, Error>`. The [`Error`] enum covers I/O errors,
//! parsing failures, and invalid file structures.

#[cfg(feature = "alloc")]
extern crate alloc;

// Shared types available with alloc
#[cfg(feature = "alloc")]
mod types;

// Modules available with alloc (writing support)
#[cfg(feature = "alloc")]
pub mod blocks;
#[cfg(feature = "alloc")]
pub mod error;
#[cfg(feature = "alloc")]
pub mod writer;

// CAN bus integration (requires alloc, dbc-specific features gated inside)
#[cfg(feature = "alloc")]
pub mod can;

// Modules requiring std (file I/O)
#[cfg(feature = "std")]
mod channel;
#[cfg(feature = "std")]
mod channel_group;
#[cfg(feature = "std")]
pub mod cut;
#[cfg(feature = "std")]
pub mod index;
#[cfg(feature = "std")]
mod mdf;
#[cfg(feature = "std")]
pub mod merge;
#[cfg(feature = "std")]
pub mod parsing;

// Re-export commonly used types at the crate root
#[cfg(feature = "alloc")]
pub use blocks::DataType;
#[cfg(feature = "alloc")]
pub use error::{Error, Result};
#[cfg(feature = "alloc")]
pub use types::DecodedValue;
#[cfg(feature = "alloc")]
pub use writer::MdfWriter;
#[cfg(feature = "alloc")]
pub use writer::{FlushPolicy, StreamingConfig};

#[cfg(feature = "std")]
pub use channel::Channel;
#[cfg(feature = "std")]
pub use channel_group::ChannelGroup;
#[cfg(feature = "std")]
pub use cut::cut_mdf_by_time;
#[cfg(feature = "std")]
pub use index::{BufferedRangeReader, ByteRangeReader, FileRangeReader, MdfIndex};
#[cfg(feature = "std")]
pub use mdf::MDF;
#[cfg(feature = "std")]
pub use merge::merge_files;
