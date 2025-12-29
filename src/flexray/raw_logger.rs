//! Raw FlexRay frame logger following ASAM MDF4 Bus Logging specification.
//!
//! This module provides [`RawFlexRayLogger`], a logger for capturing raw FlexRay
//! frames to MDF4 files using the ASAM-compliant `FLEXRAY_Frame` format.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant format
//! - `FLEXRAY_Frame` channel with ByteArray
//! - Timestamp as Float64 in seconds
//! - Source metadata (FlexRay cluster name)
//! - Channel A/B/AB support
//! - Static and dynamic segment frames
//! - Startup and sync frame support
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::flexray::{RawFlexRayLogger, FlexRayFrame, FlexRayChannel};
//!
//! let mut logger = RawFlexRayLogger::new()?;
//!
//! // Log a FlexRay frame (takes ownership)
//! let frame = FlexRayFrame::channel_a(100, 5, payload.to_vec());
//! logger.log_frame(timestamp_us, frame);
//!
//! // Or log from components
//! logger.log(100, 5, FlexRayChannel::A, timestamp_us, &payload);
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use super::frame::{FLEXRAY_HEADER_SIZE, FlexRayChannel, FlexRayFrame, MAX_FLEXRAY_PAYLOAD};
use crate::bus_logging::{BusLoggerConfig, TimestampedFrame, init_bus_channel_group};

/// Raw FlexRay frame logger using ASAM MDF4 Bus Logging format.
///
/// This logger captures raw FlexRay frames using the industry-standard
/// `FLEXRAY_Frame` composite format.
///
/// ## Channel Group Structure
///
/// All FlexRay frames are stored in a single channel group:
/// - `{source_name}_FLEXRAY_Frame` - FlexRay frames with slot, cycle, channel, and payload
///
/// ## FLEXRAY_Frame Format
///
/// Each frame is stored as a ByteArray:
/// - Bytes 0-1: Slot ID (little-endian)
/// - Byte 2: Cycle count
/// - Byte 3: Channel (0=A, 1=B, 2=AB)
/// - Bytes 4-5: Flags (little-endian)
/// - Byte 6: Header CRC (low byte)
/// - Byte 7: Payload length
/// - Bytes 8+: Payload data (padded to max size)
pub struct RawFlexRayLogger<W: crate::writer::MdfWrite> {
    writer: crate::MdfWriter<W>,
    /// Source name for metadata
    source_name: String,
    /// Buffered frames
    buffer: Vec<TimestampedFrame<FlexRayFrame>>,
    /// Channel group ID
    channel_group: Option<String>,
    initialized: bool,
}

/// Frame size including header and max payload.
const FLEXRAY_FRAME_SIZE: usize = FLEXRAY_HEADER_SIZE + MAX_FLEXRAY_PAYLOAD;

impl RawFlexRayLogger<crate::writer::VecWriter> {
    /// Create a new raw FlexRay logger with in-memory output.
    pub fn new() -> crate::Result<Self> {
        Self::with_source_name("FlexRay")
    }

    /// Create a new raw FlexRay logger with a custom source name.
    ///
    /// The source name is used for channel group names and source metadata.
    /// Examples: "FlexRay", "Chassis_FR", "Powertrain_FR", etc.
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

    /// Create a new raw FlexRay logger with a custom cluster name.
    ///
    /// Alias for [`with_source_name`](Self::with_source_name) for API compatibility.
    pub fn with_cluster_name(cluster_name: &str) -> crate::Result<Self> {
        Self::with_source_name(cluster_name)
    }

