//! Raw Ethernet frame logger following ASAM MDF4 Bus Logging specification.
//!
//! This module provides [`RawEthernetLogger`], a logger for capturing raw Ethernet
//! frames to MDF4 files using the ASAM-compliant `ETH_Frame` format.
//!
//! # Features
//!
//! - ASAM MDF4 Bus Logging compliant format
//! - `ETH_Frame` channel with ByteArray (MAC addresses + EtherType + Payload)
//! - Timestamp as Float64 in seconds
//! - Source metadata (Ethernet interface name)
//! - Supports standard and jumbo frames
//! - Direction tracking (Tx/Rx)
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::ethernet::{RawEthernetLogger, MacAddress, EthernetFrame, ethertype};
//!
//! let mut logger = RawEthernetLogger::new()?;
//!
//! // Log raw Ethernet frame
//! let frame = EthernetFrame::new(
//!     MacAddress::broadcast(),
//!     MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
//!     ethertype::IPV4,
//!     payload.to_vec(),
//! );
//! logger.log_frame(timestamp_us, frame);
//!
//! // Or log from raw bytes
//! logger.log(timestamp_us, &raw_frame_bytes);
//!
//! // Get MDF bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use super::frame::{
    ETH_HEADER_SIZE, EthernetFlags, EthernetFrame, MAX_ETHERNET_FRAME, MAX_JUMBO_PAYLOAD,
    MacAddress,
};
use crate::bus_logging::{BusLoggerConfig, init_bus_channel_group, timestamp_to_seconds};

/// Frame size classification for ASAM channel grouping.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum FrameSize {
    /// Standard Ethernet frame (up to 1514 bytes)
    Standard,
    /// Jumbo frame (> 1514 bytes, up to ~9000 bytes)
    Jumbo,
}

impl FrameSize {
    /// All frame size variants for zero-allocation iteration.
    const ALL: [Self; 2] = [Self::Standard, Self::Jumbo];

    fn group_name(&self, source_name: &str) -> String {
        match self {
            FrameSize::Standard => alloc::format!("{}_ETH_Frame", source_name),
            FrameSize::Jumbo => alloc::format!("{}_ETH_Frame_Jumbo", source_name),
        }
    }

    fn max_frame_size(&self) -> usize {
        match self {
            FrameSize::Standard => MAX_ETHERNET_FRAME,
            FrameSize::Jumbo => ETH_HEADER_SIZE + MAX_JUMBO_PAYLOAD,
        }
    }

    fn from_length(len: usize) -> Self {
        if len > MAX_ETHERNET_FRAME {
            FrameSize::Jumbo
        } else {
            FrameSize::Standard
        }
    }
}

/// A buffered raw Ethernet frame with timestamp.
#[derive(Clone)]
struct RawEthFrame {
    /// Timestamp in seconds (ASAM uses float64 seconds)
    timestamp_s: f64,
    /// Frame data (complete Ethernet frame including header)
    data: Vec<u8>,
    /// Frame flags
    flags: EthernetFlags,
}

impl RawEthFrame {
    fn new(timestamp_us: u64, data: Vec<u8>, flags: EthernetFlags) -> Self {
        Self {
            timestamp_s: timestamp_to_seconds(timestamp_us),
            data,
            flags,
        }
    }

    fn frame_size(&self) -> FrameSize {
        FrameSize::from_length(self.data.len())
    }

    /// Build the ETH_Frame ByteArray in ASAM format.
    ///
    /// Format:
    /// - Byte 0: Flags (direction, FCS valid, etc.)
    /// - Bytes 1-2: Frame length (little-endian)
    /// - Bytes 3+: Frame data (Dst MAC + Src MAC + EtherType + Payload)
    fn to_frame_bytes(&self, max_size: usize) -> Vec<u8> {
        // ASAM format: flags(1) + length(2) + frame data
        let header_size = 3;
        let total_size = header_size + max_size;

        let mut bytes = Vec::with_capacity(total_size);

        // Flags byte
        bytes.push(self.flags.to_byte());

        // Frame length (little-endian u16)
        let frame_len = self.data.len() as u16;
        bytes.extend_from_slice(&frame_len.to_le_bytes());

        // Frame data (padded to max_size)
        let copy_len = self.data.len().min(max_size);
        bytes.extend_from_slice(&self.data[..copy_len]);
        bytes.resize(total_size, 0);

        bytes
    }
}

