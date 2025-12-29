//! Event Block (##EV) - timestamped markers and triggers.
//!
//! EV blocks represent events that occurred during measurement, such as
//! triggers, user annotations, or system events. Events can form hierarchies
//! and reference specific channels or channel groups.

use super::EV_BLOCK_SIZE;
use crate::{
    Result,
    blocks::common::{
        BlockHeader, BlockParse, read_u8, read_u16, read_u32, read_u64, validate_buffer_size,
    },
};
use alloc::string::String;
use alloc::vec::Vec;

/// Event type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    /// Recording event (start/stop/pause/resume of recording).
    Recording = 0,
    /// Trigger event (hardware or software trigger).
    Trigger = 1,
    /// Marker event (user-defined marker).
    Marker = 2,
}

impl EventType {
    /// Create from raw u8 value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Recording),
            1 => Some(Self::Trigger),
            2 => Some(Self::Marker),
            _ => None,
        }
    }
}

/// Event synchronization type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventSyncType {
    /// Time in seconds.
    Time = 1,
    /// Angle in radians.
    Angle = 2,
    /// Distance in meters.
    Distance = 3,
    /// Index (sample number).
    Index = 4,
}

impl EventSyncType {
    /// Create from raw u8 value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Time),
            2 => Some(Self::Angle),
            3 => Some(Self::Distance),
            4 => Some(Self::Index),
            _ => None,
        }
    }
}

/// Event range type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventRangeType {
    /// Point event (single instant).
    Point = 0,
    /// Begin of range.
    RangeBegin = 1,
    /// End of range.
    RangeEnd = 2,
}

impl EventRangeType {
    /// Create from raw u8 value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Point),
            1 => Some(Self::RangeBegin),
            2 => Some(Self::RangeEnd),
            _ => None,
        }
    }
}

/// Event cause enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventCause {
    /// Other/unknown cause.
    Other = 0,
    /// Error condition.
    Error = 1,
    /// Tool-generated event.
    Tool = 2,
    /// Script-generated event.
    Script = 3,
    /// User-generated event.
    User = 4,
}

impl EventCause {
    /// Create from raw u8 value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Other),
            1 => Some(Self::Error),
            2 => Some(Self::Tool),
            3 => Some(Self::Script),
            4 => Some(Self::User),
            _ => None,
        }
    }
}

/// Event Block (##EV) - represents measurement events.
///
/// Events mark specific points or ranges in the measurement data. They can be
/// used for triggers, user annotations, recording events, or other markers.
///
/// # MDF4 Specification
///
/// The EV block has:
/// - 5 fixed links: next EV, parent EV, range EV, name TX, comment MD
/// - Variable links for scopes and attachments
/// - Event metadata (type, sync type, range type, cause, flags)
/// - Synchronization value (time, angle, distance, or index)
#[derive(Debug, Clone)]
pub struct EventBlock {
    /// Standard block header.
    pub header: BlockHeader,

    // === Fixed Links (5) ===
    /// Link to next event block (0 = end of list).
    pub next_ev_addr: u64,
    /// Link to parent event block for hierarchies (0 = no parent).
    pub parent_ev_addr: u64,
    /// Link to range begin/end event (0 = no range link).
    pub range_ev_addr: u64,
    /// Link to TX block containing event name (0 = no name).
    pub name_addr: u64,
    /// Link to MD block containing comment (0 = no comment).
    pub comment_addr: u64,

    // === Variable Links ===
    /// Links to scope references (DG, CG, or CN blocks).
    pub scope_addrs: Vec<u64>,
    /// Links to attachment references (AT blocks).
    pub attachment_addrs: Vec<u64>,

    // === Data Section ===
    /// Event type (recording, trigger, marker).
    pub event_type: EventType,
    /// Synchronization type (time, angle, distance, index).
    pub sync_type: EventSyncType,
    /// Range type (point, begin, end).
    pub range_type: EventRangeType,
    /// Event cause (other, error, tool, script, user).
    pub cause: EventCause,
    /// Event flags (bit 0 = post-processing event).
    pub flags: u8,
    /// Number of scope references.
    pub scope_count: u32,
    /// Number of attachment references.
    pub attachment_count: u16,
    /// Index of creator FH block (0 = first FH).
    pub creator_index: u16,
    /// Base value for synchronization (raw value before factor).
    pub sync_base_value: i64,
    /// Factor to convert sync_base_value to physical value.
    pub sync_factor: f64,
}

