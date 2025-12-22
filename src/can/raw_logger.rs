//! Raw CAN frame logger following ASAM MDF4 Bus Logging specification.
//!
//! This module provides [`RawCanLogger`], a logger for capturing raw CAN frames
//! to MDF4 files using the ASAM-compliant `CAN_DataFrame` format.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant format
//! - `CAN_DataFrame` channel with composite ByteArray (ID + DLC + Data)
//! - Timestamp as Float64 in seconds
//! - Supports both Standard (11-bit) and Extended (29-bit) CAN IDs
//! - CAN FD support with BRS/ESI flags
//! - Source metadata (CAN bus name/path)
//! - Compatible with Vector CANalyzer, PEAK tools, CSS Electronics, etc.
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::can::RawCanLogger;
//!
//! let mut logger = RawCanLogger::new()?;
//!
//! // Log raw CAN frames (classic or FD)
//! logger.log(0x100, timestamp_us, &[0x01, 0x02, 0x03, 0x04]);
//!
//! // Log extended 29-bit ID frame
//! logger.log_extended(0x18FEF100, timestamp_us, &data);
//!
//! // Log CAN FD frame with flags
//! use mdf4_rs::can::FdFlags;
//! logger.log_fd(0x200, timestamp_us, &fd_data, FdFlags::new(true, false));
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "can")]
use super::fd::FdFrame;
use super::fd::{FdFlags, MAX_FD_DATA_LEN};

/// Frame type classification for ASAM channel grouping.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum FrameType {
    /// Classic CAN with 11-bit standard ID
    Classic,
    /// Classic CAN with 29-bit extended ID
    ClassicExtended,
    /// CAN FD with 11-bit standard ID, DLC <= 8
    FdSmall,
    /// CAN FD with 29-bit extended ID, DLC <= 8
    FdSmallExtended,
    /// CAN FD with 11-bit standard ID, DLC > 8
    FdLarge,
    /// CAN FD with 29-bit extended ID, DLC > 8
    FdLargeExtended,
}

impl FrameType {
    fn group_name(&self, bus_name: &str) -> String {
        match self {
            FrameType::Classic => alloc::format!("{}_DataFrame", bus_name),
            FrameType::ClassicExtended => alloc::format!("{}_DataFrame_IDE", bus_name),
            FrameType::FdSmall => alloc::format!("{}_DataFrame_FD", bus_name),
            FrameType::FdSmallExtended => alloc::format!("{}_DataFrame_FD_IDE", bus_name),
            FrameType::FdLarge => alloc::format!("{}_DataFrame_FD_DLC_over_8", bus_name),
            FrameType::FdLargeExtended => {
                alloc::format!("{}_DataFrame_FD_IDE_DLC_over_8", bus_name)
            }
        }
    }

    fn channel_name(&self) -> &'static str {
        "CAN_DataFrame"
    }

    fn max_data_len(&self) -> usize {
        match self {
            FrameType::Classic | FrameType::ClassicExtended => 8,
            FrameType::FdSmall | FrameType::FdSmallExtended => 8,
            FrameType::FdLarge | FrameType::FdLargeExtended => 64,
        }
    }

    fn from_frame(is_extended: bool, is_fd: bool, data_len: usize) -> Self {
        match (is_extended, is_fd, data_len > 8) {
            (false, false, _) => FrameType::Classic,
            (true, false, _) => FrameType::ClassicExtended,
            (false, true, false) => FrameType::FdSmall,
            (true, true, false) => FrameType::FdSmallExtended,
            (false, true, true) => FrameType::FdLarge,
            (true, true, true) => FrameType::FdLargeExtended,
        }
    }
}

