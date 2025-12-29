//! Ethernet frame types and constants for ASAM MDF4 Bus Logging.
//!
//! This module defines the Ethernet frame structure according to
//! the ASAM MDF4 Bus Logging specification.

use alloc::vec::Vec;

use crate::bus_logging::BusFrame;

/// Maximum Ethernet payload size (standard MTU).
pub const MAX_ETHERNET_PAYLOAD: usize = 1500;

/// Maximum Ethernet frame size (including headers, excluding preamble/FCS).
/// 6 (dst MAC) + 6 (src MAC) + 2 (EtherType) + 1500 (payload) = 1514 bytes
pub const MAX_ETHERNET_FRAME: usize = 1514;

/// Jumbo frame maximum payload size.
pub const MAX_JUMBO_PAYLOAD: usize = 9000;

/// MAC address size in bytes.
pub const MAC_ADDR_SIZE: usize = 6;

/// EtherType field size in bytes.
pub const ETHERTYPE_SIZE: usize = 2;

/// Ethernet header size (dst MAC + src MAC + EtherType).
pub const ETH_HEADER_SIZE: usize = MAC_ADDR_SIZE * 2 + ETHERTYPE_SIZE;

/// Common EtherType values.
pub mod ethertype {
    /// IPv4 (0x0800)
    pub const IPV4: u16 = 0x0800;
    /// IPv6 (0x86DD)
    pub const IPV6: u16 = 0x86DD;
    /// ARP (0x0806)
    pub const ARP: u16 = 0x0806;
    /// VLAN-tagged frame (802.1Q) (0x8100)
    pub const VLAN: u16 = 0x8100;
    /// SOME/IP (0x8123) - Automotive Ethernet
    pub const SOMEIP: u16 = 0x8123;
    /// DoIP - Diagnostic over IP (0x8000) - Note: DoIP uses UDP/TCP over IP
    pub const DOIP: u16 = 0x8000;
    /// AVB/TSN Audio Video Bridging (0x22F0)
    pub const AVB: u16 = 0x22F0;
    /// PROFINET (0x8892)
    pub const PROFINET: u16 = 0x8892;
    /// EtherCAT (0x88A4)
    pub const ETHERCAT: u16 = 0x88A4;
}

/// Ethernet frame flags for ASAM MDF4 Bus Logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EthernetFlags(u8);

impl EthernetFlags {
    /// Bit 0: Frame direction (0 = Rx, 1 = Tx).
    pub const TX: u8 = 0x01;
    /// Bit 1: FCS (Frame Check Sequence) is valid.
    pub const FCS_VALID: u8 = 0x02;
    /// Bit 2: Frame was truncated.
    pub const TRUNCATED: u8 = 0x04;
    /// Bit 3: CRC error detected.
    pub const CRC_ERROR: u8 = 0x08;
    /// Bit 4: Frame has VLAN tag.
    pub const VLAN_TAGGED: u8 = 0x10;

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

    /// Check if FCS is valid.
    pub fn fcs_valid(self) -> bool {
        self.0 & Self::FCS_VALID != 0
    }

    /// Check if frame was truncated.
    pub fn is_truncated(self) -> bool {
        self.0 & Self::TRUNCATED != 0
    }

    /// Check if CRC error was detected.
    pub fn has_crc_error(self) -> bool {
        self.0 & Self::CRC_ERROR != 0
    }

    /// Check if frame has VLAN tag.
    pub fn has_vlan_tag(self) -> bool {
        self.0 & Self::VLAN_TAGGED != 0
    }

    /// Set the transmit flag.
    pub fn with_tx(self, tx: bool) -> Self {
        if tx {
            Self(self.0 | Self::TX)
        } else {
            Self(self.0 & !Self::TX)
        }
    }

    /// Set the FCS valid flag.
    pub fn with_fcs_valid(self, valid: bool) -> Self {
        if valid {
            Self(self.0 | Self::FCS_VALID)
        } else {
            Self(self.0 & !Self::FCS_VALID)
        }
    }

    /// Set the VLAN tagged flag.
    pub fn with_vlan_tagged(self, tagged: bool) -> Self {
        if tagged {
            Self(self.0 | Self::VLAN_TAGGED)
        } else {
            Self(self.0 & !Self::VLAN_TAGGED)
        }
    }
}

