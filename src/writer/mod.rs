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
//! # Example
//!
//! ```no_run
//! use mdf4_rs::{MdfWriter, DataType, DecodedValue, Result};
//!
//! fn write_sensor_data() -> Result<()> {
//!     let mut writer = MdfWriter::new("sensor_data.mf4")?;
//!     writer.init_mdf_file()?;
//!
//!     // Create a channel group for sensor readings
//!     let sensors = writer.add_channel_group(None, |cg| {
//!         // Configure channel group if needed
//!     })?;
//!
//!     // Add a time channel (master channel)
//!     let time_ch = writer.add_channel(&sensors, None, |ch| {
//!         ch.data_type = DataType::FloatLE;
//!         ch.name = Some("Time".into());
//!         ch.bit_count = 64;
//!     })?;
//!     writer.set_time_channel(&time_ch)?;
//!
//!     // Add a temperature channel linked after time
//!     let temp_ch = writer.add_channel(&sensors, Some(&time_ch), |ch| {
//!         ch.data_type = DataType::FloatLE;
//!         ch.name = Some("Temperature".into());
//!         ch.bit_count = 64;
//!     })?;
//!
//!     // Add a pressure channel linked after temperature
//!     writer.add_channel(&sensors, Some(&temp_ch), |ch| {
//!         ch.data_type = DataType::FloatLE;
//!         ch.name = Some("Pressure".into());
//!         ch.bit_count = 64;
//!     })?;
//!
//!     // Write measurement data
//!     writer.start_data_block_for_cg(&sensors, 0)?;
//!
//!     // Each record contains values for all channels in order
//!     writer.write_record(&sensors, &[
//!         DecodedValue::Float(0.0),    // Time
//!         DecodedValue::Float(25.5),   // Temperature
//!         DecodedValue::Float(101.3),  // Pressure
//!     ])?;
//!
//!     writer.write_record(&sensors, &[
//!         DecodedValue::Float(0.1),
//!         DecodedValue::Float(25.7),
//!         DecodedValue::Float(101.2),
//!     ])?;
//!
//!     writer.finish_data_block(&sensors)?;
//!     writer.finalize()?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Channel Linking
//!
//! Channels within a group form a linked list. When adding channels:
//!
//! - Pass `None` as `prev_cn_id` for the first channel (links from channel group)
//! - Pass `Some(&previous_id)` to chain subsequent channels
//!
//! **Important**: All channels in a group must be properly chained. Using `None`
//! for multiple channels will overwrite the channel group's first channel link.
//!
//! # Supported Data Types
//!
//! The writer supports all standard MDF data types through [`DataType`](crate::DataType):
//!
//! - Unsigned integers (8, 16, 32, 64 bit, little/big endian)
//! - Signed integers (8, 16, 32, 64 bit, little/big endian)
//! - Floating point (32, 64 bit, little/big endian)
//! - Strings (UTF-8, Latin-1)

use crate::blocks::ChannelBlock;
use std::{
    collections::HashMap,
    io::{Seek, Write},
};

mod data;
mod init;
mod io;

use data::ChannelEncoder;

trait WriteSeek: Write + Seek {}
impl<T: Write + Seek> WriteSeek for T {}

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
/// The writer uses internal buffering (1 MB by default). For different buffer
/// sizes, use [`new_with_capacity()`](Self::new_with_capacity).
pub struct MdfWriter {
    file: Box<dyn WriteSeek>,
    offset: u64,
    block_positions: HashMap<String, u64>,
    open_dts: HashMap<String, OpenDataBlock>,
    dt_counter: usize,
    last_dg: Option<String>,
    cg_to_dg: HashMap<String, String>,
    cg_offsets: HashMap<String, usize>,
    cg_channels: HashMap<String, Vec<ChannelBlock>>,
    channel_map: HashMap<String, (String, usize)>,
}
