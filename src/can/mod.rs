//! CAN bus integration for MDF4 files.
//!
//! This module provides utilities for logging CAN bus data to MDF4 files
//! using signal definitions from DBC files. It integrates with:
//! - [`dbc-rs`](https://crates.io/crates/dbc-rs) for DBC parsing and signal decoding
//! - [`embedded-can`](https://crates.io/crates/embedded-can) for hardware-agnostic CAN frames
//!
//! # Features
//!
//! - Uses `Dbc::decode()` for full DBC support (multiplexing, value descriptions, etc.)
//! - Batch processing for efficient logging
//! - Support for both Standard (11-bit) and Extended (29-bit) CAN IDs
//! - Full metadata preservation (units, conversions, limits)
//! - Raw value storage with conversion blocks for maximum precision
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::can::DbcMdfLogger;
//!
//! // Parse DBC file
//! let dbc = dbc_rs::Dbc::parse(dbc_content)?;
//!
//! // Create logger with full metadata
//! let mut logger = DbcMdfLogger::builder(&dbc)
//!     .store_raw_values(true)  // Store raw values with conversions
//!     .build()?;
//!
//! // Log CAN frames
//! logger.log(0x100, timestamp_us, &frame_data);
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

mod dbc_compat;
mod dbc_logger;
mod timestamped_frame;

pub use dbc_compat::{
    extract_message_info, signal_to_bit_count, signal_to_conversion,
    signal_to_conversion_with_range, signal_to_data_type, value_descriptions_to_mapping,
    MessageInfo, SignalInfo,
};
pub use dbc_logger::{DbcMdfLogger, DbcMdfLoggerBuilder, DbcMdfLoggerConfig};
pub use timestamped_frame::TimestampedFrame;

// Re-export commonly used dbc-rs types
pub use dbc_rs::{ByteOrder, Dbc, DecodedSignal, Message, Signal};