/// MAC address representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Create a MAC address from bytes.
    pub const fn new(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    /// Create a broadcast MAC address (FF:FF:FF:FF:FF:FF).
    pub const fn broadcast() -> Self {
        Self([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
    }

    /// Create a zero/unspecified MAC address.
    pub const fn zero() -> Self {
        Self([0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
    }

    /// Check if this is a broadcast address.
    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 6]
    }

    /// Check if this is a multicast address (bit 0 of first byte set).
    pub fn is_multicast(&self) -> bool {
        self.0[0] & 0x01 != 0
    }

    /// Check if this is a unicast address.
    pub fn is_unicast(&self) -> bool {
        !self.is_multicast()
    }

    /// Check if this is a locally administered address (bit 1 of first byte set).
    pub fn is_local(&self) -> bool {
        self.0[0] & 0x02 != 0
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }
}

impl From<[u8; 6]> for MacAddress {
    fn from(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }
}

impl From<MacAddress> for [u8; 6] {
    fn from(mac: MacAddress) -> Self {
        mac.0
    }
}

/// An Ethernet frame for ASAM MDF4 Bus Logging.
///
/// # ASAM ETH_Frame Format
///
/// The ASAM MDF4 Bus Logging specification defines the ETH_Frame as:
/// - Bytes 0-5: Destination MAC address
/// - Bytes 6-11: Source MAC address
/// - Bytes 12-13: EtherType (big-endian)
/// - Bytes 14+: Payload data
///
/// Additional metadata (flags, direction) is stored in separate channels.
#[derive(Debug, Clone)]
pub struct EthernetFrame {
    /// Destination MAC address.
    pub dst_mac: MacAddress,
    /// Source MAC address.
    pub src_mac: MacAddress,
    /// EtherType field (e.g., 0x0800 for IPv4).
    pub ethertype: u16,
    /// Frame payload data.
    pub payload: Vec<u8>,
    /// Frame flags (direction, FCS valid, etc.).
    pub flags: EthernetFlags,
    /// Optional VLAN tag (802.1Q TCI field).
    pub vlan_tci: Option<u16>,
}

impl EthernetFrame {
    /// Create a new Ethernet frame.
    pub fn new(dst_mac: MacAddress, src_mac: MacAddress, ethertype: u16, payload: Vec<u8>) -> Self {
        Self {
            dst_mac,
            src_mac,
            ethertype,
            payload,
            flags: EthernetFlags::default(),
            vlan_tci: None,
        }
    }

    /// Create a frame from raw bytes (dst MAC + src MAC + EtherType + payload).
    ///
    /// Returns None if the bytes are too short.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < ETH_HEADER_SIZE {
            return None;
        }

        let dst_mac = MacAddress::new([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]]);
        let src_mac =
            MacAddress::new([bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11]]);
        let ethertype = u16::from_be_bytes([bytes[12], bytes[13]]);

        // Check for VLAN tag (802.1Q)
        let (ethertype, vlan_tci, payload_start) =
            if ethertype == ethertype::VLAN && bytes.len() >= ETH_HEADER_SIZE + 4 {
                let tci = u16::from_be_bytes([bytes[14], bytes[15]]);
                let real_ethertype = u16::from_be_bytes([bytes[16], bytes[17]]);
                (real_ethertype, Some(tci), 18)
            } else {
                (ethertype, None, ETH_HEADER_SIZE)
            };

        let payload = bytes[payload_start..].to_vec();
        let flags = if vlan_tci.is_some() {
            EthernetFlags::default().with_vlan_tagged(true)
        } else {
            EthernetFlags::default()
        };

        Some(Self {
            dst_mac,
            src_mac,
            ethertype,
            payload,
            flags,
            vlan_tci,
        })
    }

    /// Serialize the frame to bytes for ASAM MDF4 ETH_Frame format.
    ///
    /// Format:
    /// - Bytes 0-5: Destination MAC
    /// - Bytes 6-11: Source MAC
    /// - Bytes 12-13: EtherType (big-endian)
    /// - Bytes 14+: Payload
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ETH_HEADER_SIZE + self.payload.len());

        // Destination MAC
        bytes.extend_from_slice(self.dst_mac.as_bytes());
        // Source MAC
        bytes.extend_from_slice(self.src_mac.as_bytes());

        // Handle VLAN tag if present
        if let Some(tci) = self.vlan_tci {
            bytes.extend_from_slice(&ethertype::VLAN.to_be_bytes());
            bytes.extend_from_slice(&tci.to_be_bytes());
        }

        // EtherType (big-endian per Ethernet spec)
        bytes.extend_from_slice(&self.ethertype.to_be_bytes());
        // Payload
        bytes.extend_from_slice(&self.payload);

        bytes
    }

    /// Get the total frame length (header + payload).
    pub fn len(&self) -> usize {
        let vlan_size = if self.vlan_tci.is_some() { 4 } else { 0 };
        ETH_HEADER_SIZE + vlan_size + self.payload.len()
    }

    /// Check if the frame is empty (no payload).
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
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

    /// Set FCS valid flag.
    pub fn with_fcs_valid(mut self, valid: bool) -> Self {
        self.flags = self.flags.with_fcs_valid(valid);
        self
    }
}

