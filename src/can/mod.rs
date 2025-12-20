//! CAN bus integration for MDF4 files.
//!
//! This module provides utilities for logging and reading CAN bus data with MDF4 files.
//! It supports multiple modes:
//!
//! 1. **With DBC**: Use [`DbcMdfLogger`] for full signal decoding with metadata
//! 2. **Without DBC**: Use [`RawCanLogger`] for raw frame capture
//! 3. **Post-processing**: Use [`DbcOverlayReader`] to decode raw captures with DBC
//!
//! # Features
//!
//! - Uses `Dbc::decode()` for full DBC support (multiplexing, value descriptions, etc.)
//! - Raw frame logging when no DBC is available
//! - Read-time DBC overlay for post-processing raw captures
//! - Batch processing for efficient logging
//! - Support for both Standard (11-bit) and Extended (29-bit) CAN IDs
//! - Full metadata preservation (units, conversions, limits)
//! - Raw value storage with conversion blocks for maximum precision
//! - **CAN FD support**: Up to 64 bytes per frame with BRS/ESI flags
//!
//! # Example with DBC
//!
//! ```ignore
//! use mdf4_rs::can::DbcMdfLogger;
//!
//! // Parse DBC file
//! let dbc = dbc_rs::Dbc::parse(dbc_content)?;
//!
//! // Create logger with full metadata
//! let mut logger = DbcMdfLogger::builder(&dbc)
//!     .store_raw_values(true)
//!     .build()?;
//!
//! // Log CAN frames
//! logger.log(0x100, timestamp_us, &frame_data);
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```
//!
//! # Example without DBC (Raw Logging)
//!
//! ```ignore
//! use mdf4_rs::can::RawCanLogger;
//!
//! // Create raw logger (no DBC needed)
//! let mut logger = RawCanLogger::new()?;
//!
//! // Log raw CAN frames
//! logger.log(0x100, timestamp_us, &frame_data);
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

mod dbc_compat;
mod dbc_logger;
#[cfg(feature = "std")]
mod dbc_overlay;
pub mod fd;
mod raw_logger;
mod timestamped_frame;

pub use dbc_compat::{
    extract_message_info, signal_to_bit_count, signal_to_conversion,
    signal_to_conversion_with_range, signal_to_data_type, value_descriptions_to_mapping,
    MessageInfo, SignalInfo,
};
pub use dbc_logger::{DbcMdfLogger, DbcMdfLoggerBuilder, DbcMdfLoggerConfig};
#[cfg(feature = "std")]
pub use dbc_overlay::{DbcOverlayReader, DecodedFrame, OverlayStatistics, SignalValue};
pub use fd::{dlc_to_len, len_to_dlc, FdFlags, FdFrame, SimpleFdFrame, MAX_FD_DATA_LEN};
pub use raw_logger::RawCanLogger;
pub use timestamped_frame::TimestampedFrame;

// Re-export commonly used dbc-rs types
pub use dbc_rs::{ByteOrder, Dbc, DecodedSignal, Message, Signal};
