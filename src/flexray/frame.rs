//! FlexRay frame types and constants for ASAM MDF4 Bus Logging.
//!
//! This module defines the FlexRay frame structure according to
//! the ASAM MDF4 Bus Logging specification.

use alloc::vec::Vec;

use crate::bus_logging::BusFrame;

/// Maximum FlexRay payload size (254 bytes, 127 words × 2).
pub const MAX_FLEXRAY_PAYLOAD: usize = 254;

/// Maximum FlexRay slot ID (2047, 11 bits).
pub const MAX_SLOT_ID: u16 = 2047;

/// Maximum FlexRay cycle count (63, 6 bits).
pub const MAX_CYCLE_COUNT: u8 = 63;

/// FlexRay channel identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FlexRayChannel {
    /// Channel A.
    #[default]
    A = 0,
    /// Channel B.
    B = 1,
    /// Both channels (A and B).
    AB = 2,
}

impl FlexRayChannel {
    /// Create from raw byte value.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::A,
            1 => Self::B,
            2 => Self::AB,
            _ => Self::A,
        }
    }
}

/// FlexRay frame flags for ASAM MDF4 Bus Logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlexRayFlags(u16);

impl FlexRayFlags {
    /// Bit 0: Frame direction (0 = Rx, 1 = Tx).
    pub const TX: u16 = 0x0001;
    /// Bit 1: Startup frame indicator.
    pub const STARTUP: u16 = 0x0002;
    /// Bit 2: Sync frame indicator.
    pub const SYNC: u16 = 0x0004;
    /// Bit 3: Null frame indicator (no payload).
    pub const NULL_FRAME: u16 = 0x0008;
    /// Bit 4: Payload preamble indicator.
    pub const PAYLOAD_PREAMBLE: u16 = 0x0010;
    /// Bit 5: Header CRC error.
    pub const HEADER_CRC_ERROR: u16 = 0x0020;
    /// Bit 6: Frame CRC error.
    pub const FRAME_CRC_ERROR: u16 = 0x0040;
    /// Bit 7: Coding error.
    pub const CODING_ERROR: u16 = 0x0080;
    /// Bit 8: TSS violation.
    pub const TSS_VIOLATION: u16 = 0x0100;
    /// Bit 9: Valid frame received.
    pub const VALID: u16 = 0x0200;
    /// Bit 10: Network Management Vector included.
    pub const NM_VECTOR: u16 = 0x0400;
    /// Bit 11: Dynamic segment frame.
    pub const DYNAMIC: u16 = 0x0800;

    /// Create flags from raw u16.
    pub fn from_u16(value: u16) -> Self {
        Self(value)
    }

    /// Get raw u16 value.
    pub fn to_u16(self) -> u16 {
        self.0
    }

    /// Create flags for received frame.
    pub fn rx() -> Self {
        Self(Self::VALID)
    }

    /// Create flags for transmitted frame.
    pub fn tx() -> Self {
        Self(Self::TX | Self::VALID)
    }

    /// Check if this is a transmitted frame.
    pub fn is_tx(self) -> bool {
        self.0 & Self::TX != 0
    }

    /// Check if this is a received frame.
    pub fn is_rx(self) -> bool {
        !self.is_tx()
    }

    /// Check if this is a startup frame.
    pub fn is_startup(self) -> bool {
        self.0 & Self::STARTUP != 0
    }

    /// Check if this is a sync frame.
    pub fn is_sync(self) -> bool {
        self.0 & Self::SYNC != 0
    }

    /// Check if this is a null frame.
    pub fn is_null_frame(self) -> bool {
        self.0 & Self::NULL_FRAME != 0
    }

    /// Check if payload preamble is present.
    pub fn has_payload_preamble(self) -> bool {
        self.0 & Self::PAYLOAD_PREAMBLE != 0
    }

    /// Check if header CRC error occurred.
    pub fn has_header_crc_error(self) -> bool {
        self.0 & Self::HEADER_CRC_ERROR != 0
    }

    /// Check if frame CRC error occurred.
    pub fn has_frame_crc_error(self) -> bool {
        self.0 & Self::FRAME_CRC_ERROR != 0
    }

    /// Check if coding error occurred.
    pub fn has_coding_error(self) -> bool {
        self.0 & Self::CODING_ERROR != 0
    }

    /// Check if frame is valid.
    pub fn is_valid(self) -> bool {
        self.0 & Self::VALID != 0
    }

    /// Check if NM vector is included.
    pub fn has_nm_vector(self) -> bool {
        self.0 & Self::NM_VECTOR != 0
    }

    /// Check if this is a dynamic segment frame.
    pub fn is_dynamic(self) -> bool {
        self.0 & Self::DYNAMIC != 0
    }

