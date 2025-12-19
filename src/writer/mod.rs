//! MDF4 file writer module.
//!
//! This module provides [`MdfWriter`], a builder-style API for creating MDF4 files.
//! The writer handles all low-level details including block alignment, link updates,
//! and proper encoding of different data types.
//!
//! # Architecture
//!
//! MDF files are organized hierarchically:
//!
//! ```text
//! MDF File
//! └── Data Groups (DG)
//!     └── Channel Groups (CG)
//!         └── Channels (CN)
//!             └── Data values
//! ```
//!
//! The writer maintains this structure and automatically links blocks together.
//!
//! # Writing Workflow
//!
//! 1. Create a new [`MdfWriter`]
//! 2. Initialize the file structure with [`init_mdf_file()`](MdfWriter::init_mdf_file)
//! 3. Add channel groups with [`add_channel_group()`](MdfWriter::add_channel_group)
//! 4. Add channels to groups with [`add_channel()`](MdfWriter::add_channel)
//! 5. Start a data block with [`start_data_block_for_cg()`](MdfWriter::start_data_block_for_cg)
//! 6. Write records with [`write_record()`](MdfWriter::write_record)
//! 7. Finish the data block with [`finish_data_block()`](MdfWriter::finish_data_block)
//! 8. Finalize the file with [`finalize()`](MdfWriter::finalize)
//!
//! # Example (std feature)
//!
#![cfg_attr(feature = "std", doc = "```no_run")]
#![cfg_attr(
    feature = "std",
    doc = "use mdf4_rs::{MdfWriter, DataType, DecodedValue, Result};"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "fn write_sensor_data() -> Result<()> {")]
#![cfg_attr(
    feature = "std",
    doc = "    let mut writer = MdfWriter::new(\"sensor_data.mf4\")?;"
)]
#![cfg_attr(feature = "std", doc = "    writer.init_mdf_file()?;")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(
    feature = "std",
    doc = "    // Create a channel group for sensor readings"
)]
#![cfg_attr(
    feature = "std",
    doc = "    let sensors = writer.add_channel_group(None, |cg| {"
)]
#![cfg_attr(feature = "std", doc = "        // Configure channel group if needed")]
#![cfg_attr(feature = "std", doc = "    })?;")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    // Add a time channel (master channel)")]
#![cfg_attr(
    feature = "std",
    doc = "    let time_ch = writer.add_channel(&sensors, None, |ch| {"
)]
#![cfg_attr(feature = "std", doc = "        ch.data_type = DataType::FloatLE;")]
#![cfg_attr(feature = "std", doc = "        ch.name = Some(\"Time\".into());")]
#![cfg_attr(feature = "std", doc = "        ch.bit_count = 64;")]
#![cfg_attr(feature = "std", doc = "    })?;")]
#![cfg_attr(feature = "std", doc = "    writer.set_time_channel(&time_ch)?;")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(
    feature = "std",
    doc = "    // Add a temperature channel linked after time"
)]
#![cfg_attr(
    feature = "std",
    doc = "    let temp_ch = writer.add_channel(&sensors, Some(&time_ch), |ch| {"
)]
#![cfg_attr(feature = "std", doc = "        ch.data_type = DataType::FloatLE;")]
#![cfg_attr(
    feature = "std",
    doc = "        ch.name = Some(\"Temperature\".into());"
)]
#![cfg_attr(feature = "std", doc = "        ch.bit_count = 64;")]
#![cfg_attr(feature = "std", doc = "    })?;")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    // Write measurement data")]
#![cfg_attr(
    feature = "std",
    doc = "    writer.start_data_block_for_cg(&sensors, 0)?;"
)]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    writer.write_record(&sensors, &[")]
#![cfg_attr(feature = "std", doc = "        DecodedValue::Float(0.0),    // Time")]
#![cfg_attr(
    feature = "std",
    doc = "        DecodedValue::Float(25.5),   // Temperature"
)]
#![cfg_attr(feature = "std", doc = "    ])?;")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    writer.finish_data_block(&sensors)?;")]
#![cfg_attr(feature = "std", doc = "    writer.finalize()?;")]
#![cfg_attr(feature = "std", doc = "")]
#![cfg_attr(feature = "std", doc = "    Ok(())")]
#![cfg_attr(feature = "std", doc = "}")]
#![cfg_attr(feature = "std", doc = "```")]
//!
//! # no_std Usage
//!
//! With just the `alloc` feature, you can write MDF data to memory:
//!
//! ```ignore
//! use mdf4_rs::{MdfWriter, DataType, DecodedValue, Result};
//! use mdf4_rs::writer::VecWriter;
//!
//! fn write_to_memory() -> Result<Vec<u8>> {
//!     let writer = VecWriter::new();
//!     let mut mdf = MdfWriter::from_writer(writer);
//!     mdf.init_mdf_file()?;
//!     // ... add channels and data ...
//!     mdf.finalize()?;
//!     Ok(mdf.into_inner().into_inner())
//! }
//! ```
//!
//! # Supported Data Types
//!
//! The writer supports all standard MDF data types through [`DataType`](crate::DataType):
//!
//! - Unsigned integers (8, 16, 32, 64 bit, little/big endian)
//! - Signed integers (8, 16, 32, 64 bit, little/big endian)
//! - Floating point (32, 64 bit, little/big endian)
//! - Strings (UTF-8, Latin-1)

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::blocks::ChannelBlock;