/// A buffered raw CAN frame in ASAM format.
#[derive(Clone)]
struct RawFrame {
    /// Timestamp in seconds (ASAM uses float64 seconds)
    timestamp_s: f64,
    /// CAN ID (11 or 29 bits)
    can_id: u32,
    /// Data Length Code
    dlc: u8,
    /// Frame data
    data: [u8; MAX_FD_DATA_LEN],
    /// Actual data length
    data_len: usize,
    /// CAN FD flags (BRS, ESI)
    fd_flags: FdFlags,
    /// True if this frame uses a 29-bit extended ID
    is_extended: bool,
    /// True if this is a CAN FD frame
    is_fd: bool,
}

impl RawFrame {
    fn new_classic(
        timestamp_us: u64,
        can_id: u32,
        dlc: u8,
        data: &[u8],
        is_extended: bool,
    ) -> Self {
        let mut frame_data = [0u8; MAX_FD_DATA_LEN];
        let len = data.len().min(8);
        frame_data[..len].copy_from_slice(&data[..len]);
        Self {
            timestamp_s: timestamp_us as f64 / 1_000_000.0,
            can_id,
            dlc,
            data: frame_data,
            data_len: len,
            fd_flags: FdFlags::default(),
            is_extended,
            is_fd: false,
        }
    }

    fn new_fd(
        timestamp_us: u64,
        can_id: u32,
        dlc: u8,
        data: &[u8],
        flags: FdFlags,
        is_extended: bool,
    ) -> Self {
        let mut frame_data = [0u8; MAX_FD_DATA_LEN];
        let len = data.len().min(MAX_FD_DATA_LEN);
        frame_data[..len].copy_from_slice(&data[..len]);
        Self {
            timestamp_s: timestamp_us as f64 / 1_000_000.0,
            can_id,
            dlc,
            data: frame_data,
            data_len: len,
            fd_flags: flags,
            is_extended,
            is_fd: true,
        }
    }

    fn frame_type(&self) -> FrameType {
        FrameType::from_frame(self.is_extended, self.is_fd, self.data_len)
    }

    /// Build the CAN_DataFrame ByteArray in ASAM format.
    ///
    /// Format for classic CAN (13 bytes):
    /// - Bytes 0-3: CAN ID (little-endian, bit 31 set for extended ID)
    /// - Byte 4: DLC
    /// - Bytes 5-12: Data (8 bytes, zero-padded)
    ///
    /// Format for CAN FD with DLC > 8:
    /// - Bytes 0-3: CAN ID (little-endian, bit 31 set for extended ID)
    /// - Byte 4: DLC
    /// - Byte 5: FD flags (bit 0 = BRS, bit 1 = ESI)
    /// - Bytes 6+: Data (up to 64 bytes)
    fn to_dataframe_bytes(&self) -> Vec<u8> {
        let max_data = self.frame_type().max_data_len();
        let has_fd_flags = self.is_fd && self.data_len > 8;

        // Calculate total size: ID(4) + DLC(1) + [FD_flags(1)] + Data(max_data)
        let total_size = if has_fd_flags {
            4 + 1 + 1 + max_data
        } else {
            4 + 1 + max_data
        };

        let mut bytes = Vec::with_capacity(total_size);

        // CAN ID with extended flag in bit 31
        let id_with_flags = if self.is_extended {
            self.can_id | 0x8000_0000
        } else {
            self.can_id
        };
        bytes.extend_from_slice(&id_with_flags.to_le_bytes());

        // DLC
        bytes.push(self.dlc);

        // FD flags (only for FD frames with DLC > 8)
        if has_fd_flags {
            bytes.push(self.fd_flags.to_byte());
        }

        // Data (zero-padded to max_data)
        bytes.extend_from_slice(&self.data[..self.data_len.min(max_data)]);
        bytes.resize(total_size, 0);

        bytes
    }
}