    /// Check if any error occurred.
    pub fn has_error(self) -> bool {
        self.0
            & (Self::HEADER_CRC_ERROR
                | Self::FRAME_CRC_ERROR
                | Self::CODING_ERROR
                | Self::TSS_VIOLATION)
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

    /// Set the valid flag.
    pub fn with_valid(self, valid: bool) -> Self {
        if valid {
            Self(self.0 | Self::VALID)
        } else {
            Self(self.0 & !Self::VALID)
        }
    }

    /// Set the startup flag.
    pub fn with_startup(self, startup: bool) -> Self {
        if startup {
            Self(self.0 | Self::STARTUP)
        } else {
            Self(self.0 & !Self::STARTUP)
        }
    }

    /// Set the sync flag.
    pub fn with_sync(self, sync: bool) -> Self {
        if sync {
            Self(self.0 | Self::SYNC)
        } else {
            Self(self.0 & !Self::SYNC)
        }
    }

    /// Set the null frame flag.
    pub fn with_null_frame(self, null_frame: bool) -> Self {
        if null_frame {
            Self(self.0 | Self::NULL_FRAME)
        } else {
            Self(self.0 & !Self::NULL_FRAME)
        }
    }

    /// Set the dynamic segment flag.
    pub fn with_dynamic(self, dynamic: bool) -> Self {
        if dynamic {
            Self(self.0 | Self::DYNAMIC)
        } else {
            Self(self.0 & !Self::DYNAMIC)
        }
    }
}

/// A FlexRay frame for ASAM MDF4 Bus Logging.
///
/// # ASAM FLEXRAY_Frame Format
///
/// The ASAM MDF4 Bus Logging specification defines the FLEXRAY_Frame as:
/// - Bytes 0-1: Slot ID (little-endian, 11 bits used)
/// - Byte 2: Cycle count (0-63)
/// - Byte 3: Channel (0=A, 1=B, 2=AB)
/// - Bytes 4-5: Flags (little-endian)
/// - Byte 6: Header CRC (low byte)
/// - Byte 7: Payload length in bytes
/// - Bytes 8+: Payload data (up to 254 bytes)
#[derive(Debug, Clone)]
pub struct FlexRayFrame {
    /// Slot ID (1-2047).
    pub slot_id: u16,
    /// Cycle count (0-63).
    pub cycle: u8,
    /// Channel (A, B, or AB).
    pub channel: FlexRayChannel,
    /// Frame flags.
    pub flags: FlexRayFlags,
    /// Header CRC (11 bits).
    pub header_crc: u16,
    /// Payload data (up to 254 bytes).
    pub payload: Vec<u8>,
}

/// FlexRay header size (before payload).
pub const FLEXRAY_HEADER_SIZE: usize = 8;

impl FlexRayFrame {
    /// Create a new FlexRay frame.
    ///
    /// # Arguments
    /// * `slot_id` - Slot ID (1-2047)
    /// * `cycle` - Cycle count (0-63)
    /// * `channel` - Channel (A, B, or AB)
    /// * `payload` - Frame payload (up to 254 bytes)
    pub fn new(slot_id: u16, cycle: u8, channel: FlexRayChannel, payload: Vec<u8>) -> Self {
        Self {
            slot_id: slot_id.min(MAX_SLOT_ID),
            cycle: cycle & MAX_CYCLE_COUNT,
            channel,
            flags: FlexRayFlags::rx(),
            header_crc: 0,
            payload: if payload.len() > MAX_FLEXRAY_PAYLOAD {
                payload[..MAX_FLEXRAY_PAYLOAD].to_vec()
            } else {
                payload
            },
        }
    }

    /// Create a new FlexRay frame on channel A.
    pub fn channel_a(slot_id: u16, cycle: u8, payload: Vec<u8>) -> Self {
        Self::new(slot_id, cycle, FlexRayChannel::A, payload)
    }

    /// Create a new FlexRay frame on channel B.
    pub fn channel_b(slot_id: u16, cycle: u8, payload: Vec<u8>) -> Self {
        Self::new(slot_id, cycle, FlexRayChannel::B, payload)
    }

    /// Create a null frame (no payload).
    pub fn null_frame(slot_id: u16, cycle: u8, channel: FlexRayChannel) -> Self {
        let mut frame = Self::new(slot_id, cycle, channel, Vec::new());
        frame.flags = frame.flags.with_null_frame(true);
        frame
    }