/// Raw Ethernet frame logger using ASAM MDF4 Bus Logging format.
///
/// This logger captures raw Ethernet frames using the industry-standard
/// `ETH_Frame` composite format, compatible with automotive Ethernet
/// analysis tools.
///
/// ## Channel Group Structure
///
/// Frames are organized into channel groups by size:
/// - `{source_name}_ETH_Frame` - Standard Ethernet frames (up to 1514 bytes)
/// - `{source_name}_ETH_Frame_Jumbo` - Jumbo frames (> 1514 bytes)
///
/// ## ETH_Frame Format
///
/// Each frame is stored as a ByteArray:
/// - Byte 0: Flags (bit 0 = Tx/Rx, bit 1 = FCS valid, etc.)
/// - Bytes 1-2: Frame length (little-endian u16)
/// - Bytes 3+: Frame data (Dst MAC + Src MAC + EtherType + Payload)
pub struct RawEthernetLogger<W: crate::writer::MdfWrite> {
    writer: crate::MdfWriter<W>,
    /// Source name for metadata
    source_name: String,
    /// Buffered frames by size category
    buffers: alloc::collections::BTreeMap<FrameSize, Vec<RawEthFrame>>,
    /// Channel group IDs by frame size
    channel_groups: alloc::collections::BTreeMap<FrameSize, String>,
    initialized: bool,
}

impl RawEthernetLogger<crate::writer::VecWriter> {
    /// Create a new raw Ethernet logger with in-memory output.
    pub fn new() -> crate::Result<Self> {
        Self::with_source_name("ETH")
    }

