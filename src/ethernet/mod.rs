//! Ethernet bus integration for MDF4 files.
//!
//! This module provides utilities for logging Ethernet traffic to MDF4 files
//! following the ASAM MDF4 Bus Logging specification.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant `ETH_Frame` format
//! - Support for standard and jumbo frames
//! - Direction tracking (Tx/Rx)
//! - VLAN tag support (802.1Q)
//! - Common EtherType constants
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::ethernet::{RawEthernetLogger, MacAddress, EthernetFrame, ethertype};
//!
//! // Create logger
//! let mut logger = RawEthernetLogger::new()?;
//!
//! // Log from raw bytes
//! logger.log(timestamp_us, &frame_bytes);
//!
//! // Or construct and log a frame
//! let frame = EthernetFrame::new(
//!     MacAddress::broadcast(),
//!     MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
//!     ethertype::IPV4,
//!     payload.to_vec(),
//! );
//! logger.log_frame(timestamp_us, frame);
//!
//! // Log with explicit direction
//! logger.log_tx(timestamp_us, &tx_frame_bytes);
//! logger.log_rx(timestamp_us, &rx_frame_bytes);
//!
//! // Finalize
//! let mdf_bytes = logger.finalize()?;
//! ```

pub mod frame;
mod raw_logger;

// Re-export frame types
pub use frame::{
    // Constants
    ETH_HEADER_SIZE,
    ETHERTYPE_SIZE,
    EthernetFlags,
    EthernetFrame,
    MAC_ADDR_SIZE,
    MAX_ETHERNET_FRAME,
    MAX_ETHERNET_PAYLOAD,
    MAX_JUMBO_PAYLOAD,
    MacAddress,
    // EtherType module
    ethertype,
};

// Re-export logger
pub use raw_logger::RawEthernetLogger;
