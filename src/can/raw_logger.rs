//! Raw CAN frame logger without DBC decoding.
//!
//! This module provides [`RawCanLogger`], a simple logger for capturing raw CAN
//! frames to MDF4 files when no DBC file is available.
//!
//! # Features
//!
//! - Logs raw CAN frames without signal decoding
//! - Stores timestamp, CAN ID, DLC, and raw data bytes
//! - Supports both Standard (11-bit) and Extended (29-bit) CAN IDs
//! - Compatible with embedded-can Frame trait
//! - **CAN FD support**: Up to 64 bytes per frame with BRS/ESI flags
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
//! // Log CAN FD frame with flags
//! use mdf4_rs::can::FdFlags;
//! logger.log_fd(0x200, timestamp_us, &fd_data, FdFlags::new(true, false));
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::fd::{FdFlags, FdFrame, MAX_FD_DATA_LEN};

/// A buffered raw CAN frame.
#[derive(Clone)]
struct RawFrame {
    timestamp_us: u64,
    dlc: u8,
    data: [u8; MAX_FD_DATA_LEN],
    data_len: usize,
    fd_flags: FdFlags,
}

impl RawFrame {
    fn new_classic(timestamp_us: u64, dlc: u8, data: &[u8]) -> Self {
        let mut frame_data = [0u8; MAX_FD_DATA_LEN];
        let len = data.len().min(8);
        frame_data[..len].copy_from_slice(&data[..len]);
        Self {
            timestamp_us,
            dlc,
            data: frame_data,
            data_len: len,
            fd_flags: FdFlags::default(),
        }
    }

    fn new_fd(timestamp_us: u64, dlc: u8, data: &[u8], flags: FdFlags) -> Self {
        let mut frame_data = [0u8; MAX_FD_DATA_LEN];
        let len = data.len().min(MAX_FD_DATA_LEN);
        frame_data[..len].copy_from_slice(&data[..len]);
        Self {
            timestamp_us,
            dlc,
            data: frame_data,
            data_len: len,
            fd_flags: flags,
        }
    }
}

/// Raw CAN frame logger for MDF4 files.
///
/// This logger captures raw CAN frames without any DBC-based signal decoding.
/// Each CAN ID gets its own channel group with channels for:
/// - Timestamp (microseconds)
/// - CAN ID
/// - DLC (Data Length Code)
/// - FD_Flags (BRS/ESI for CAN FD frames)
/// - Data bytes (up to 64 bytes for CAN FD)
///
/// Use this when no DBC file is available and you want to capture
/// raw CAN bus traffic for later analysis.
pub struct RawCanLogger<W: crate::writer::MdfWrite> {
    writer: crate::MdfWriter<W>,
    /// Maps CAN ID to channel group ID
    channel_groups: BTreeMap<u32, String>,
    /// Buffered frames per CAN ID
    buffers: BTreeMap<u32, Vec<RawFrame>>,
    initialized: bool,
    /// Whether to create separate channel groups per CAN ID
    group_by_id: bool,
    /// Maximum data length seen (for determining number of data channels)
    max_data_len: usize,
}

impl RawCanLogger<crate::writer::VecWriter> {
    /// Create a new raw CAN logger with in-memory output.
    pub fn new() -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());
        Ok(Self {
            writer,
            channel_groups: BTreeMap::new(),
            buffers: BTreeMap::new(),
            initialized: false,
            group_by_id: true,
            max_data_len: 8, // Default to classic CAN
        })
    }

    /// Create a new raw CAN logger with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self {
            writer,
            channel_groups: BTreeMap::new(),
            buffers: BTreeMap::new(),
            initialized: false,
            group_by_id: true,
            max_data_len: 8,
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
        let writer = crate::MdfWriter::new(path)?;
        Ok(Self {
            writer,
            channel_groups: BTreeMap::new(),
            buffers: BTreeMap::new(),
            initialized: false,
            group_by_id: true,
            max_data_len: 8,
        })
    }

    /// Finalize and close the MDF file.
    pub fn finalize_file(mut self) -> crate::Result<()> {
        self.flush_and_finalize()
    }
}

impl<W: crate::writer::MdfWrite> RawCanLogger<W> {
    /// Log a raw CAN frame (classic CAN, up to 8 bytes).
    ///
    /// # Arguments
    /// * `can_id` - The CAN message ID (11-bit or 29-bit)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Raw frame data (up to 8 bytes for classic CAN)
    ///
    /// # Returns
    /// Always returns `true` (raw logging never rejects frames)
    #[inline]
    pub fn log(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        let dlc = data.len().min(8) as u8;
        let frame = RawFrame::new_classic(timestamp_us, dlc, data);

        self.buffers
            .entry(can_id)
            .or_insert_with(Vec::new)
            .push(frame);
        true
    }