    /// Create a new raw FlexRay logger with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> crate::Result<Self> {
        let writer =
            crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self {
            writer,
            source_name: String::from("FlexRay"),
            buffer: Vec::new(),
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
impl RawFlexRayLogger<crate::writer::FileWriter> {
    /// Create a new raw FlexRay logger that writes to a file.
    pub fn new_file(path: &str) -> crate::Result<Self> {
        Self::new_file_with_source_name(path, "FlexRay")
    }

    /// Create a new raw FlexRay logger that writes to a file with custom source name.
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

    /// Create a new raw FlexRay logger that writes to a file with custom cluster name.
    ///
    /// Alias for [`new_file_with_source_name`](Self::new_file_with_source_name) for API compatibility.
    pub fn new_file_with_cluster_name(path: &str, cluster_name: &str) -> crate::Result<Self> {
        Self::new_file_with_source_name(path, cluster_name)
    }

    /// Finalize and close the MDF file.
    pub fn finalize_file(mut self) -> crate::Result<()> {
        self.flush_and_finalize()
    }
}

impl<W: crate::writer::MdfWrite> RawFlexRayLogger<W> {
    /// Set the source name for metadata.
    ///
    /// Must be called before logging any frames.
    pub fn set_source_name(&mut self, name: &str) {
        self.source_name = String::from(name);
    }

    /// Set the FlexRay cluster name for source metadata.
    ///
    /// Alias for [`set_source_name`](Self::set_source_name) for API compatibility.
    pub fn set_cluster_name(&mut self, name: &str) {
        self.set_source_name(name);
    }

    /// Log a raw FlexRay frame.
    ///
    /// # Arguments
    /// * `slot_id` - Slot ID (1-2047)
    /// * `cycle` - Cycle count (0-63)
    /// * `channel` - Channel (A, B, or AB)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `payload` - Frame payload (up to 254 bytes)
    ///
    /// # Returns
    /// Always returns `true`
    pub fn log(
        &mut self,
        slot_id: u16,
        cycle: u8,
        channel: FlexRayChannel,
        timestamp_us: u64,
        payload: &[u8],
    ) -> bool {
        self.log_frame(
            timestamp_us,
            FlexRayFrame::new(slot_id, cycle, channel, payload.to_vec()),
        )
    }

    /// Log a FlexRay frame on channel A.
    pub fn log_channel_a(
        &mut self,
        slot_id: u16,
        cycle: u8,
        timestamp_us: u64,
        payload: &[u8],
    ) -> bool {
        self.log(slot_id, cycle, FlexRayChannel::A, timestamp_us, payload)
    }

    /// Log a FlexRay frame on channel B.
    pub fn log_channel_b(
        &mut self,
        slot_id: u16,
        cycle: u8,
        timestamp_us: u64,
        payload: &[u8],
    ) -> bool {
        self.log(slot_id, cycle, FlexRayChannel::B, timestamp_us, payload)
    }

    /// Log a transmitted FlexRay frame.
    pub fn log_tx(
        &mut self,
        slot_id: u16,
        cycle: u8,
        channel: FlexRayChannel,
        timestamp_us: u64,
        payload: &[u8],
    ) -> bool {
        self.log_frame(
            timestamp_us,
            FlexRayFrame::new(slot_id, cycle, channel, payload.to_vec()).with_tx(),
        )
    }

    /// Log a received FlexRay frame.
    pub fn log_rx(
        &mut self,
        slot_id: u16,
        cycle: u8,
        channel: FlexRayChannel,
        timestamp_us: u64,
        payload: &[u8],
    ) -> bool {
        self.log_frame(
            timestamp_us,
            FlexRayFrame::new(slot_id, cycle, channel, payload.to_vec()).with_rx(),
        )
    }

    /// Log a FlexRayFrame struct (takes ownership, avoids clone).
    ///
    /// # Arguments
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `frame` - The FlexRay frame to log (consumed)
    pub fn log_frame(&mut self, timestamp_us: u64, frame: FlexRayFrame) -> bool {
        self.buffer.push(TimestampedFrame::new(timestamp_us, frame));
        true
    }

    /// Log a FlexRayFrame struct by reference (clones the frame).
    ///
    /// Use [`log_frame`](Self::log_frame) if you don't need to reuse the frame.
    pub fn log_frame_ref(&mut self, timestamp_us: u64, frame: &FlexRayFrame) -> bool {
        self.log_frame(timestamp_us, frame.clone())
    }

    /// Log a null frame (no payload).
    pub fn log_null_frame(
        &mut self,
        slot_id: u16,
        cycle: u8,
        channel: FlexRayChannel,
        timestamp_us: u64,
    ) -> bool {
        self.log_frame(
            timestamp_us,
            FlexRayFrame::null_frame(slot_id, cycle, channel),
        )
    }

    /// Log a startup frame.
    pub fn log_startup(
        &mut self,
        slot_id: u16,
        cycle: u8,
        channel: FlexRayChannel,
        timestamp_us: u64,
        payload: &[u8],
    ) -> bool {
        self.log_frame(
            timestamp_us,
            FlexRayFrame::startup(slot_id, cycle, channel, payload.to_vec()),
        )
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
            group_name: alloc::format!("{}_FLEXRAY_Frame", self.source_name),
            data_channel_name: String::from("FLEXRAY_Frame"),
            data_channel_bits: (FLEXRAY_FRAME_SIZE * 8) as u32,
            source_block: crate::blocks::SourceBlock::flexray(),
        };

        let (cg, _data_ch) = init_bus_channel_group(&mut self.writer, &config)?;

        self.channel_group = Some(cg);
        self.initialized = true;
        Ok(())
    }

    /// Write frames to the MDF file.
    ///
    /// FlexRay frames are padded to max payload size for fixed record size.
    fn write_frames(&mut self) -> crate::Result<()> {
        use crate::DecodedValue;

        let cg = match &self.channel_group {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        self.writer.start_data_block_for_cg(&cg, 0)?;

        for entry in &self.buffer {
            // Pad frame bytes to fixed size
            let mut frame_bytes = entry.frame.to_bytes();
            frame_bytes.resize(FLEXRAY_FRAME_SIZE, 0);

            let values = [
                DecodedValue::Float(entry.timestamp_s),
                DecodedValue::ByteArray(frame_bytes),
            ];
            self.writer.write_record(&cg, &values)?;
        }

        self.writer.finish_data_block(&cg)?;
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

    /// Get the number of unique slot IDs.
    pub fn unique_slot_count(&self) -> usize {
        let mut slots = alloc::collections::BTreeSet::new();
        for entry in &self.buffer {
            slots.insert(entry.frame.slot_id);
        }
        slots.len()
    }

    /// Get the number of frames for a specific slot ID.
    pub fn frame_count_for_slot(&self, slot_id: u16) -> usize {
        self.buffer
            .iter()
            .filter(|e| e.frame.slot_id == slot_id)
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

    /// Get count of channel A frames (includes AB).
    pub fn channel_a_count(&self) -> usize {
        self.buffer
            .iter()
            .filter(|e| matches!(e.frame.channel, FlexRayChannel::A | FlexRayChannel::AB))
            .count()
    }

    /// Get count of channel B frames (includes AB).
    pub fn channel_b_count(&self) -> usize {
        self.buffer
            .iter()
            .filter(|e| matches!(e.frame.channel, FlexRayChannel::B | FlexRayChannel::AB))
            .count()
    }

    /// Get count of startup frames.
    pub fn startup_frame_count(&self) -> usize {
        self.buffer
            .iter()
            .filter(|e| e.frame.flags.is_startup())
            .count()
    }

    /// Get count of null frames.
    pub fn null_frame_count(&self) -> usize {
        self.buffer
            .iter()
            .filter(|e| e.frame.flags.is_null_frame())
            .count()
    }

    /// Get count of frames for a specific channel.
    pub fn channel_frame_count(&self, channel: FlexRayChannel) -> usize {
        self.buffer
            .iter()
            .filter(|e| e.frame.channel == channel)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_flexray_logger_basic() {
        let mut logger = RawFlexRayLogger::new().unwrap();

        assert!(logger.log(100, 0, FlexRayChannel::A, 1000, &[0x01, 0x02, 0x03, 0x04]));
        assert!(logger.log(101, 1, FlexRayChannel::B, 2000, &[0x05, 0x06]));
        assert!(logger.log(100, 2, FlexRayChannel::A, 3000, &[0x07, 0x08, 0x09]));

        assert_eq!(logger.total_frame_count(), 3);
        assert_eq!(logger.frame_count_for_slot(100), 2);
        assert_eq!(logger.frame_count_for_slot(101), 1);
        assert_eq!(logger.unique_slot_count(), 2);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
        assert_eq!(&mdf_bytes[0..3], b"MDF");
    }

    #[test]
    fn test_raw_flexray_logger_channels() {
        let mut logger = RawFlexRayLogger::new().unwrap();

        assert!(logger.log_channel_a(100, 0, 1000, &[0x01]));
        assert!(logger.log_channel_b(101, 0, 2000, &[0x02]));
        assert!(logger.log(102, 0, FlexRayChannel::AB, 3000, &[0x03]));

        assert_eq!(logger.channel_a_count(), 2); // A and AB
        assert_eq!(logger.channel_b_count(), 2); // B and AB

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_flexray_logger_tx_rx() {
        let mut logger = RawFlexRayLogger::new().unwrap();

        assert!(logger.log_tx(100, 0, FlexRayChannel::A, 1000, &[0x01]));
        assert!(logger.log_rx(100, 1, FlexRayChannel::A, 2000, &[0x02]));
        assert!(logger.log_tx(100, 2, FlexRayChannel::A, 3000, &[0x03]));

        assert_eq!(logger.tx_frame_count(), 2);
        assert_eq!(logger.rx_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_flexray_logger_special_frames() {
        let mut logger = RawFlexRayLogger::new().unwrap();

        assert!(logger.log_null_frame(50, 0, FlexRayChannel::A, 1000));
        assert!(logger.log_startup(1, 0, FlexRayChannel::AB, 0, &[0x00; 8]));

        assert_eq!(logger.null_frame_count(), 1);
        assert_eq!(logger.startup_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_flexray_logger_empty() {
        let logger = RawFlexRayLogger::new().unwrap();
        assert_eq!(logger.total_frame_count(), 0);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_flexray_logger_frame_struct() {
        let mut logger = RawFlexRayLogger::new().unwrap();

        let frame = FlexRayFrame::channel_a(100, 5, vec![0xAA, 0xBB, 0xCC, 0xDD])
            .with_tx()
            .with_dynamic();
        assert!(logger.log_frame(1000, frame));

        assert_eq!(logger.total_frame_count(), 1);
        assert_eq!(logger.tx_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_source_name() {
        let logger = RawFlexRayLogger::with_source_name("Chassis_FR").unwrap();
        assert_eq!(logger.source_name, "Chassis_FR");
    }

    #[test]
    fn test_cluster_name_alias() {
        let logger = RawFlexRayLogger::with_cluster_name("Chassis_FR").unwrap();
        assert_eq!(logger.source_name, "Chassis_FR");
    }
}
