//! Raw LIN frame logger following ASAM MDF4 Bus Logging specification.
//!
//! This module provides [`RawLinLogger`], a logger for capturing raw LIN frames
//! to MDF4 files using the ASAM-compliant `LIN_Frame` format.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant format
//! - `LIN_Frame` channel with ByteArray (ID + Length + Flags + Checksum + Data)
//! - Timestamp as Float64 in seconds
//! - Source metadata (LIN bus name)
//! - Classic and Enhanced checksum support
//! - Error flag tracking
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::lin::{RawLinLogger, LinFrame};
//!
//! let mut logger = RawLinLogger::new()?;
//!
//! // Log a LIN frame (takes ownership)
//! let frame = LinFrame::with_enhanced_checksum(0x20, &[0x01, 0x02, 0x03, 0x04]);
//! logger.log_frame(timestamp_us, frame);
//!
//! // Or log from components
//! logger.log(0x20, timestamp_us, &[0x01, 0x02, 0x03, 0x04]);
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use super::frame::{LinFlags, LinFrame};
use crate::bus_logging::{
    BusLoggerConfig, TimestampedFrame, init_bus_channel_group, write_timestamped_frames,
};

/// LIN_Frame size in ASAM format.
/// ID(1) + Length(1) + Flags(1) + Checksum(1) + Data(8) = 12 bytes
const LIN_FRAME_SIZE: usize = 12;

/// Raw LIN frame logger using ASAM MDF4 Bus Logging format.
///
/// This logger captures raw LIN frames using the industry-standard
/// `LIN_Frame` composite format.
///
/// ## Channel Group Structure
///
/// All LIN frames are stored in a single channel group:
/// - `{source_name}_LIN_Frame` - LIN frames with ID, data, and metadata
///
/// ## LIN_Frame Format
///
/// Each frame is stored as a 12-byte ByteArray:
/// - Byte 0: Frame ID (0-63)
/// - Byte 1: Data length (0-8)
/// - Byte 2: Flags (direction, errors, checksum type)
/// - Byte 3: Checksum
/// - Bytes 4-11: Data (8 bytes, zero-padded)
pub struct RawLinLogger<W: crate::writer::MdfWrite> {
    writer: crate::MdfWriter<W>,
    /// Source name for metadata
    source_name: String,
    /// Buffered frames
    buffer: Vec<TimestampedFrame<LinFrame>>,
    /// Channel group ID
    channel_group: Option<String>,
    initialized: bool,
}

impl RawLinLogger<crate::writer::VecWriter> {
    /// Create a new raw LIN logger with in-memory output.
    pub fn new() -> crate::Result<Self> {
        Self::with_source_name("LIN")
    }

    /// Create a new raw LIN logger with a custom source name.
    ///
    /// The source name is used for channel group names and source metadata.
    /// Examples: "LIN", "LIN1", "Body_LIN", etc.
    pub fn with_source_name(source_name: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());
        Ok(Self {
            writer,
            source_name: String::from(source_name),
            buffer: Vec::new(),
            channel_group: None,
            initialized: false,
        })
    }

    /// Create a new raw LIN logger with a custom bus name.
    ///
    /// Alias for [`with_source_name`](Self::with_source_name) for API compatibility.
    pub fn with_bus_name(bus_name: &str) -> crate::Result<Self> {
        Self::with_source_name(bus_name)
    }

    /// Create a new raw LIN logger with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> crate::Result<Self> {
        let writer =
            crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self {
            writer,
            source_name: String::from("LIN"),
            buffer: Vec::with_capacity(capacity / LIN_FRAME_SIZE),
            channel_group: None,
            initialized: false,
        })
    }

    /// Finalize the MDF file and return the bytes.
    pub fn finalize(mut self) -> crate::Result<Vec<u8>> {
        self.flush_and_finalize()?;
        Ok(self.writer.into_inner().into_inner())
    }
}

#[cfg(feature = "std")]
impl RawLinLogger<crate::writer::FileWriter> {
    /// Create a new raw LIN logger that writes to a file.
    pub fn new_file(path: &str) -> crate::Result<Self> {
        Self::new_file_with_source_name(path, "LIN")
    }