    /// Log a CAN FD frame (up to 64 bytes).
    ///
    /// # Arguments
    /// * `can_id` - The CAN message ID (11-bit or 29-bit)
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `data` - Raw frame data (up to 64 bytes for CAN FD)
    /// * `flags` - CAN FD flags (BRS, ESI)
    ///
    /// # Returns
    /// Always returns `true` (raw logging never rejects frames)
    #[inline]
    pub fn log_fd(&mut self, can_id: u32, timestamp_us: u64, data: &[u8], flags: FdFlags) -> bool {
        let dlc = super::fd::len_to_dlc(data.len());
        let frame = RawFrame::new_fd(timestamp_us, dlc, data, flags);

        // Track max data length for channel creation
        if data.len() > self.max_data_len {
            self.max_data_len = data.len().min(MAX_FD_DATA_LEN);
        }

        self.buffers
            .entry(can_id)
            .or_insert_with(Vec::new)
            .push(frame);
        true
    }

    /// Log an embedded-can frame.
    #[cfg(feature = "can")]
    #[inline]
    pub fn log_frame<F: embedded_can::Frame>(&mut self, timestamp_us: u64, frame: &F) -> bool {
        let can_id = match frame.id() {
            embedded_can::Id::Standard(id) => id.as_raw() as u32,
            embedded_can::Id::Extended(id) => id.as_raw() | 0x8000_0000,
        };
        self.log(can_id, timestamp_us, frame.data())
    }

    /// Log a CAN FD frame using the FdFrame trait.
    #[cfg(feature = "can")]
    #[inline]
    pub fn log_fd_frame<F: FdFrame>(&mut self, timestamp_us: u64, frame: &F) -> bool {
        let can_id = match frame.id() {
            embedded_can::Id::Standard(id) => id.as_raw() as u32,
            embedded_can::Id::Extended(id) => id.as_raw() | 0x8000_0000,
        };
        if frame.is_fd() {
            self.log_fd(can_id, timestamp_us, frame.data(), frame.fd_flags())
        } else {
            self.log(can_id, timestamp_us, frame.data())
        }
    }

    /// Flush buffered data to the MDF writer.
    pub fn flush(&mut self) -> crate::Result<()> {
        if !self.initialized {
            self.initialize_mdf()?;
        }

        // Write data for each CAN ID
        for can_id in self.buffers.keys().copied().collect::<Vec<_>>() {
            self.write_frames(can_id)?;
        }

        // Clear all buffers
        for buffer in self.buffers.values_mut() {
            buffer.clear();
        }

        Ok(())
    }

    /// Initialize the MDF file structure.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        use crate::DataType;

        self.writer.init_mdf_file()?;

        // Determine data channel count based on max data length seen
        let data_channel_count = self.max_data_len.max(8);