impl BlockParse<'_> for EventBlock {
    const ID: &'static str = "##EV";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        // Calculate link count from header
        let link_count = header.link_count as usize;

        // Data section starts after header + links
        let data_offset = 24 + link_count * 8;
        validate_buffer_size(bytes, data_offset + 32)?;

        // Parse data section first to get scope/attachment counts
        let event_type_raw = read_u8(bytes, data_offset);
        let sync_type_raw = read_u8(bytes, data_offset + 1);
        let range_type_raw = read_u8(bytes, data_offset + 2);
        let cause_raw = read_u8(bytes, data_offset + 3);
        let flags = read_u8(bytes, data_offset + 4);
        // bytes data_offset+5..data_offset+8 are reserved
        let scope_count = read_u32(bytes, data_offset + 8);
        let attachment_count = read_u16(bytes, data_offset + 12);
        let creator_index = read_u16(bytes, data_offset + 14);
        let sync_base_value = read_u64(bytes, data_offset + 16) as i64;
        let sync_factor = f64::from_le_bytes([
            bytes[data_offset + 24],
            bytes[data_offset + 25],
            bytes[data_offset + 26],
            bytes[data_offset + 27],
            bytes[data_offset + 28],
            bytes[data_offset + 29],
            bytes[data_offset + 30],
            bytes[data_offset + 31],
        ]);

        // Parse fixed links (5)
        let next_ev_addr = read_u64(bytes, 24);
        let parent_ev_addr = read_u64(bytes, 32);
        let range_ev_addr = read_u64(bytes, 40);
        let name_addr = read_u64(bytes, 48);
        let comment_addr = read_u64(bytes, 56);

        // Parse variable links (scope + attachment)
        let mut scope_addrs = Vec::with_capacity(scope_count as usize);
        let mut attachment_addrs = Vec::with_capacity(attachment_count as usize);

        let scope_start = 64; // After 5 fixed links
        for i in 0..scope_count as usize {
            scope_addrs.push(read_u64(bytes, scope_start + i * 8));
        }

        let attachment_start = scope_start + scope_count as usize * 8;
        for i in 0..attachment_count as usize {
            attachment_addrs.push(read_u64(bytes, attachment_start + i * 8));
        }

        // Convert enum types with defaults for unknown values
        let event_type = EventType::from_u8(event_type_raw).unwrap_or(EventType::Marker);
        let sync_type = EventSyncType::from_u8(sync_type_raw).unwrap_or(EventSyncType::Time);
        let range_type = EventRangeType::from_u8(range_type_raw).unwrap_or(EventRangeType::Point);
        let cause = EventCause::from_u8(cause_raw).unwrap_or(EventCause::Other);

        Ok(Self {
            header,
            next_ev_addr,
            parent_ev_addr,
            range_ev_addr,
            name_addr,
            comment_addr,
            scope_addrs,
            attachment_addrs,
            event_type,
            sync_type,
            range_type,
            cause,
            flags,
            scope_count,
            attachment_count,
            creator_index,
            sync_base_value,
            sync_factor,
        })
    }
}