    /// Create a new raw Ethernet logger with a custom source name.
    ///
    /// The source name is used for channel group names and source metadata.
    /// Examples: "ETH", "eth0", "Ethernet1", "Vehicle_ETH", etc.
    pub fn with_source_name(source_name: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());
        Ok(Self {
            writer,
            source_name: String::from(source_name),
            buffers: alloc::collections::BTreeMap::new(),
            channel_groups: alloc::collections::BTreeMap::new(),
            initialized: false,
        })
    }

    /// Create a new raw Ethernet logger with a custom interface name.
    ///
    /// Alias for [`with_source_name`](Self::with_source_name) for API compatibility.
    pub fn with_interface_name(interface_name: &str) -> crate::Result<Self> {
        Self::with_source_name(interface_name)
    }

    /// Create a new raw Ethernet logger with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> crate::Result<Self> {
        let writer =
            crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self {
            writer,
            source_name: String::from("ETH"),
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
impl RawEthernetLogger<crate::writer::FileWriter> {
    /// Create a new raw Ethernet logger that writes to a file.
    pub fn new_file(path: &str) -> crate::Result<Self> {
        Self::new_file_with_source_name(path, "ETH")
    }

    /// Create a new raw Ethernet logger that writes to a file with custom source name.
    pub fn new_file_with_source_name(path: &str, source_name: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::new(path)?;
        Ok(Self {
            writer,
            source_name: String::from(source_name),
            buffers: alloc::collections::BTreeMap::new(),
            channel_groups: alloc::collections::BTreeMap::new(),
            initialized: false,
        })
    }

    /// Create a new raw Ethernet logger that writes to a file with custom interface name.
    ///
    /// Alias for [`new_file_with_source_name`](Self::new_file_with_source_name) for API compatibility.
    pub fn new_file_with_interface_name(path: &str, interface_name: &str) -> crate::Result<Self> {
        Self::new_file_with_source_name(path, interface_name)
    }

    /// Finalize and close the MDF file.
    pub fn finalize_file(mut self) -> crate::Result<()> {
        self.flush_and_finalize()
    }
}

impl<W: crate::writer::MdfWrite> RawEthernetLogger<W> {
    /// Set the source name for metadata.
    ///
    /// Must be called before logging any frames.
    pub fn set_source_name(&mut self, name: &str) {
        self.source_name = String::from(name);
    }

    /// Set the Ethernet interface name for source metadata.
    ///
    /// Alias for [`set_source_name`](Self::set_source_name) for API compatibility.
    pub fn set_interface_name(&mut self, name: &str) {
        self.set_source_name(name);
    }

    /// Log a raw Ethernet frame from bytes.
    ///
    /// # Arguments
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `frame_bytes` - Raw frame bytes (Dst MAC + Src MAC + EtherType + Payload)
    ///
    /// # Returns
    /// `true` if frame was logged, `false` if frame is too short
    pub fn log(&mut self, timestamp_us: u64, frame_bytes: &[u8]) -> bool {
        self.log_with_flags(timestamp_us, frame_bytes, EthernetFlags::rx())
    }

    /// Log a raw Ethernet frame with explicit flags.
    ///
    /// # Arguments
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `frame_bytes` - Raw frame bytes
    /// * `flags` - Frame flags (direction, FCS valid, etc.)
    pub fn log_with_flags(
        &mut self,
        timestamp_us: u64,
        frame_bytes: &[u8],
        flags: EthernetFlags,
    ) -> bool {
        if frame_bytes.len() < ETH_HEADER_SIZE {
            return false;
        }

        let frame = RawEthFrame::new(timestamp_us, frame_bytes.to_vec(), flags);
        self.buffers
            .entry(frame.frame_size())
            .or_default()
            .push(frame);
        true
    }

    /// Log a transmitted frame.
    pub fn log_tx(&mut self, timestamp_us: u64, frame_bytes: &[u8]) -> bool {
        self.log_with_flags(timestamp_us, frame_bytes, EthernetFlags::tx())
    }

    /// Log a received frame.
    pub fn log_rx(&mut self, timestamp_us: u64, frame_bytes: &[u8]) -> bool {
        self.log_with_flags(timestamp_us, frame_bytes, EthernetFlags::rx())
    }

    /// Log an EthernetFrame struct (takes ownership).
    ///
    /// # Arguments
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `frame` - The Ethernet frame to log (consumed)
    pub fn log_frame(&mut self, timestamp_us: u64, frame: EthernetFrame) -> bool {
        let bytes = frame.to_bytes();
        self.log_with_flags(timestamp_us, &bytes, frame.flags)
    }

    /// Log an EthernetFrame struct by reference (clones payload).
    ///
    /// Use [`log_frame`](Self::log_frame) if you don't need to reuse the frame.
    pub fn log_frame_ref(&mut self, timestamp_us: u64, frame: &EthernetFrame) -> bool {
        let bytes = frame.to_bytes();
        self.log_with_flags(timestamp_us, &bytes, frame.flags)
    }

    /// Log a frame constructed from components.
    ///
    /// # Arguments
    /// * `timestamp_us` - Timestamp in microseconds
    /// * `dst_mac` - Destination MAC address
    /// * `src_mac` - Source MAC address
    /// * `ethertype` - EtherType field
    /// * `payload` - Payload data
    pub fn log_components(
        &mut self,
        timestamp_us: u64,
        dst_mac: MacAddress,
        src_mac: MacAddress,
        ethertype: u16,
        payload: &[u8],
    ) -> bool {
        self.log_frame(
            timestamp_us,
            EthernetFrame::new(dst_mac, src_mac, ethertype, payload.to_vec()),
        )
    }

    /// Flush buffered data to the MDF writer.
    pub fn flush(&mut self) -> crate::Result<()> {
        if !self.initialized {
            self.initialize_mdf()?;
        }

        // Write data for each frame size category (zero-allocation iteration)
        for frame_size in FrameSize::ALL {
            if self.buffers.contains_key(&frame_size) {
                self.write_frames(frame_size)?;
            }
        }

        // Clear all buffers
        for buffer in self.buffers.values_mut() {
            buffer.clear();
        }

        Ok(())
    }

    /// Initialize the MDF file structure with ASAM-compliant channel groups.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        self.writer.init_mdf_file()?;

        // Create a channel group for each frame size that has data
        for &frame_size in self.buffers.keys() {
            let max_frame_size = frame_size.max_frame_size();
            // ETH_Frame size: flags(1) + length(2) + frame_data(max_frame_size)
            let frame_channel_size = 3 + max_frame_size;

            let config = BusLoggerConfig {
                source_name: self.source_name.clone(),
                group_name: frame_size.group_name(&self.source_name),
                data_channel_name: String::from("ETH_Frame"),
                data_channel_bits: (frame_channel_size * 8) as u32,
                source_block: crate::blocks::SourceBlock::ethernet(),
            };

            let (cg, _data_ch) = init_bus_channel_group(&mut self.writer, &config)?;
            self.channel_groups.insert(frame_size, cg);
        }

        self.initialized = true;
        Ok(())
    }

    /// Write frames for a specific frame size category.
    fn write_frames(&mut self, frame_size: FrameSize) -> crate::Result<()> {
        use crate::DecodedValue;

        let cg = match self.channel_groups.get(&frame_size) {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        let frames = match self.buffers.get(&frame_size) {
            Some(f) if !f.is_empty() => f,
            _ => return Ok(()),
        };

        let max_size = frame_size.max_frame_size();
        self.writer.start_data_block_for_cg(&cg, 0)?;

        for frame in frames {
            let values = [
                DecodedValue::Float(frame.timestamp_s),
                DecodedValue::ByteArray(frame.to_frame_bytes(max_size)),
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
        self.buffers.values().map(|b| b.len()).sum()
    }

    /// Get the number of standard frames logged.
    pub fn standard_frame_count(&self) -> usize {
        self.buffers
            .get(&FrameSize::Standard)
            .map(|b| b.len())
            .unwrap_or(0)
    }

    /// Get the number of jumbo frames logged.
    pub fn jumbo_frame_count(&self) -> usize {
        self.buffers
            .get(&FrameSize::Jumbo)
            .map(|b| b.len())
            .unwrap_or(0)
    }

    /// Check if any jumbo frames have been logged.
    pub fn has_jumbo_frames(&self) -> bool {
        self.jumbo_frame_count() > 0
    }

    /// Get count of transmitted frames.
    pub fn tx_frame_count(&self) -> usize {
        self.buffers
            .values()
            .flat_map(|frames| frames.iter())
            .filter(|f| f.flags.is_tx())
            .count()
    }

    /// Get count of received frames.
    pub fn rx_frame_count(&self) -> usize {
        self.buffers
            .values()
            .flat_map(|frames| frames.iter())
            .filter(|f| f.flags.is_rx())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ethernet::frame::ethertype;

    fn create_test_frame(payload_len: usize) -> Vec<u8> {
        let mut frame = Vec::with_capacity(ETH_HEADER_SIZE + payload_len);
        // Dst MAC (broadcast)
        frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        // Src MAC
        frame.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        // EtherType (IPv4)
        frame.extend_from_slice(&[0x08, 0x00]);
        // Payload
        frame.extend(core::iter::repeat(0xAA).take(payload_len));
        frame
    }

    #[test]
    fn test_raw_ethernet_logger_basic() {
        let mut logger = RawEthernetLogger::new().unwrap();

        // Log some frames
        let frame1 = create_test_frame(100);
        let frame2 = create_test_frame(200);
        let frame3 = create_test_frame(50);

        assert!(logger.log(1000, &frame1));
        assert!(logger.log(2000, &frame2));
        assert!(logger.log(1500, &frame3));

        assert_eq!(logger.total_frame_count(), 3);
        assert_eq!(logger.standard_frame_count(), 3);
        assert_eq!(logger.jumbo_frame_count(), 0);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
        assert_eq!(&mdf_bytes[0..3], b"MDF");
    }

    #[test]
    fn test_raw_ethernet_logger_tx_rx() {
        let mut logger = RawEthernetLogger::new().unwrap();

        let frame = create_test_frame(100);

        assert!(logger.log_tx(1000, &frame));
        assert!(logger.log_rx(2000, &frame));
        assert!(logger.log_tx(3000, &frame));

        assert_eq!(logger.total_frame_count(), 3);
        assert_eq!(logger.tx_frame_count(), 2);
        assert_eq!(logger.rx_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_ethernet_logger_empty() {
        let logger = RawEthernetLogger::new().unwrap();
        assert_eq!(logger.total_frame_count(), 0);

        let mdf_bytes = logger.finalize().unwrap();
        // Even empty file should have MDF header
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_ethernet_logger_short_frame() {
        let mut logger = RawEthernetLogger::new().unwrap();

        // Frame too short (less than 14 bytes header)
        let short_frame = [0u8; 10];
        assert!(!logger.log(1000, &short_frame));

        assert_eq!(logger.total_frame_count(), 0);
    }

    #[test]
    fn test_raw_ethernet_logger_frame_struct() {
        let mut logger = RawEthernetLogger::new().unwrap();

        let frame = EthernetFrame::new(
            MacAddress::broadcast(),
            MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
            ethertype::IPV4,
            vec![0x45, 0x00, 0x00, 0x28, 0x00, 0x00],
        );

        assert!(logger.log_frame(1000, frame));
        assert_eq!(logger.total_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_raw_ethernet_logger_components() {
        let mut logger = RawEthernetLogger::new().unwrap();

        assert!(logger.log_components(
            1000,
            MacAddress::broadcast(),
            MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
            ethertype::ARP,
            &[0x00, 0x01, 0x08, 0x00, 0x06, 0x04],
        ));

        assert_eq!(logger.total_frame_count(), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_source_name() {
        let logger = RawEthernetLogger::with_source_name("eth0").unwrap();
        assert_eq!(logger.source_name, "eth0");
    }

    #[test]
    fn test_interface_name_alias() {
        let logger = RawEthernetLogger::with_interface_name("eth0").unwrap();
        assert_eq!(logger.source_name, "eth0");
    }

    #[test]
    fn test_frame_bytes_format() {
        let frame_data = create_test_frame(10);
        let raw_frame = RawEthFrame::new(1_000_000, frame_data.clone(), EthernetFlags::tx());

        let bytes = raw_frame.to_frame_bytes(MAX_ETHERNET_FRAME);

        // Check flags byte
        assert_eq!(bytes[0], EthernetFlags::TX);
        // Check length (little-endian)
        let len = u16::from_le_bytes([bytes[1], bytes[2]]);
        assert_eq!(len, frame_data.len() as u16);
        // Check frame data starts at byte 3
        assert_eq!(&bytes[3..3 + frame_data.len()], &frame_data[..]);
    }
}
