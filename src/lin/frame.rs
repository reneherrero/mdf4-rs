//! LIN frame types and constants for ASAM MDF4 Bus Logging.
//!
//! This module defines the LIN frame structure according to
//! the ASAM MDF4 Bus Logging specification.

use alloc::vec::Vec;

use crate::bus_logging::BusFrame;

/// Maximum LIN frame data size (8 bytes).
pub const MAX_LIN_DATA_LEN: usize = 8;

/// LIN frame ID range (0-63, 6 bits).
pub const MAX_LIN_ID: u8 = 63;

/// LIN checksum type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ChecksumType {
    /// Classic checksum (LIN 1.x) - sum of data bytes only.
    #[default]
    Classic = 0,
    /// Enhanced checksum (LIN 2.x) - sum of ID and data bytes.
    Enhanced = 1,
}

impl ChecksumType {
    /// Create from raw byte value.
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Enhanced,
            _ => Self::Classic,
        }
    }
}

/// LIN frame flags for ASAM MDF4 Bus Logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinFlags(u8);

impl LinFlags {
    /// Bit 0: Frame direction (0 = Rx, 1 = Tx).
    pub const TX: u8 = 0x01;
    /// Bit 1: Wake-up signal.
    pub const WAKEUP: u8 = 0x02;
    /// Bit 2: Checksum error.
    pub const CHECKSUM_ERROR: u8 = 0x04;
    /// Bit 3: No response (slave didn't respond).
    pub const NO_RESPONSE: u8 = 0x08;
    /// Bit 4: Sync error.
    pub const SYNC_ERROR: u8 = 0x10;
    /// Bit 5: Framing error.
    pub const FRAMING_ERROR: u8 = 0x20;
    /// Bit 6: Short response (incomplete data).
    pub const SHORT_RESPONSE: u8 = 0x40;
    /// Bit 7: Enhanced checksum used (LIN 2.x).
    pub const ENHANCED_CHECKSUM: u8 = 0x80;

    /// Create flags from raw byte.
    pub fn from_byte(value: u8) -> Self {
        Self(value)
    }

    /// Get raw byte value.
    pub fn to_byte(self) -> u8 {
        self.0
    }

    /// Create flags for received frame.
    pub fn rx() -> Self {
        Self(0)
    }

    /// Create flags for transmitted frame.
    pub fn tx() -> Self {
        Self(Self::TX)
    }

    /// Check if this is a transmitted frame.
    pub fn is_tx(self) -> bool {
        self.0 & Self::TX != 0
    }

    /// Check if this is a received frame.
    pub fn is_rx(self) -> bool {
        !self.is_tx()
    }

    /// Check if this is a wake-up frame.
    pub fn is_wakeup(self) -> bool {
        self.0 & Self::WAKEUP != 0
    }

    /// Check if checksum error occurred.
    pub fn has_checksum_error(self) -> bool {
        self.0 & Self::CHECKSUM_ERROR != 0
    }

    /// Check if slave didn't respond.
    pub fn has_no_response(self) -> bool {
        self.0 & Self::NO_RESPONSE != 0
    }

    /// Check if sync error occurred.
    pub fn has_sync_error(self) -> bool {
        self.0 & Self::SYNC_ERROR != 0
    }

    /// Check if framing error occurred.
    pub fn has_framing_error(self) -> bool {
        self.0 & Self::FRAMING_ERROR != 0
    }

    /// Check if response was incomplete.
    pub fn has_short_response(self) -> bool {
        self.0 & Self::SHORT_RESPONSE != 0
    }

    /// Check if enhanced (LIN 2.x) checksum is used.
    pub fn uses_enhanced_checksum(self) -> bool {
        self.0 & Self::ENHANCED_CHECKSUM != 0
    }

    /// Check if any error occurred.
    pub fn has_error(self) -> bool {
        self.0
            & (Self::CHECKSUM_ERROR
                | Self::NO_RESPONSE
                | Self::SYNC_ERROR
                | Self::FRAMING_ERROR
                | Self::SHORT_RESPONSE)
            != 0
    }

    /// Set the transmit flag.
    pub fn with_tx(self, tx: bool) -> Self {
        if tx {
            Self(self.0 | Self::TX)
        } else {
            Self(self.0 & !Self::TX)
        }
    }