impl EventBlock {
    /// Creates a new EventBlock with the given parameters.
    ///
    /// # Arguments
    /// * `event_type` - Type of event (recording, trigger, marker)
    /// * `sync_type` - Synchronization type
    /// * `sync_value` - Synchronization value (time in seconds, etc.)
    pub fn new(event_type: EventType, sync_type: EventSyncType, sync_value: f64) -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##EV"),
                reserved: 0,
                length: EV_BLOCK_SIZE as u64,
                link_count: 5, // 5 fixed links, no scopes or attachments
            },
            next_ev_addr: 0,
            parent_ev_addr: 0,
            range_ev_addr: 0,
            name_addr: 0,
            comment_addr: 0,
            scope_addrs: Vec::new(),
            attachment_addrs: Vec::new(),
            event_type,
            sync_type,
            range_type: EventRangeType::Point,
            cause: EventCause::Other,
            flags: 0,
            scope_count: 0,
            attachment_count: 0,
            creator_index: 0,
            sync_base_value: sync_value as i64,
            sync_factor: 1.0,
        }
    }

    /// Creates a marker event at the given time.
    pub fn marker(time_s: f64) -> Self {
        Self::new(EventType::Marker, EventSyncType::Time, time_s)
    }

    /// Creates a trigger event at the given time.
    pub fn trigger(time_s: f64) -> Self {
        Self::new(EventType::Trigger, EventSyncType::Time, time_s)
    }

    /// Returns the physical synchronization value.
    ///
    /// This is `sync_base_value * sync_factor`, giving the actual time/angle/distance.
    pub fn sync_value(&self) -> f64 {
        self.sync_base_value as f64 * self.sync_factor
    }

    /// Returns true if this is a post-processing event.
    pub fn is_post_processing(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Serializes the EventBlock to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let link_count = 5 + self.scope_count as usize + self.attachment_count as usize;
        let total_size = 24 + link_count * 8 + 32;

        let mut buffer = Vec::with_capacity(total_size);

        // Update header with correct link count and size
        let mut header = self.header.clone();
        header.link_count = link_count as u64;
        header.length = total_size as u64;
        buffer.extend_from_slice(&header.to_bytes()?);

        // Fixed links (5)
        buffer.extend_from_slice(&self.next_ev_addr.to_le_bytes());
        buffer.extend_from_slice(&self.parent_ev_addr.to_le_bytes());
        buffer.extend_from_slice(&self.range_ev_addr.to_le_bytes());
        buffer.extend_from_slice(&self.name_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Variable links - scopes
        for addr in &self.scope_addrs {
            buffer.extend_from_slice(&addr.to_le_bytes());
        }

        // Variable links - attachments
        for addr in &self.attachment_addrs {
            buffer.extend_from_slice(&addr.to_le_bytes());
        }

        // Data section
        buffer.push(self.event_type as u8);
        buffer.push(self.sync_type as u8);
        buffer.push(self.range_type as u8);
        buffer.push(self.cause as u8);
        buffer.push(self.flags);
        buffer.extend_from_slice(&[0u8; 3]); // reserved
        buffer.extend_from_slice(&self.scope_count.to_le_bytes());
        buffer.extend_from_slice(&self.attachment_count.to_le_bytes());
        buffer.extend_from_slice(&self.creator_index.to_le_bytes());
        buffer.extend_from_slice(&self.sync_base_value.to_le_bytes());
        buffer.extend_from_slice(&self.sync_factor.to_le_bytes());

        Ok(buffer)
    }
}