mod data;
mod init;
mod io;
mod traits;

use data::ChannelEncoder;
pub use traits::{MdfWrite, VecWriter};

#[cfg(feature = "std")]
pub use traits::FileWriter;

/// Helper structure tracking an open data block during writing.
struct OpenDataBlock {
    dg_id: String,
    dt_id: String,
    start_pos: u64,
    record_size: usize,
    record_count: u64,
    /// Total number of records written across all DT blocks for this group
    total_record_count: u64,
    channels: Vec<ChannelBlock>,
    dt_ids: Vec<String>,
    dt_positions: Vec<u64>,
    dt_sizes: Vec<u64>,
    /// Scratch buffer reused for record encoding
    record_buf: Vec<u8>,
    /// Template filled with constant values used to initialise each record
    record_template: Vec<u8>,
    /// Precomputed per-channel encoders
    encoders: Vec<ChannelEncoder>,
}

/// Writer for creating MDF4 files.
///
/// `MdfWriter` provides a structured API for building valid MDF4 files with
/// proper block alignment (8-byte), zero padding, and link resolution.
///
/// # Thread Safety
///
/// `MdfWriter` is not thread-safe. All writing operations should be performed
/// from a single thread.
///
/// # Performance
///
/// When using file-based writing (`std` feature), the writer uses internal
/// buffering (1 MB by default). For different buffer sizes, use
/// [`new_with_capacity()`](Self::new_with_capacity).
pub struct MdfWriter<W: MdfWrite = VecWriter> {
    writer: W,
    offset: u64,
    block_positions: BTreeMap<String, u64>,
    open_dts: BTreeMap<String, OpenDataBlock>,
    dt_counter: usize,
    last_dg: Option<String>,
    cg_to_dg: BTreeMap<String, String>,
    cg_offsets: BTreeMap<String, usize>,
    cg_channels: BTreeMap<String, Vec<ChannelBlock>>,
    channel_map: BTreeMap<String, (String, usize)>,
}

impl<W: MdfWrite> MdfWriter<W> {
    /// Create a new MdfWriter from any type implementing MdfWrite.
    ///
    /// This is the general constructor that works with any writer backend.
    pub fn from_writer(writer: W) -> Self {
        MdfWriter {
            writer,
            offset: 0,
            block_positions: BTreeMap::new(),
            open_dts: BTreeMap::new(),
            dt_counter: 0,
            last_dg: None,
            cg_to_dg: BTreeMap::new(),
            cg_offsets: BTreeMap::new(),
            cg_channels: BTreeMap::new(),
            channel_map: BTreeMap::new(),
        }
    }

    /// Consume the writer and return the underlying writer backend.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Get a reference to the underlying writer backend.
    pub fn writer(&self) -> &W {
        &self.writer
    }

    /// Get a mutable reference to the underlying writer backend.
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl MdfWriter<VecWriter> {
    /// Create a new MdfWriter that writes to an in-memory buffer.
    ///
    /// This is useful for embedded systems or when you want to build
    /// the MDF data in memory before writing to external storage.
    pub fn in_memory() -> Self {
        Self::from_writer(VecWriter::new())
    }

    /// Create a new MdfWriter with the specified initial buffer capacity.
    pub fn in_memory_with_capacity(capacity: usize) -> Self {
        Self::from_writer(VecWriter::with_capacity(capacity))
    }
}