impl BusFrame for EthernetFrame {
    fn to_mdf_bytes(&self) -> Vec<u8> {
        self.to_bytes()
    }

    fn mdf_size(&self) -> usize {
        // ETH_Frame header (4 bytes) + Ethernet header (14) + payload
        4 + ETH_HEADER_SIZE + self.payload.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_broadcast() {
        let broadcast = MacAddress::broadcast();
        assert!(broadcast.is_broadcast());
        assert!(broadcast.is_multicast());
        assert!(!broadcast.is_unicast());
    }

    #[test]
    fn test_mac_address_unicast() {
        let mac = MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        assert!(!mac.is_broadcast());
        assert!(!mac.is_multicast());
        assert!(mac.is_unicast());
    }

    #[test]
    fn test_mac_address_multicast() {
        let mac = MacAddress::new([0x01, 0x00, 0x5E, 0x00, 0x00, 0x01]);
        assert!(mac.is_multicast());
        assert!(!mac.is_unicast());
    }

    #[test]
    fn test_ethernet_flags() {
        let flags = EthernetFlags::tx();
        assert!(flags.is_tx());
        assert!(!flags.is_rx());

        let flags = EthernetFlags::rx();
        assert!(flags.is_rx());
        assert!(!flags.is_tx());

        let flags = EthernetFlags::default()
            .with_tx(true)
            .with_fcs_valid(true)
            .with_vlan_tagged(true);
        assert!(flags.is_tx());
        assert!(flags.fcs_valid());
        assert!(flags.has_vlan_tag());
    }

    #[test]
    fn test_frame_to_bytes() {
        let frame = EthernetFrame::new(
            MacAddress::new([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]),
            MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
            ethertype::IPV4,
            vec![0x45, 0x00, 0x00, 0x1C], // Minimal IP header start
        );

        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), ETH_HEADER_SIZE + 4);

        // Check destination MAC
        assert_eq!(&bytes[0..6], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        // Check source MAC
        assert_eq!(&bytes[6..12], &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        // Check EtherType (big-endian)
        assert_eq!(&bytes[12..14], &[0x08, 0x00]);
        // Check payload
        assert_eq!(&bytes[14..], &[0x45, 0x00, 0x00, 0x1C]);
    }

    #[test]
    fn test_frame_from_bytes() {
        let bytes = [
            // Dst MAC
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // Src MAC
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, // EtherType (IPv4)
            0x08, 0x00, // Payload
            0x45, 0x00, 0x00, 0x1C,
        ];

        let frame = EthernetFrame::from_bytes(&bytes).unwrap();
        assert!(frame.dst_mac.is_broadcast());
        assert_eq!(frame.ethertype, ethertype::IPV4);
        assert_eq!(frame.payload, vec![0x45, 0x00, 0x00, 0x1C]);
    }

    #[test]
    fn test_frame_roundtrip() {
        let original = EthernetFrame::new(
            MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
            MacAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]),
            ethertype::ARP,
            vec![0x00, 0x01, 0x08, 0x00, 0x06, 0x04],
        );

        let bytes = original.to_bytes();
        let parsed = EthernetFrame::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.dst_mac, original.dst_mac);
        assert_eq!(parsed.src_mac, original.src_mac);
        assert_eq!(parsed.ethertype, original.ethertype);
        assert_eq!(parsed.payload, original.payload);
    }

    #[test]
    fn test_frame_with_vlan() {
        // Frame with 802.1Q VLAN tag
        let bytes = [
            // Dst MAC
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, // Src MAC
            0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, // VLAN EtherType
            0x81, 0x00, // TCI (Priority 5, VLAN 100)
            0xA0, 0x64, // Real EtherType (IPv4)
            0x08, 0x00, // Payload
            0x45, 0x00,
        ];

        let frame = EthernetFrame::from_bytes(&bytes).unwrap();
        assert_eq!(frame.ethertype, ethertype::IPV4);
        assert_eq!(frame.vlan_tci, Some(0xA064));
        assert!(frame.flags.has_vlan_tag());
    }
}