    /// Set the enhanced checksum flag.
    pub fn with_enhanced_checksum(self, enhanced: bool) -> Self {
        if enhanced {
            Self(self.0 | Self::ENHANCED_CHECKSUM)
        } else {
            Self(self.0 & !Self::ENHANCED_CHECKSUM)
        }
    }
}

/// A LIN frame for ASAM MDF4 Bus Logging.
///
/// # ASAM LIN_Frame Format
///
/// The ASAM MDF4 Bus Logging specification defines the LIN_Frame as:
/// - Byte 0: Frame ID (0-63)
/// - Byte 1: Data length (0-8)
/// - Byte 2: Flags (direction, errors, checksum type)
/// - Byte 3: Checksum
/// - Bytes 4-11: Data (8 bytes, zero-padded)
#[derive(Debug, Clone)]
pub struct LinFrame {
    /// LIN frame ID (0-63, 6 bits).
    pub id: u8,
    /// Frame data (up to 8 bytes).
    pub data: [u8; MAX_LIN_DATA_LEN],
    /// Actual data length.
    pub data_len: u8,
    /// Frame flags.
    pub flags: LinFlags,
    /// Checksum byte.
    pub checksum: u8,
}

impl LinFrame {
    /// Create a new LIN frame.
    ///
    /// # Arguments
    /// * `id` - Frame ID (0-63)
    /// * `data` - Frame data (up to 8 bytes)
    pub fn new(id: u8, data: &[u8]) -> Self {
        let mut frame_data = [0u8; MAX_LIN_DATA_LEN];
        let len = data.len().min(MAX_LIN_DATA_LEN);
        frame_data[..len].copy_from_slice(&data[..len]);

        Self {
            id: id & MAX_LIN_ID,
            data: frame_data,
            data_len: len as u8,
            flags: LinFlags::default(),
            checksum: 0,
        }
    }

    /// Create a LIN frame with classic checksum.
    pub fn with_classic_checksum(id: u8, data: &[u8]) -> Self {
        let mut frame = Self::new(id, data);
        frame.checksum = frame.calculate_classic_checksum();
        frame
    }

    /// Create a LIN frame with enhanced checksum.
    pub fn with_enhanced_checksum(id: u8, data: &[u8]) -> Self {
        let mut frame = Self::new(id, data);
        frame.checksum = frame.calculate_enhanced_checksum();
        frame.flags = frame.flags.with_enhanced_checksum(true);
        frame
    }

    /// Calculate classic checksum (data bytes only).
    pub fn calculate_classic_checksum(&self) -> u8 {
        let mut sum: u16 = 0;
        for i in 0..self.data_len as usize {
            sum += self.data[i] as u16;
            if sum > 0xFF {
                sum = (sum & 0xFF) + 1;
            }
        }
        !sum as u8
    }

    /// Calculate enhanced checksum (ID + data bytes).
    pub fn calculate_enhanced_checksum(&self) -> u8 {
        let protected_id = self.protected_id();
        let mut sum: u16 = protected_id as u16;
        for i in 0..self.data_len as usize {
            sum += self.data[i] as u16;
            if sum > 0xFF {
                sum = (sum & 0xFF) + 1;
            }
        }
        !sum as u8
    }

    /// Get the protected ID (ID with parity bits).
    pub fn protected_id(&self) -> u8 {
        let id = self.id & 0x3F;
        let p0 = (id ^ (id >> 1) ^ (id >> 2) ^ (id >> 4)) & 0x01;
        let p1 = !((id >> 1) ^ (id >> 3) ^ (id >> 4) ^ (id >> 5)) & 0x01;
        id | (p0 << 6) | (p1 << 7)
    }

    /// Set frame direction to transmit.
    pub fn with_tx(mut self) -> Self {
        self.flags = self.flags.with_tx(true);
        self
    }

    /// Set frame direction to receive.
    pub fn with_rx(mut self) -> Self {
        self.flags = self.flags.with_tx(false);
        self
    }