    /// Create a new raw LIN logger that writes to a file with custom source name.
    pub fn new_file_with_source_name(path: &str, source_name: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::new(path)?;
        Ok(Self {
            writer,
            source_name: String::from(source_name),
            buffer: Vec::new(),
            channel_group: None,
            initialized: false,
        })
    }

    /// Create a new raw LIN logger that writes to a file with custom bus name.
    ///
    /// Alias for [`new_file_with_source_name`](Self::new_file_with_source_name) for API compatibility.
    pub fn new_file_with_bus_name(path: &str, bus_name: &str) -> crate::Result<Self> {
        Self::new_file_with_source_name(path, bus_name)
    }

    /// Finalize and close the MDF file.
    pub fn finalize_file(mut self) -> crate::Result<()> {
        self.flush_and_finalize()
    }
}

impl<W: crate::writer::MdfWrite> RawLinLogger<W> {
    /// Set the source name for metadata.
    ///
    /// Must be called before logging any frames.
    pub fn set_source_name(&mut self, name: &str) {
        self.source_name = String::from(name);
    }

    /// Set the LIN bus name for source metadata.
    ///
    /// Alias for [`set_source_name`](Self::set_source_name) for API compatibility.
    pub fn set_bus_name(&mut self, name: &str) {
        self.set_source_name(name);
    }

    /// Log a raw LIN frame by ID and data.
    ///
    /// Uses enhanced checksum (LIN 2.x) by default.
    ///
    /// # Arguments
    /// * `id` - Frame ID (0-63)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Frame data (up to 8 bytes)
    ///
    /// # Returns
    /// Always returns `true`
    pub fn log(&mut self, id: u8, timestamp_us: u64, data: &[u8]) -> bool {
        self.log_frame(timestamp_us, LinFrame::with_enhanced_checksum(id, data))
    }

    /// Log a LIN frame with classic checksum (LIN 1.x).
    pub fn log_classic(&mut self, id: u8, timestamp_us: u64, data: &[u8]) -> bool {
        self.log_frame(timestamp_us, LinFrame::with_classic_checksum(id, data))
    }

    /// Log a transmitted LIN frame.
    pub fn log_tx(&mut self, id: u8, timestamp_us: u64, data: &[u8]) -> bool {
        self.log_frame(
            timestamp_us,
            LinFrame::with_enhanced_checksum(id, data).with_tx(),
        )
    }

    /// Log a received LIN frame.
    pub fn log_rx(&mut self, id: u8, timestamp_us: u64, data: &[u8]) -> bool {
        self.log_frame(
            timestamp_us,
            LinFrame::with_enhanced_checksum(id, data).with_rx(),
        )
    }

    /// Log a LinFrame struct (takes ownership, avoids clone).
    ///
    /// # Arguments
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `frame` - The LIN frame to log (consumed)
    pub fn log_frame(&mut self, timestamp_us: u64, frame: LinFrame) -> bool {
        self.buffer.push(TimestampedFrame::new(timestamp_us, frame));
        true
    }

    /// Log a LinFrame struct by reference (clones the frame).
    ///
    /// Use [`log_frame`](Self::log_frame) if you don't need to reuse the frame.
    pub fn log_frame_ref(&mut self, timestamp_us: u64, frame: &LinFrame) -> bool {
        self.log_frame(timestamp_us, frame.clone())
    }

    /// Log a frame with explicit flags.
    pub fn log_with_flags(
        &mut self,
        id: u8,
        timestamp_us: u64,
        data: &[u8],
        flags: LinFlags,
        checksum: u8,
    ) -> bool {
        let mut frame = LinFrame::new(id, data);
        frame.flags = flags;
        frame.checksum = checksum;
        self.log_frame(timestamp_us, frame)
    }

    /// Flush buffered data to the MDF writer.
    pub fn flush(&mut self) -> crate::Result<()> {
        if !self.initialized {
            self.initialize_mdf()?;
        }

        if self.buffer.is_empty() {
            return Ok(());
        }

        self.write_frames()?;
        self.buffer.clear();

        Ok(())
    }

    /// Initialize the MDF file structure.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        self.writer.init_mdf_file()?;

        let config = BusLoggerConfig {
            source_name: self.source_name.clone(),
            group_name: alloc::format!("{}_LIN_Frame", self.source_name),
            data_channel_name: String::from("LIN_Frame"),
            data_channel_bits: (LIN_FRAME_SIZE * 8) as u32,
            source_block: crate::blocks::SourceBlock::lin_bus(),
        };