/// Raw CAN frame logger using ASAM MDF4 Bus Logging format.
///
/// This logger captures raw CAN frames using the industry-standard
/// `CAN_DataFrame` composite format, compatible with tools like:
/// - Vector CANalyzer
/// - PEAK PCAN-View
/// - CSS Electronics MDF4 Converters
/// - asammdf (Python)
///
/// ## Channel Group Structure
///
/// Frames are organized into channel groups by type:
/// - `CAN_DataFrame` - Standard 11-bit ID, classic CAN
/// - `CAN_DataFrame_IDE` - Extended 29-bit ID, classic CAN
/// - `CAN_DataFrame_FD` - Standard ID, CAN FD (DLC <= 8)
/// - `CAN_DataFrame_FD_IDE` - Extended ID, CAN FD (DLC <= 8)
/// - `CAN_DataFrame_FD_DLC_over_8` - Standard ID, CAN FD (DLC > 8)
/// - `CAN_DataFrame_FD_IDE_DLC_over_8` - Extended ID, CAN FD (DLC > 8)
///
/// ## CAN_DataFrame Format
///
/// Each frame is stored as a ByteArray:
/// - Bytes 0-3: CAN ID (little-endian, bit 31 = extended flag)
/// - Byte 4: DLC
/// - Bytes 5+: Data (padded to 8 or 64 bytes)
pub struct RawCanLogger<W: crate::writer::MdfWrite> {
    writer: crate::MdfWriter<W>,
    /// CAN bus name for source metadata
    bus_name: String,
    /// Buffered frames by type
    buffers: alloc::collections::BTreeMap<FrameType, Vec<RawFrame>>,
    /// Channel group IDs by frame type
    channel_groups: alloc::collections::BTreeMap<FrameType, String>,
    initialized: bool,
}

impl RawCanLogger<crate::writer::VecWriter> {
    /// Create a new raw CAN logger with in-memory output.
    pub fn new() -> crate::Result<Self> {
        Self::with_bus_name("CAN")
    }