        if self.group_by_id {
            // Create a channel group for each CAN ID
            for &can_id in self.buffers.keys() {
                let cg = self.writer.add_channel_group(None, |_| {})?;

                // Set channel group name
                let name = if can_id & 0x8000_0000 != 0 {
                    alloc::format!("CAN_0x{:08X}", can_id & 0x1FFF_FFFF)
                } else {
                    alloc::format!("CAN_0x{:03X}", can_id)
                };
                self.writer.set_channel_group_name(&cg, &name)?;

                // Add timestamp channel
                let time_ch = self.writer.add_channel(&cg, None, |ch| {
                    ch.data_type = DataType::UnsignedIntegerLE;
                    ch.name = Some(alloc::string::String::from("Timestamp"));
                    ch.bit_count = 64;
                })?;
                self.writer.set_time_channel(&time_ch)?;
                self.writer.set_channel_unit(&time_ch, "us")?;

                // Add CAN ID channel
                let id_ch = self.writer.add_channel(&cg, Some(&time_ch), |ch| {
                    ch.data_type = DataType::UnsignedIntegerLE;
                    ch.name = Some(alloc::string::String::from("CAN_ID"));
                    ch.bit_count = 32;
                })?;

                // Add DLC channel
                let dlc_ch = self.writer.add_channel(&cg, Some(&id_ch), |ch| {
                    ch.data_type = DataType::UnsignedIntegerLE;
                    ch.name = Some(alloc::string::String::from("DLC"));
                    ch.bit_count = 8;
                })?;

                // Add FD_Flags channel (BRS=bit0, ESI=bit1)
                let flags_ch = self.writer.add_channel(&cg, Some(&dlc_ch), |ch| {
                    ch.data_type = DataType::UnsignedIntegerLE;
                    ch.name = Some(alloc::string::String::from("FD_Flags"));
                    ch.bit_count = 8;
                })?;

                // Add data byte channels (up to 64 for CAN FD)
                let mut prev_ch = flags_ch;
                for i in 0..data_channel_count {
                    let ch = self.writer.add_channel(&cg, Some(&prev_ch), |ch| {
                        ch.data_type = DataType::UnsignedIntegerLE;
                        ch.name = Some(alloc::format!("Data_{}", i));
                        ch.bit_count = 8;
                    })?;
                    prev_ch = ch;
                }

                self.channel_groups.insert(can_id, cg);
            }
        } else {
            // Single channel group for all CAN IDs
            let cg = self.writer.add_channel_group(None, |_| {})?;
            self.writer.set_channel_group_name(&cg, "RawCAN")?;

            // Add timestamp channel
            let time_ch = self.writer.add_channel(&cg, None, |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::string::String::from("Timestamp"));
                ch.bit_count = 64;
            })?;
            self.writer.set_time_channel(&time_ch)?;
            self.writer.set_channel_unit(&time_ch, "us")?;

            // Add CAN ID channel
            let id_ch = self.writer.add_channel(&cg, Some(&time_ch), |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::string::String::from("CAN_ID"));
                ch.bit_count = 32;
            })?;

            // Add DLC channel
            let dlc_ch = self.writer.add_channel(&cg, Some(&id_ch), |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::string::String::from("DLC"));
                ch.bit_count = 8;
            })?;

            // Add FD_Flags channel
            let flags_ch = self.writer.add_channel(&cg, Some(&dlc_ch), |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::string::String::from("FD_Flags"));
                ch.bit_count = 8;
            })?;

            // Add data byte channels
            let mut prev_ch = flags_ch;
            for i in 0..data_channel_count {
                let ch = self.writer.add_channel(&cg, Some(&prev_ch), |ch| {
                    ch.data_type = DataType::UnsignedIntegerLE;
                    ch.name = Some(alloc::format!("Data_{}", i));
                    ch.bit_count = 8;
                })?;
                prev_ch = ch;
            }

            // Use same channel group for all CAN IDs
            for &can_id in self.buffers.keys() {
                self.channel_groups.insert(can_id, cg.clone());
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// Write frames for a specific CAN ID.
    fn write_frames(&mut self, can_id: u32) -> crate::Result<()> {
        use crate::DecodedValue;

        let cg = match self.channel_groups.get(&can_id) {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        let frames = match self.buffers.get(&can_id) {
            Some(f) if !f.is_empty() => f,
            _ => return Ok(()),
        };

        // Determine data channel count (must match what was created in initialize_mdf)
        let data_channel_count = self.max_data_len.max(8);

        self.writer.start_data_block_for_cg(&cg, 0)?;

        for frame in frames {
            // Build values: timestamp, can_id, dlc, fd_flags, data[0..n]
            let mut values = Vec::with_capacity(4 + data_channel_count);
            values.push(DecodedValue::UnsignedInteger(frame.timestamp_us));
            values.push(DecodedValue::UnsignedInteger(can_id as u64));
            values.push(DecodedValue::UnsignedInteger(frame.dlc as u64));
            values.push(DecodedValue::UnsignedInteger(frame.fd_flags.to_byte() as u64));

            // Add data bytes (pad with zeros if needed)
            for i in 0..data_channel_count {
                let byte = if i < frame.data_len { frame.data[i] } else { 0 };
                values.push(DecodedValue::UnsignedInteger(byte as u64));
            }

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
    pub fn frame_count(&self, can_id: u32) -> usize {
        self.buffers.get(&can_id).map(|b| b.len()).unwrap_or(0)
    }

    /// Get all CAN IDs being logged.
    pub fn can_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.buffers.keys().copied()
    }

    /// Get the total number of frames logged.
    pub fn total_frame_count(&self) -> usize {
        self.buffers.values().map(|b| b.len()).sum()
    }

    /// Get the number of unique CAN IDs.
    pub fn unique_id_count(&self) -> usize {
        self.buffers.len()
    }

    /// Configure whether to group frames by CAN ID.
    ///
    /// When `true` (default), each CAN ID gets its own channel group.
    /// When `false`, all frames go into a single channel group.
    pub fn set_group_by_id(&mut self, group_by_id: bool) {
        self.group_by_id = group_by_id;
    }

    /// Check if any CAN FD frames have been logged.
    pub fn has_fd_frames(&self) -> bool {
        self.max_data_len > 8
    }

    /// Get the maximum data length seen across all frames.
    ///
    /// Returns 8 for classic CAN only, or up to 64 for CAN FD.
    pub fn max_data_length(&self) -> usize {
        self.max_data_len
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

        assert_eq!(logger.frame_count(0x100), 2);
        assert_eq!(logger.frame_count(0x200), 1);
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
        let extended_id = 0x1234_5678 | 0x8000_0000;
        assert!(logger.log(extended_id, 1000, &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]));

        assert_eq!(logger.frame_count(extended_id), 1);

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

        assert_eq!(logger.frame_count(0x100), 1);
        assert!(logger.has_fd_frames());
        assert_eq!(logger.max_data_length(), 32);

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

        assert_eq!(logger.frame_count(0x200), 1);
        assert!(logger.has_fd_frames());
        assert_eq!(logger.max_data_length(), 64);

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
        assert_eq!(logger.max_data_length(), 24);

        // Log another classic CAN frame
        assert!(logger.log(0x100, 3000, &[9, 10, 11, 12]));

        assert_eq!(logger.frame_count(0x100), 2);
        assert_eq!(logger.frame_count(0x200), 1);
        assert_eq!(logger.total_frame_count(), 3);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
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
}
