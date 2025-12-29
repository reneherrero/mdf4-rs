//! LIN bus integration for MDF4 files.
//!
//! This module provides utilities for logging LIN (Local Interconnect Network)
//! traffic to MDF4 files following the ASAM MDF4 Bus Logging specification.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant `LIN_Frame` format
//! - Classic (LIN 1.x) and Enhanced (LIN 2.x) checksum support
//! - Protected ID calculation with parity bits
//! - Direction tracking (Tx/Rx)
//! - Error flag tracking (checksum, sync, framing, no response)
//!
//! # LIN Protocol Overview
//!
//! LIN is a low-cost, single-wire serial network used in automotive applications
//! for communication between sensors, actuators, and ECUs. Key characteristics:
//!
//! - Single master, multiple slave architecture
//! - Up to 16 slave nodes
//! - Frame IDs 0-59 for unconditional frames, 60-61 for diagnostics
//! - Maximum 8 bytes of data per frame
//! - Baud rates: typically 9600, 10400, or 19200 bps
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::lin::{RawLinLogger, LinFrame};
//!
//! // Create logger
//! let mut logger = RawLinLogger::with_bus_name("Body_LIN")?;
//!
//! // Log frames using enhanced checksum (LIN 2.x)
//! logger.log(0x20, timestamp_us, &[0x01, 0x02, 0x03, 0x04]);
//!
//! // Or use classic checksum (LIN 1.x)
//! logger.log_classic(0x21, timestamp_us, &[0x05, 0x06]);
//!
//! // Log with explicit direction
//! logger.log_tx(0x22, timestamp_us, &data);
//! logger.log_rx(0x23, timestamp_us, &data);
//!
//! // Use LinFrame for more control
//! let frame = LinFrame::with_enhanced_checksum(0x3C, &diagnostic_data)
//!     .with_tx();
//! logger.log_frame(timestamp_us, frame);
//!
//! // Finalize
//! let mdf_bytes = logger.finalize()?;
//! ```

pub mod frame;
mod raw_logger;

// Re-export frame types
pub use frame::{
    ChecksumType, LinFlags, LinFrame, MAX_LIN_DATA_LEN, MAX_LIN_ID, ScheduleEntryType,
};

// Re-export logger
pub use raw_logger::RawLinLogger;