    /// Create a new raw CAN logger with a custom bus name.
    ///
    /// The bus name is used for channel group names and source metadata.
    /// Examples: "CAN", "CAN1", "Vehicle_CAN", etc.
    pub fn with_bus_name(bus_name: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());
        Ok(Self {
            writer,
            bus_name: String::from(bus_name),
            buffers: alloc::collections::BTreeMap::new(),
            channel_groups: alloc::collections::BTreeMap::new(),
            initialized: false,
        })
    }

    /// Create a new raw CAN logger with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> crate::Result<Self> {
        let writer =
            crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self {
            writer,
            bus_name: String::from("CAN"),
            buffers: alloc::collections::BTreeMap::new(),
            channel_groups: alloc::collections::BTreeMap::new(),
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
impl RawCanLogger<crate::writer::FileWriter> {
    /// Create a new raw CAN logger that writes to a file.
    pub fn new_file(path: &str) -> crate::Result<Self> {
        Self::new_file_with_bus_name(path, "CAN")
    }

    /// Create a new raw CAN logger that writes to a file with custom bus name.
    pub fn new_file_with_bus_name(path: &str, bus_name: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::new(path)?;
        Ok(Self {
            writer,
            bus_name: String::from(bus_name),
            buffers: alloc::collections::BTreeMap::new(),
            channel_groups: alloc::collections::BTreeMap::new(),
            initialized: false,
        })
    }

    /// Finalize and close the MDF file.
    pub fn finalize_file(mut self) -> crate::Result<()> {
        self.flush_and_finalize()
    }
}

impl<W: crate::writer::MdfWrite> RawCanLogger<W> {
    /// Set the CAN bus name for source metadata.
    ///
    /// Must be called before logging any frames.
    pub fn set_bus_name(&mut self, name: &str) {
        self.bus_name = String::from(name);
    }

    /// Log a raw CAN frame with standard 11-bit ID (classic CAN, up to 8 bytes).
    ///
    /// # Arguments
    /// * `can_id` - The CAN message ID (11-bit standard)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Raw frame data (up to 8 bytes for classic CAN)
    ///
    /// # Returns
    /// Always returns `true` (raw logging never rejects frames)
    #[inline]
    pub fn log(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        let dlc = data.len().min(8) as u8;
        let frame = RawFrame::new_classic(timestamp_us, can_id, dlc, data, false);
        self.buffers
            .entry(frame.frame_type())
            .or_default()
            .push(frame);
        true
    }

    /// Log a raw CAN frame with extended 29-bit ID (classic CAN, up to 8 bytes).
    ///
    /// # Arguments
    /// * `can_id` - The CAN message ID (29-bit extended)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Raw frame data (up to 8 bytes for classic CAN)
    ///
    /// # Returns
    /// Always returns `true` (raw logging never rejects frames)
    #[inline]
    pub fn log_extended(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        let dlc = data.len().min(8) as u8;
        let frame = RawFrame::new_classic(timestamp_us, can_id, dlc, data, true);
        self.buffers
            .entry(frame.frame_type())
            .or_default()
            .push(frame);
        true
    }

    /// Log a CAN FD frame with standard 11-bit ID (up to 64 bytes).
    ///
    /// # Arguments
    /// * `can_id` - The CAN message ID (11-bit standard)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Raw frame data (up to 64 bytes for CAN FD)
    /// * `flags` - CAN FD flags (BRS, ESI)
    ///
    /// # Returns
    /// Always returns `true` (raw logging never rejects frames)
    #[inline]
    pub fn log_fd(&mut self, can_id: u32, timestamp_us: u64, data: &[u8], flags: FdFlags) -> bool {
        let dlc = super::fd::len_to_dlc(data.len());
        let frame = RawFrame::new_fd(timestamp_us, can_id, dlc, data, flags, false);
        self.buffers
            .entry(frame.frame_type())
            .or_default()
            .push(frame);
        true
    }

    /// Log a CAN FD frame with extended 29-bit ID (up to 64 bytes).
    ///
    /// # Arguments
    /// * `can_id` - The CAN message ID (29-bit extended)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Raw frame data (up to 64 bytes for CAN FD)
    /// * `flags` - CAN FD flags (BRS, ESI)
    ///
    /// # Returns
    /// Always returns `true` (raw logging never rejects frames)
    #[inline]
    pub fn log_fd_extended(
        &mut self,
        can_id: u32,
        timestamp_us: u64,
        data: &[u8],
        flags: FdFlags,
    ) -> bool {
        let dlc = super::fd::len_to_dlc(data.len());
        let frame = RawFrame::new_fd(timestamp_us, can_id, dlc, data, flags, true);
        self.buffers
            .entry(frame.frame_type())
            .or_default()
            .push(frame);
        true
    }

    /// Log an embedded-can frame.
    ///
    /// Automatically detects Standard vs Extended ID from the frame.
    #[cfg(feature = "can")]
    #[inline]
    pub fn log_frame<F: embedded_can::Frame>(&mut self, timestamp_us: u64, frame: &F) -> bool {
        match frame.id() {
            embedded_can::Id::Standard(id) => {
                self.log(id.as_raw() as u32, timestamp_us, frame.data())
            }
            embedded_can::Id::Extended(id) => {
                self.log_extended(id.as_raw(), timestamp_us, frame.data())
            }
        }
    }

    /// Log a CAN FD frame using the FdFrame trait.
    ///
    /// Automatically detects Standard vs Extended ID from the frame.
    #[cfg(feature = "can")]
    #[inline]
    pub fn log_fd_frame<F: FdFrame>(&mut self, timestamp_us: u64, frame: &F) -> bool {
        if frame.is_fd() {
            match frame.id() {
                embedded_can::Id::Standard(id) => self.log_fd(
                    id.as_raw() as u32,
                    timestamp_us,
                    frame.data(),
                    frame.fd_flags(),
                ),
                embedded_can::Id::Extended(id) => {
                    self.log_fd_extended(id.as_raw(), timestamp_us, frame.data(), frame.fd_flags())
                }
            }
        } else {
            match frame.id() {
                embedded_can::Id::Standard(id) => {
                    self.log(id.as_raw() as u32, timestamp_us, frame.data())
                }
                embedded_can::Id::Extended(id) => {
                    self.log_extended(id.as_raw(), timestamp_us, frame.data())
                }
            }
        }
    }

    /// Flush buffered data to the MDF writer.
    pub fn flush(&mut self) -> crate::Result<()> {
        if !self.initialized {
            self.initialize_mdf()?;
        }

        // Write data for each frame type
        for frame_type in self.buffers.keys().copied().collect::<Vec<_>>() {
            self.write_frames(frame_type)?;
        }

        // Clear all buffers
        for buffer in self.buffers.values_mut() {
            buffer.clear();
        }

        Ok(())
    }

    /// Initialize the MDF file structure with ASAM-compliant channel groups.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        use crate::DataType;

        self.writer.init_mdf_file()?;

        // Create a channel group for each frame type that has data
        for &frame_type in self.buffers.keys() {
            let group_name = frame_type.group_name(&self.bus_name);
            let max_data_len = frame_type.max_data_len();

            // Calculate CAN_DataFrame size: ID(4) + DLC(1) + Data(max_data_len)
            // For FD with large data, add FD_flags byte
            let dataframe_size =
                if matches!(frame_type, FrameType::FdLarge | FrameType::FdLargeExtended) {
                    4 + 1 + 1 + max_data_len // ID + DLC + FD_flags + Data
                } else {
                    4 + 1 + max_data_len // ID + DLC + Data
                };

            let cg = self.writer.add_channel_group(None, |_| {})?;
            self.writer.set_channel_group_name(&cg, &group_name)?;

            // Set source information (ASAM requires this for bus logging)
            let source = crate::blocks::SourceBlock::can_bus();
            self.writer
                .set_channel_group_source(&cg, &source, Some(&self.bus_name))?;

            // Add Timestamp channel (Float64 in seconds - ASAM standard)
            let time_ch = self.writer.add_channel(&cg, None, |ch| {
                ch.data_type = DataType::FloatLE;
                ch.name = Some(alloc::string::String::from("Timestamp"));
                ch.bit_count = 64;
            })?;
            self.writer.set_time_channel(&time_ch)?;
            self.writer.set_channel_unit(&time_ch, "s")?;

            // Add CAN_DataFrame channel (ByteArray - ASAM composite format)
            let _df_ch = self.writer.add_channel(&cg, Some(&time_ch), |ch| {
                ch.data_type = DataType::ByteArray;
                ch.name = Some(alloc::string::String::from(frame_type.channel_name()));
                ch.bit_count = (dataframe_size * 8) as u32;
            })?;

            self.channel_groups.insert(frame_type, cg);
        }

        self.initialized = true;
        Ok(())
    }

    /// Write frames for a specific frame type.
    fn write_frames(&mut self, frame_type: FrameType) -> crate::Result<()> {
        use crate::DecodedValue;

        let cg = match self.channel_groups.get(&frame_type) {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        let frames = match self.buffers.get(&frame_type) {
            Some(f) if !f.is_empty() => f,
            _ => return Ok(()),
        };

        self.writer.start_data_block_for_cg(&cg, 0)?;

        for frame in frames {
            let values = [
                DecodedValue::Float(frame.timestamp_s),
                DecodedValue::ByteArray(frame.to_dataframe_bytes()),
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

    /// Get the number of frames logged for a specific CAN ID.
    pub fn frame_count_for_id(&self, can_id: u32) -> usize {
        self.buffers
            .values()
            .flat_map(|frames| frames.iter())
            .filter(|f| f.can_id == can_id)
            .count()
    }

    /// Get the total number of frames logged.
    pub fn total_frame_count(&self) -> usize {
        self.buffers.values().map(|b| b.len()).sum()
    }

    /// Get the number of unique CAN IDs.
    pub fn unique_id_count(&self) -> usize {
        let mut ids = alloc::collections::BTreeSet::new();
        for frames in self.buffers.values() {
            for frame in frames {
                ids.insert(frame.can_id);
            }
        }
        ids.len()
    }

    /// Check if any CAN FD frames have been logged.
    pub fn has_fd_frames(&self) -> bool {
        self.buffers.keys().any(|ft| {
            matches!(
                ft,
                FrameType::FdSmall
                    | FrameType::FdSmallExtended
                    | FrameType::FdLarge
                    | FrameType::FdLargeExtended
            )
        })
    }

    /// Check if any extended 29-bit ID frames have been logged.
    pub fn has_extended_frames(&self) -> bool {
        self.buffers.keys().any(|ft| {
            matches!(
                ft,
                FrameType::ClassicExtended
                    | FrameType::FdSmallExtended
                    | FrameType::FdLargeExtended
            )
        })
    }

    /// Get count of standard 11-bit ID frames.
    pub fn standard_frame_count(&self) -> usize {
        self.buffers
            .iter()
            .filter(|(ft, _)| {
                matches!(
                    ft,
                    FrameType::Classic | FrameType::FdSmall | FrameType::FdLarge
                )
            })
            .map(|(_, frames)| frames.len())
            .sum()
    }

    /// Get count of extended 29-bit ID frames.
    pub fn extended_frame_count(&self) -> usize {
        self.buffers
            .iter()
            .filter(|(ft, _)| {
                matches!(
                    ft,
                    FrameType::ClassicExtended
                        | FrameType::FdSmallExtended
                        | FrameType::FdLargeExtended
                )
            })
            .map(|(_, frames)| frames.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_can_logger_basic() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log some frames
        assert!(logger.log(0x100, 1000, &[0x01, 0x02, 0x03, 0x04]));
        assert!(logger.log(0x100, 2000, &[0x05, 0x06, 0x07, 0x08]));
        assert!(logger.log(0x200, 1500, &[0xAA, 0xBB]));

        assert_eq!(logger.frame_count_for_id(0x100), 2);
        assert_eq!(logger.frame_count_for_id(0x200), 1);
        assert_eq!(logger.total_frame_count(), 3);
        assert_eq!(logger.unique_id_count(), 2);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
        assert_eq!(&mdf_bytes[0..3], b"MDF");
    }

    #[test]
    fn test_raw_can_logger_extended_id() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log extended ID (29-bit)
        let extended_id = 0x1234_5678;
        assert!(logger.log_extended(
            extended_id,
            1000,
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        ));

        assert_eq!(logger.frame_count_for_id(extended_id), 1);
        assert!(logger.has_extended_frames());
        assert_eq!(logger.extended_frame_count(), 1);
        assert_eq!(logger.standard_frame_count(), 0);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_can_logger_mixed_standard_and_extended() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log standard 11-bit ID
        assert!(logger.log(0x100, 1000, &[0x01, 0x02, 0x03, 0x04]));
        assert!(!logger.has_extended_frames());

        // Log extended 29-bit ID
        assert!(logger.log_extended(0x18FEF100, 2000, &[0xAA, 0xBB, 0xCC, 0xDD]));
        assert!(logger.has_extended_frames());

        assert_eq!(logger.standard_frame_count(), 1);
        assert_eq!(logger.extended_frame_count(), 1);
        assert_eq!(logger.unique_id_count(), 2);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_can_logger_extended_fd() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log extended CAN FD frame (29-bit ID with 32 bytes and BRS)
        let fd_data: [u8; 32] = [0x55; 32];
        let flags = FdFlags::new(true, false);
        assert!(logger.log_fd_extended(0x18DA00F1, 1000, &fd_data, flags));

        assert!(logger.has_extended_frames());
        assert!(logger.has_fd_frames());
        assert_eq!(logger.extended_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_can_logger_empty() {
        let logger = RawCanLogger::new().unwrap();
        assert_eq!(logger.total_frame_count(), 0);
        assert_eq!(logger.unique_id_count(), 0);

        let mdf_bytes = logger.finalize().unwrap();
        // Even empty file should have MDF header
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_can_logger_fd_basic() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log a CAN FD frame with 32 bytes and BRS flag
        let fd_data: [u8; 32] = [0xAA; 32];
        let flags = FdFlags::new(true, false);
        assert!(logger.log_fd(0x100, 1000, &fd_data, flags));

        assert_eq!(logger.frame_count_for_id(0x100), 1);
        assert!(logger.has_fd_frames());

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
        assert_eq!(&mdf_bytes[0..3], b"MDF");
    }

    #[test]
    fn test_raw_can_logger_fd_64_bytes() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log a maximum size CAN FD frame (64 bytes)
        let fd_data: [u8; 64] = core::array::from_fn(|i| i as u8);
        let flags = FdFlags::new(true, true); // BRS and ESI
        assert!(logger.log_fd(0x200, 2000, &fd_data, flags));

        assert_eq!(logger.frame_count_for_id(0x200), 1);
        assert!(logger.has_fd_frames());

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_can_logger_mixed_classic_and_fd() {
        let mut logger = RawCanLogger::new().unwrap();

        // Log classic CAN frame (8 bytes)
        assert!(logger.log(0x100, 1000, &[1, 2, 3, 4, 5, 6, 7, 8]));
        assert!(!logger.has_fd_frames());

        // Log CAN FD frame (24 bytes)
        let fd_data: [u8; 24] = [0xBB; 24];
        assert!(logger.log_fd(0x200, 2000, &fd_data, FdFlags::default()));
        assert!(logger.has_fd_frames());

        // Log another classic CAN frame
        assert!(logger.log(0x100, 3000, &[9, 10, 11, 12]));

        assert_eq!(logger.frame_count_for_id(0x100), 2);
        assert_eq!(logger.frame_count_for_id(0x200), 1);
        assert_eq!(logger.total_frame_count(), 3);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_dataframe_format() {
        // Test the CAN_DataFrame byte layout
        let frame = RawFrame::new_classic(1_000_000, 0x123, 4, &[0xAA, 0xBB, 0xCC, 0xDD], false);
        let bytes = frame.to_dataframe_bytes();

        // ID: 0x123 in little-endian
        assert_eq!(bytes[0], 0x23);
        assert_eq!(bytes[1], 0x01);
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x00);
        // DLC
        assert_eq!(bytes[4], 4);
        // Data (padded to 8)
        assert_eq!(bytes[5], 0xAA);
        assert_eq!(bytes[6], 0xBB);
        assert_eq!(bytes[7], 0xCC);
        assert_eq!(bytes[8], 0xDD);
        assert_eq!(bytes[9], 0x00); // padding
    }

    #[test]
    fn test_dataframe_format_extended() {
        // Test extended ID with bit 31 set
        let frame =
            RawFrame::new_classic(1_000_000, 0x18FEF100, 8, &[1, 2, 3, 4, 5, 6, 7, 8], true);
        let bytes = frame.to_dataframe_bytes();

        // ID: 0x18FEF100 | 0x80000000 = 0x98FEF100 in little-endian
        let id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(id, 0x98FEF100);
        assert!(id & 0x8000_0000 != 0); // Extended flag set
    }

    #[test]
    fn test_fd_flags() {
        let flags = FdFlags::new(true, false);
        assert!(flags.brs());
        assert!(!flags.esi());

        let flags = FdFlags::new(false, true);
        assert!(!flags.brs());
        assert!(flags.esi());

        let flags = FdFlags::from_byte(0x03);
        assert!(flags.brs());
        assert!(flags.esi());
        assert_eq!(flags.to_byte(), 0x03);
    }

    #[test]
    fn test_bus_name() {
        let logger = RawCanLogger::with_bus_name("Vehicle_CAN").unwrap();
        assert_eq!(logger.bus_name, "Vehicle_CAN");
    }
}