        let (cg, _data_ch) = init_bus_channel_group(&mut self.writer, &config)?;

        self.channel_group = Some(cg);
        self.initialized = true;
        Ok(())
    }

    /// Write frames to the MDF file.
    fn write_frames(&mut self) -> crate::Result<()> {
        let cg = match &self.channel_group {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        write_timestamped_frames(&mut self.writer, &cg, self.buffer.drain(..))?;
        Ok(())
    }

    /// Flush and finalize the MDF file.
    fn flush_and_finalize(&mut self) -> crate::Result<()> {
        self.flush()?;
        self.writer.finalize()
    }

    /// Get the total number of frames logged.
    pub fn total_frame_count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the number of unique frame IDs.
    pub fn unique_id_count(&self) -> usize {
        let mut ids = alloc::collections::BTreeSet::new();
        for entry in &self.buffer {
            ids.insert(entry.frame.id);
        }
        ids.len()
    }

    /// Get the number of frames for a specific ID.
    pub fn frame_count_for_id(&self, id: u8) -> usize {
        self.buffer
            .iter()
            .filter(|e| e.frame.id == (id & super::frame::MAX_LIN_ID))
            .count()
    }

    /// Get count of transmitted frames.
    pub fn tx_frame_count(&self) -> usize {
        self.buffer.iter().filter(|e| e.frame.flags.is_tx()).count()
    }

    /// Get count of received frames.
    pub fn rx_frame_count(&self) -> usize {
        self.buffer.iter().filter(|e| e.frame.flags.is_rx()).count()
    }

    /// Get count of frames with errors.
    pub fn error_frame_count(&self) -> usize {
        self.buffer
            .iter()
            .filter(|e| e.frame.flags.has_error())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_lin_logger_basic() {
        let mut logger = RawLinLogger::new().unwrap();

        assert!(logger.log(0x20, 1000, &[0x01, 0x02, 0x03, 0x04]));
        assert!(logger.log(0x21, 2000, &[0x05, 0x06]));
        assert!(logger.log(0x20, 3000, &[0x07, 0x08, 0x09, 0x0A]));

        assert_eq!(logger.total_frame_count(), 3);
        assert_eq!(logger.frame_count_for_id(0x20), 2);
        assert_eq!(logger.frame_count_for_id(0x21), 1);
        assert_eq!(logger.unique_id_count(), 2);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
        assert_eq!(&mdf_bytes[0..3], b"MDF");
    }

    #[test]
    fn test_raw_lin_logger_tx_rx() {
        let mut logger = RawLinLogger::new().unwrap();

        assert!(logger.log_tx(0x20, 1000, &[0x01, 0x02]));
        assert!(logger.log_rx(0x20, 2000, &[0x03, 0x04]));
        assert!(logger.log_tx(0x21, 3000, &[0x05, 0x06]));

        assert_eq!(logger.tx_frame_count(), 2);
        assert_eq!(logger.rx_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_lin_logger_classic_checksum() {
        let mut logger = RawLinLogger::new().unwrap();

        assert!(logger.log_classic(0x20, 1000, &[0x01, 0x02, 0x03, 0x04]));

        assert_eq!(logger.total_frame_count(), 1);
        // Classic checksum should not set enhanced flag
        assert!(!logger.buffer[0].frame.flags.uses_enhanced_checksum());

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_lin_logger_empty() {
        let logger = RawLinLogger::new().unwrap();
        assert_eq!(logger.total_frame_count(), 0);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_lin_logger_frame_struct() {
        let mut logger = RawLinLogger::new().unwrap();

        let frame = LinFrame::with_enhanced_checksum(0x20, &[0x01, 0x02, 0x03, 0x04]).with_tx();
        assert!(logger.log_frame(1000, frame));

        assert_eq!(logger.total_frame_count(), 1);
        assert_eq!(logger.tx_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_source_name() {
        let logger = RawLinLogger::with_source_name("Body_LIN").unwrap();
        assert_eq!(logger.source_name, "Body_LIN");
    }

    #[test]
    fn test_bus_name_alias() {
        let logger = RawLinLogger::with_bus_name("Body_LIN").unwrap();
        assert_eq!(logger.source_name, "Body_LIN");
    }
}