    /// Serialize the frame to bytes for ASAM MDF4 LIN_Frame format.
    ///
    /// Format (12 bytes total):
    /// - Byte 0: Frame ID
    /// - Byte 1: Data length
    /// - Byte 2: Flags
    /// - Byte 3: Checksum
    /// - Bytes 4-11: Data (8 bytes, zero-padded)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12);
        bytes.push(self.id);
        bytes.push(self.data_len);
        bytes.push(self.flags.to_byte());
        bytes.push(self.checksum);
        bytes.extend_from_slice(&self.data);
        bytes
    }

    /// Parse a frame from ASAM MDF4 LIN_Frame format bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 12 {
            return None;
        }

        let mut data = [0u8; MAX_LIN_DATA_LEN];
        data.copy_from_slice(&bytes[4..12]);

        Some(Self {
            id: bytes[0] & MAX_LIN_ID,
            data_len: bytes[1].min(MAX_LIN_DATA_LEN as u8),
            flags: LinFlags::from_byte(bytes[2]),
            checksum: bytes[3],
            data,
        })
    }

    /// Get the data slice.
    pub fn data(&self) -> &[u8] {
        &self.data[..self.data_len as usize]
    }
}

impl Default for LinFrame {
    fn default() -> Self {
        Self::new(0, &[])
    }
}

impl BusFrame for LinFrame {
    fn to_mdf_bytes(&self) -> Vec<u8> {
        self.to_bytes()
    }

    fn mdf_size(&self) -> usize {
        12 // Fixed size: ID + Length + Flags + Checksum + Data(8)
    }
}

/// LIN schedule table entry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScheduleEntryType {
    /// Unconditional frame.
    Unconditional = 0,
    /// Event-triggered frame.
    EventTriggered = 1,
    /// Sporadic frame.
    Sporadic = 2,
    /// Diagnostic request (Master Request Frame).
    DiagnosticRequest = 3,
    /// Diagnostic response (Slave Response Frame).
    DiagnosticResponse = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lin_frame_basic() {
        let frame = LinFrame::new(0x20, &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(frame.id, 0x20);
        assert_eq!(frame.data_len, 4);
        assert_eq!(frame.data(), &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_lin_frame_id_masking() {
        // ID should be masked to 6 bits (0-63)
        let frame = LinFrame::new(0xFF, &[0x01]);
        assert_eq!(frame.id, 0x3F);
    }

    #[test]
    fn test_lin_frame_data_truncation() {
        // Data should be truncated to 8 bytes
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];
        let frame = LinFrame::new(0x10, &data);
        assert_eq!(frame.data_len, 8);
    }

    #[test]
    fn test_lin_flags() {
        let flags = LinFlags::tx();
        assert!(flags.is_tx());
        assert!(!flags.is_rx());

        let flags = LinFlags::rx();
        assert!(flags.is_rx());
        assert!(!flags.is_tx());

        let flags = LinFlags::from_byte(LinFlags::CHECKSUM_ERROR | LinFlags::NO_RESPONSE);
        assert!(flags.has_checksum_error());
        assert!(flags.has_no_response());
        assert!(flags.has_error());
    }

    #[test]
    fn test_protected_id() {
        // Test known protected ID values
        let frame = LinFrame::new(0x00, &[]);
        assert_eq!(frame.protected_id() & 0x3F, 0x00);

        let frame = LinFrame::new(0x3C, &[]); // Diagnostic request ID
        let pid = frame.protected_id();
        assert_eq!(pid & 0x3F, 0x3C);
    }

    #[test]
    fn test_classic_checksum() {
        let frame = LinFrame::new(0x20, &[0x01, 0x02, 0x03, 0x04]);
        let checksum = frame.calculate_classic_checksum();
        // Classic checksum = ~(0x01 + 0x02 + 0x03 + 0x04) = ~0x0A = 0xF5
        assert_eq!(checksum, 0xF5);
    }

    #[test]
    fn test_frame_roundtrip() {
        let original = LinFrame::with_enhanced_checksum(0x20, &[0x01, 0x02, 0x03, 0x04]).with_tx();
        let bytes = original.to_bytes();
        let parsed = LinFrame::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.data_len, original.data_len);
        assert_eq!(parsed.data(), original.data());
        assert_eq!(parsed.checksum, original.checksum);
        assert!(parsed.flags.is_tx());
    }

    #[test]
    fn test_checksum_types() {
        assert_eq!(ChecksumType::from_u8(0), ChecksumType::Classic);
        assert_eq!(ChecksumType::from_u8(1), ChecksumType::Enhanced);
        assert_eq!(ChecksumType::from_u8(99), ChecksumType::Classic);
    }
}
