//! FlexRay bus integration for MDF4 files.
//!
//! This module provides utilities for logging FlexRay traffic to MDF4 files
//! following the ASAM MDF4 Bus Logging specification.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant `FLEXRAY_Frame` format
//! - Dual channel support (A, B, or both)
//! - Static and dynamic segment frames
//! - Startup and sync frame support
//! - Null frame handling
//! - Direction tracking (Tx/Rx)
//! - Error flag tracking
//!
//! # FlexRay Protocol Overview
//!
//! FlexRay is a deterministic, fault-tolerant, high-speed automotive bus used
//! for safety-critical and chassis applications. Key characteristics:
//!
//! - Dual-channel redundant architecture (Channel A and B)
//! - Time-triggered with static and dynamic segments
//! - Up to 10 Mbit/s data rate
//! - Cycle-based communication (64 cycles, 0-63)
//! - Slot IDs 1-2047 for frame identification
//! - Maximum 254 bytes payload per frame
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::flexray::{RawFlexRayLogger, FlexRayFrame, FlexRayChannel};
//!
//! // Create logger
//! let mut logger = RawFlexRayLogger::with_cluster_name("Chassis_FR")?;
//!
//! // Log frames on channel A
//! logger.log_channel_a(100, 0, timestamp_us, &payload);
//!
//! // Log frames on channel B
//! logger.log_channel_b(101, 0, timestamp_us, &payload);
//!
//! // Log on both channels
//! logger.log(102, 0, FlexRayChannel::AB, timestamp_us, &payload);
//!
//! // Log with explicit direction
//! logger.log_tx(103, 1, FlexRayChannel::A, timestamp_us, &payload);
//! logger.log_rx(104, 1, FlexRayChannel::B, timestamp_us, &payload);
//!
//! // Log special frames
//! logger.log_null_frame(50, 0, FlexRayChannel::A, timestamp_us);
//! logger.log_startup(1, 0, FlexRayChannel::AB, timestamp_us, &startup_data);
//!
//! // Use FlexRayFrame for more control
//! let frame = FlexRayFrame::channel_a(100, 5, payload.to_vec())
//!     .with_tx()
//!     .with_dynamic();
//! logger.log_frame(timestamp_us, &frame);
//!
//! // Finalize
//! let mdf_bytes = logger.finalize()?;
//! ```

pub mod frame;
mod raw_logger;

// Re-export frame types
pub use frame::{
    FLEXRAY_HEADER_SIZE, FlexRayChannel, FlexRayFlags, FlexRayFrame, MAX_CYCLE_COUNT,
    MAX_FLEXRAY_PAYLOAD, MAX_SLOT_ID,
};

// Re-export logger
pub use raw_logger::RawFlexRayLogger;