impl Default for EventBlock {
    fn default() -> Self {
        Self::marker(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_ev_bytes(
        event_type: u8,
        sync_type: u8,
        range_type: u8,
        cause: u8,
        scope_count: u32,
        attachment_count: u16,
        sync_base: i64,
        sync_factor: f64,
    ) -> Vec<u8> {
        let link_count = 5 + scope_count as u64 + attachment_count as u64;
        let total_len = 24 + link_count as usize * 8 + 32;

        let mut bytes = Vec::with_capacity(total_len);

        // Block header (24 bytes)
        bytes.extend_from_slice(b"##EV");
        bytes.extend_from_slice(&0u32.to_le_bytes()); // reserved
        bytes.extend_from_slice(&(total_len as u64).to_le_bytes()); // length
        bytes.extend_from_slice(&link_count.to_le_bytes()); // link_count

        // Fixed links (5 x 8 = 40 bytes)
        bytes.extend_from_slice(&0u64.to_le_bytes()); // next_ev
        bytes.extend_from_slice(&0u64.to_le_bytes()); // parent_ev
        bytes.extend_from_slice(&0u64.to_le_bytes()); // range_ev
        bytes.extend_from_slice(&0u64.to_le_bytes()); // name
        bytes.extend_from_slice(&0u64.to_le_bytes()); // comment

        // Variable links - scopes
        for _ in 0..scope_count {
            bytes.extend_from_slice(&0u64.to_le_bytes());
        }

        // Variable links - attachments
        for _ in 0..attachment_count {
            bytes.extend_from_slice(&0u64.to_le_bytes());
        }

        // Data section (32 bytes)
        bytes.push(event_type);
        bytes.push(sync_type);
        bytes.push(range_type);
        bytes.push(cause);
        bytes.push(0); // flags
        bytes.extend_from_slice(&[0u8; 3]); // reserved
        bytes.extend_from_slice(&scope_count.to_le_bytes());
        bytes.extend_from_slice(&attachment_count.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // creator_index
        bytes.extend_from_slice(&sync_base.to_le_bytes());
        bytes.extend_from_slice(&sync_factor.to_le_bytes());

        bytes
    }

    #[test]
    fn parse_basic_event() {
        let bytes = create_ev_bytes(
            2, // Marker
            1, // Time
            0, // Point
            4, // User
            0, // no scopes
            0, // no attachments
            1000, 0.001,
        );

        let ev = EventBlock::from_bytes(&bytes).unwrap();
        assert_eq!(ev.event_type, EventType::Marker);
        assert_eq!(ev.sync_type, EventSyncType::Time);
        assert_eq!(ev.range_type, EventRangeType::Point);
        assert_eq!(ev.cause, EventCause::User);
        assert_eq!(ev.sync_base_value, 1000);
        assert!((ev.sync_factor - 0.001).abs() < 1e-10);
        assert!((ev.sync_value() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn parse_event_with_scopes() {
        let bytes = create_ev_bytes(
            1, // Trigger
            1, // Time
            1, // RangeBegin
            2, // Tool
            2, // 2 scopes
            0, // no attachments
            5000, 1.0,
        );

        let ev = EventBlock::from_bytes(&bytes).unwrap();
        assert_eq!(ev.event_type, EventType::Trigger);
        assert_eq!(ev.range_type, EventRangeType::RangeBegin);
        assert_eq!(ev.scope_count, 2);
        assert_eq!(ev.scope_addrs.len(), 2);
    }

    #[test]
    fn roundtrip() {
        let original = EventBlock {
            header: BlockHeader {
                id: String::from("##EV"),
                reserved: 0,
                length: EV_BLOCK_SIZE as u64,
                link_count: 5,
            },
            next_ev_addr: 0x1000,
            parent_ev_addr: 0x2000,
            range_ev_addr: 0,
            name_addr: 0x3000,
            comment_addr: 0x4000,
            scope_addrs: Vec::new(),
            attachment_addrs: Vec::new(),
            event_type: EventType::Trigger,
            sync_type: EventSyncType::Time,
            range_type: EventRangeType::Point,
            cause: EventCause::Tool,
            flags: 0x01,
            scope_count: 0,
            attachment_count: 0,
            creator_index: 1,
            sync_base_value: 123456789,
            sync_factor: 1e-9,
        };

        let bytes = original.to_bytes().unwrap();
        let parsed = EventBlock::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.next_ev_addr, original.next_ev_addr);
        assert_eq!(parsed.parent_ev_addr, original.parent_ev_addr);
        assert_eq!(parsed.name_addr, original.name_addr);
        assert_eq!(parsed.event_type, original.event_type);
        assert_eq!(parsed.sync_type, original.sync_type);
        assert_eq!(parsed.cause, original.cause);
        assert_eq!(parsed.flags, original.flags);
        assert_eq!(parsed.sync_base_value, original.sync_base_value);
        assert!((parsed.sync_factor - original.sync_factor).abs() < 1e-20);
    }

    #[test]
    fn marker_constructor() {
        let ev = EventBlock::marker(10.5);
        assert_eq!(ev.event_type, EventType::Marker);
        assert_eq!(ev.sync_type, EventSyncType::Time);
        assert_eq!(ev.sync_base_value, 10);
    }

    #[test]
    fn trigger_constructor() {
        let ev = EventBlock::trigger(5.0);
        assert_eq!(ev.event_type, EventType::Trigger);
        assert_eq!(ev.sync_type, EventSyncType::Time);
    }
}