    /// Create a startup frame.
    pub fn startup(slot_id: u16, cycle: u8, channel: FlexRayChannel, payload: Vec<u8>) -> Self {
        let mut frame = Self::new(slot_id, cycle, channel, payload);
        frame.flags = frame.flags.with_startup(true).with_sync(true);
        frame
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

    /// Set frame as dynamic segment.
    pub fn with_dynamic(mut self) -> Self {
        self.flags = self.flags.with_dynamic(true);
        self
    }

    /// Serialize the frame to bytes for ASAM MDF4 FLEXRAY_Frame format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let total_size = FLEXRAY_HEADER_SIZE + self.payload.len();
        let mut bytes = Vec::with_capacity(total_size);

        // Slot ID (2 bytes, little-endian)
        bytes.extend_from_slice(&self.slot_id.to_le_bytes());
        // Cycle count
        bytes.push(self.cycle);
        // Channel
        bytes.push(self.channel as u8);
        // Flags (2 bytes, little-endian)
        bytes.extend_from_slice(&self.flags.to_u16().to_le_bytes());
        // Header CRC (low byte only in minimal format)
        bytes.push((self.header_crc & 0xFF) as u8);
        // Payload length
        bytes.push(self.payload.len() as u8);
        // Payload
        bytes.extend_from_slice(&self.payload);

        bytes
    }

    /// Parse a frame from ASAM MDF4 FLEXRAY_Frame format bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < FLEXRAY_HEADER_SIZE {
            return None;
        }

        let slot_id = u16::from_le_bytes([bytes[0], bytes[1]]);
        let cycle = bytes[2];
        let channel = FlexRayChannel::from_u8(bytes[3]);
        let flags = FlexRayFlags::from_u16(u16::from_le_bytes([bytes[4], bytes[5]]));
        let header_crc = bytes[6] as u16;
        let payload_len = bytes[7] as usize;

        if bytes.len() < FLEXRAY_HEADER_SIZE + payload_len {
            return None;
        }

        let payload = bytes[FLEXRAY_HEADER_SIZE..FLEXRAY_HEADER_SIZE + payload_len].to_vec();

        Some(Self {
            slot_id,
            cycle,
            channel,
            flags,
            header_crc,
            payload,
        })
    }

    /// Get the payload slice.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Get payload length.
    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }
}

impl Default for FlexRayFrame {
    fn default() -> Self {
        Self::new(1, 0, FlexRayChannel::A, Vec::new())
    }
}

impl BusFrame for FlexRayFrame {
    fn to_mdf_bytes(&self) -> Vec<u8> {
        self.to_bytes()
    }

    fn mdf_size(&self) -> usize {
        FLEXRAY_HEADER_SIZE + self.payload.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flexray_frame_basic() {
        let frame = FlexRayFrame::new(100, 5, FlexRayChannel::A, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(frame.slot_id, 100);
        assert_eq!(frame.cycle, 5);
        assert_eq!(frame.channel, FlexRayChannel::A);
        assert_eq!(frame.payload(), &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_flexray_frame_slot_id_clamping() {
        let frame = FlexRayFrame::new(3000, 0, FlexRayChannel::A, vec![]);
        assert_eq!(frame.slot_id, MAX_SLOT_ID);
    }

    #[test]
    fn test_flexray_frame_cycle_masking() {
        let frame = FlexRayFrame::new(1, 100, FlexRayChannel::A, vec![]);
        assert_eq!(frame.cycle, 100 & MAX_CYCLE_COUNT);
    }

    #[test]
    fn test_flexray_flags() {
        let flags = FlexRayFlags::tx();
        assert!(flags.is_tx());
        assert!(flags.is_valid());

        let flags = FlexRayFlags::rx();
        assert!(flags.is_rx());
        assert!(flags.is_valid());

        let flags = FlexRayFlags::from_u16(
            FlexRayFlags::STARTUP | FlexRayFlags::SYNC | FlexRayFlags::VALID,
        );
        assert!(flags.is_startup());
        assert!(flags.is_sync());
        assert!(flags.is_valid());
    }

    #[test]
    fn test_flexray_frame_roundtrip() {
        let original = FlexRayFrame::new(100, 10, FlexRayChannel::B, vec![0xAA, 0xBB, 0xCC, 0xDD])
            .with_tx()
            .with_dynamic();
        let bytes = original.to_bytes();
        let parsed = FlexRayFrame::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.slot_id, original.slot_id);
        assert_eq!(parsed.cycle, original.cycle);
        assert_eq!(parsed.channel, original.channel);
        assert!(parsed.flags.is_tx());
        assert!(parsed.flags.is_dynamic());
        assert_eq!(parsed.payload(), original.payload());
    }

    #[test]
    fn test_flexray_channel() {
        assert_eq!(FlexRayChannel::from_u8(0), FlexRayChannel::A);
        assert_eq!(FlexRayChannel::from_u8(1), FlexRayChannel::B);
        assert_eq!(FlexRayChannel::from_u8(2), FlexRayChannel::AB);
        assert_eq!(FlexRayChannel::from_u8(99), FlexRayChannel::A);
    }

    #[test]
    fn test_null_frame() {
        let frame = FlexRayFrame::null_frame(50, 0, FlexRayChannel::A);
        assert!(frame.flags.is_null_frame());
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn test_startup_frame() {
        let frame = FlexRayFrame::startup(1, 0, FlexRayChannel::AB, vec![0x00; 8]);
        assert!(frame.flags.is_startup());
        assert!(frame.flags.is_sync());
    }
}
