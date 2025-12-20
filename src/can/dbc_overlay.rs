//! Read-time DBC overlay for decoding raw CAN captures.
//!
//! This module provides [`DbcOverlayReader`], which applies DBC signal definitions
//! to raw CAN data stored in MDF4 files without modifying the original file.
//!
//! # Use Cases
//!
//! - Post-process raw captures with updated DBC files
//! - Decode signals you didn't know about during capture
//! - Apply different DBC versions to the same capture
//! - Preserve raw data while getting decoded values on-demand
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::can::DbcOverlayReader;
//! use mdf4_rs::FileRangeReader;
//!
//! // Load raw CAN capture and DBC
//! let dbc = dbc_rs::Dbc::from_file("vehicle.dbc")?;
//! let mut reader = FileRangeReader::new("raw_capture.mf4")?;
//!
//! // Create overlay reader
//! let overlay = DbcOverlayReader::from_file("raw_capture.mf4", &dbc)?;
//!
//! // Iterate over decoded frames for a specific message
//! for frame in overlay.frames("EngineData", &mut reader)? {
//!     println!("{}: {:?}", frame.timestamp_us, frame.signals);
//! }
//!
//! // Or get all values for a specific signal
//! for (timestamp, value) in overlay.signal_values("EngineRPM", &mut reader)? {
//!     println!("{}: {}", timestamp, value);
//! }
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use crate::index::{ByteRangeReader, IndexedChannelGroup, MdfIndex};
use crate::{DecodedValue, Error, Result};

/// A decoded CAN frame with all signal values.
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    /// Timestamp in microseconds
    pub timestamp_us: u64,
    /// CAN ID (with bit 31 set for extended IDs)
    pub can_id: u32,
    /// Whether this is an extended 29-bit ID
    pub is_extended: bool,
    /// Decoded signal values: (signal_name, physical_value)
    pub signals: Vec<(String, f64)>,
}

/// A timestamped signal value.
#[derive(Debug, Clone)]
pub struct SignalValue {
    /// Timestamp in microseconds
    pub timestamp_us: u64,
    /// Physical value after DBC conversion
    pub value: f64,
    /// Raw integer value before conversion
    pub raw_value: i64,
}

/// Information about a raw CAN channel group in the MDF file.
#[derive(Debug)]
struct RawCanGroup {
    /// Index in the MdfIndex channel_groups
    group_index: usize,
    /// Channel indices for the standard raw CAN format
    timestamp_channel: usize,
    can_id_channel: usize,
    dlc_channel: usize,
    ide_channel: Option<usize>,
    data_channels: Vec<usize>,
    /// Number of data bytes available
    data_byte_count: usize,
}

/// Read-time DBC overlay reader for decoding raw CAN captures.
///
/// This reader applies DBC signal definitions to raw CAN data stored in MDF4
/// files, providing decoded signal values without modifying the original file.
///
/// # Storage Format
///
/// The overlay reader expects raw CAN data in the format written by
/// [`RawCanLogger`](super::RawCanLogger):
/// - Timestamp (u64, microseconds)
/// - CAN_ID (u32)
/// - DLC (u8)
/// - FD_Flags (u8, optional)
/// - IDE (u8, 0=standard, 1=extended)
/// - Data_0..Data_N (u8 bytes)
///
/// # Thread Safety
///
/// The overlay reader is not thread-safe. For concurrent access, create
/// separate instances or use external synchronization.
pub struct DbcOverlayReader<'dbc> {
    /// The DBC file for signal decoding
    dbc: &'dbc dbc_rs::Dbc,
    /// The MDF index for efficient reading
    index: MdfIndex,
    /// Detected raw CAN groups in the MDF file
    raw_can_groups: Vec<RawCanGroup>,
}

impl<'dbc> DbcOverlayReader<'dbc> {
    /// Create a new overlay reader from an MDF file path and DBC.
    ///
    /// # Arguments
    /// * `mdf_path` - Path to the MDF4 file containing raw CAN data
    /// * `dbc` - The DBC file for signal decoding
    ///
    /// # Returns
    /// A new overlay reader, or an error if the file cannot be read or
    /// contains no recognizable raw CAN data.
    #[cfg(feature = "std")]
    pub fn from_file(mdf_path: &str, dbc: &'dbc dbc_rs::Dbc) -> Result<Self> {
        let index = MdfIndex::from_file(mdf_path)?;
        Self::from_index(index, dbc)
    }

    /// Create a new overlay reader from an existing MdfIndex and DBC.
    ///
    /// This is useful when you already have an index loaded or want to
    /// share the index with other readers.
    pub fn from_index(index: MdfIndex, dbc: &'dbc dbc_rs::Dbc) -> Result<Self> {
        let raw_can_groups = Self::detect_raw_can_groups(&index)?;

        if raw_can_groups.is_empty() {
            return Err(Error::BlockSerializationError(
                "No raw CAN channel groups found in MDF file".into(),
            ));
        }

        Ok(Self {
            dbc,
            index,
            raw_can_groups,
        })
    }

    /// Detect channel groups that contain raw CAN data.
    fn detect_raw_can_groups(index: &MdfIndex) -> Result<Vec<RawCanGroup>> {
        let mut groups = Vec::new();

        for (group_idx, group) in index.channel_groups.iter().enumerate() {
            if let Some(raw_group) = Self::try_parse_raw_can_group(group_idx, group) {
                groups.push(raw_group);
            }
        }

        Ok(groups)
    }

    /// Try to parse a channel group as raw CAN data.
    fn try_parse_raw_can_group(
        group_index: usize,
        group: &IndexedChannelGroup,
    ) -> Option<RawCanGroup> {
        // Look for the required channels by name
        let mut timestamp_channel = None;
        let mut can_id_channel = None;
        let mut dlc_channel = None;
        let mut ide_channel = None;
        let mut data_channels = Vec::new();

        for (ch_idx, channel) in group.channels.iter().enumerate() {
            let name = match &channel.name {
                Some(n) => n.as_str(),
                None => continue,
            };
            match name {
                "Timestamp" => timestamp_channel = Some(ch_idx),
                "CAN_ID" => can_id_channel = Some(ch_idx),
                "DLC" => dlc_channel = Some(ch_idx),
                "IDE" => ide_channel = Some(ch_idx),
                n if n.starts_with("Data_") => {
                    // Parse the data channel index
                    if let Ok(idx) = n[5..].parse::<usize>() {
                        // Ensure we have enough space
                        if data_channels.len() <= idx {
                            data_channels.resize(idx + 1, None);
                        }
                        data_channels[idx] = Some(ch_idx);
                    }
                }
                _ => {}
            }
        }

        // Check if we have the minimum required channels
        let timestamp = timestamp_channel?;
        let can_id = can_id_channel?;
        let dlc = dlc_channel?;

        // Convert data channel options to indices
        let data_channels: Vec<usize> = data_channels.into_iter().flatten().collect();

        if data_channels.is_empty() {
            return None;
        }

        Some(RawCanGroup {
            group_index,
            timestamp_channel: timestamp,
            can_id_channel: can_id,
            dlc_channel: dlc,
            ide_channel,
            data_channels: data_channels.clone(),
            data_byte_count: data_channels.len(),
        })
    }

    /// Get the number of raw CAN channel groups found.
    pub fn raw_group_count(&self) -> usize {
        self.raw_can_groups.len()
    }

    /// Get the underlying MDF index.
    pub fn index(&self) -> &MdfIndex {
        &self.index
    }

    /// Get the DBC being used for decoding.
    pub fn dbc(&self) -> &dbc_rs::Dbc {
        self.dbc
    }

    /// Read all raw frames from the MDF file.
    ///
    /// Returns a vector of (timestamp_us, can_id, is_extended, data) tuples.
    #[allow(clippy::type_complexity)]
    pub fn read_raw_frames<R: ByteRangeReader<Error = Error>>(
        &self,
        reader: &mut R,
    ) -> Result<Vec<(u64, u32, bool, Vec<u8>)>> {
        let mut frames = Vec::new();

        for raw_group in &self.raw_can_groups {
            // Read all channel values for this group
            let timestamps = self.index.read_channel_values(
                raw_group.group_index,
                raw_group.timestamp_channel,
                reader,
            )?;
            let can_ids = self.index.read_channel_values(
                raw_group.group_index,
                raw_group.can_id_channel,
                reader,
            )?;
            let dlcs = self.index.read_channel_values(
                raw_group.group_index,
                raw_group.dlc_channel,
                reader,
            )?;

            let ides = if let Some(ide_ch) = raw_group.ide_channel {
                self.index
                    .read_channel_values(raw_group.group_index, ide_ch, reader)?
            } else {
                vec![Some(DecodedValue::UnsignedInteger(0)); timestamps.len()]
            };

            // Read data channels
            let mut data_columns: Vec<Vec<Option<DecodedValue>>> = Vec::new();
            for &data_ch in &raw_group.data_channels {
                let values =
                    self.index
                        .read_channel_values(raw_group.group_index, data_ch, reader)?;
                data_columns.push(values);
            }

            // Combine into frames
            let record_count = timestamps.len();
            for i in 0..record_count {
                let timestamp = match &timestamps[i] {
                    Some(DecodedValue::UnsignedInteger(v)) => *v,
                    Some(DecodedValue::SignedInteger(v)) => *v as u64,
                    _ => continue,
                };

                let can_id = match &can_ids[i] {
                    Some(DecodedValue::UnsignedInteger(v)) => *v as u32,
                    Some(DecodedValue::SignedInteger(v)) => *v as u32,
                    _ => continue,
                };

                let dlc = match &dlcs[i] {
                    Some(DecodedValue::UnsignedInteger(v)) => *v as u8,
                    Some(DecodedValue::SignedInteger(v)) => *v as u8,
                    _ => continue,
                };

                let is_extended = match &ides[i] {
                    Some(DecodedValue::UnsignedInteger(v)) => *v != 0,
                    Some(DecodedValue::SignedInteger(v)) => *v != 0,
                    _ => false,
                };

                // Extract data bytes
                let data_len = super::fd::dlc_to_len(dlc).min(raw_group.data_byte_count);
                let mut data = vec![0u8; data_len];
                for (byte_idx, column) in data_columns.iter().enumerate() {
                    if byte_idx >= data_len {
                        break;
                    }
                    if let Some(Some(DecodedValue::UnsignedInteger(v))) = column.get(i) {
                        data[byte_idx] = *v as u8;
                    }
                }

                frames.push((timestamp, can_id, is_extended, data));
            }
        }

        // Sort by timestamp
        frames.sort_by_key(|(ts, _, _, _)| *ts);

        Ok(frames)
    }

    /// Read and decode all frames for a specific DBC message.
    ///
    /// # Arguments
    /// * `message_name` - The message name as defined in the DBC file
    /// * `reader` - A byte range reader for the MDF file
    ///
    /// # Returns
    /// A vector of decoded frames with all signal values.
    pub fn frames<R: ByteRangeReader<Error = Error>>(
        &self,
        message_name: &str,
        reader: &mut R,
    ) -> Result<Vec<DecodedFrame>> {
        // Find the message in the DBC by name
        let message = self
            .dbc
            .messages()
            .iter()
            .find(|m| m.name() == message_name)
            .ok_or_else(|| {
                Error::BlockSerializationError(alloc::format!(
                    "Message '{}' not found in DBC",
                    message_name
                ))
            })?;

        let msg_id = message.id();
        let raw_frames = self.read_raw_frames(reader)?;

        let mut decoded_frames = Vec::new();

        for (timestamp, can_id, is_extended, data) in raw_frames {
            // Match CAN ID (handle extended bit)
            let frame_id = if is_extended {
                can_id | 0x8000_0000
            } else {
                can_id
            };

            if frame_id != msg_id {
                continue;
            }

            // Decode using DBC
            if let Ok(decoded_signals) = self.dbc.decode(can_id, &data, is_extended) {
                let signals: Vec<(String, f64)> = decoded_signals
                    .iter()
                    .map(|s| (String::from(s.name), s.value))
                    .collect();

                decoded_frames.push(DecodedFrame {
                    timestamp_us: timestamp,
                    can_id,
                    is_extended,
                    signals,
                });
            }
        }

        Ok(decoded_frames)
    }

    /// Read all values for a specific signal across the entire capture.
    ///
    /// # Arguments
    /// * `signal_name` - The signal name as defined in the DBC file
    /// * `reader` - A byte range reader for the MDF file
    ///
    /// # Returns
    /// A vector of signal values with timestamps.
    pub fn signal_values<R: ByteRangeReader<Error = Error>>(
        &self,
        signal_name: &str,
        reader: &mut R,
    ) -> Result<Vec<SignalValue>> {
        // Find which message contains this signal
        let (message, _signal) = self
            .dbc
            .messages()
            .iter()
            .find_map(|msg| {
                msg.signals()
                    .iter()
                    .find(|s| s.name() == signal_name)
                    .map(|sig| (msg, sig))
            })
            .ok_or_else(|| {
                Error::BlockSerializationError(alloc::format!(
                    "Signal '{}' not found in DBC",
                    signal_name
                ))
            })?;

        let msg_id = message.id();
        let raw_frames = self.read_raw_frames(reader)?;

        let mut values = Vec::new();

        for (timestamp, can_id, is_extended, data) in raw_frames {
            // Match CAN ID
            let frame_id = if is_extended {
                can_id | 0x8000_0000
            } else {
                can_id
            };

            if frame_id != msg_id {
                continue;
            }

            // Decode using DBC
            if let Ok(decoded_signals) = self.dbc.decode(can_id, &data, is_extended) {
                if let Some(sig) = decoded_signals.iter().find(|s| s.name == signal_name) {
                    values.push(SignalValue {
                        timestamp_us: timestamp,
                        value: sig.value,
                        raw_value: sig.raw_value,
                    });
                }
            }
        }

        Ok(values)
    }

    /// Get all unique CAN IDs found in the raw capture.
    pub fn can_ids<R: ByteRangeReader<Error = Error>>(&self, reader: &mut R) -> Result<Vec<u32>> {
        use alloc::collections::BTreeSet;

        let frames = self.read_raw_frames(reader)?;
        let ids: BTreeSet<u32> = frames.iter().map(|(_, id, _, _)| *id).collect();
        Ok(ids.into_iter().collect())
    }

    /// Get statistics about the raw capture.
    pub fn statistics<R: ByteRangeReader<Error = Error>>(
        &self,
        reader: &mut R,
    ) -> Result<OverlayStatistics> {
        let frames = self.read_raw_frames(reader)?;

        let total_frames = frames.len();
        let unique_ids: alloc::collections::BTreeSet<u32> =
            frames.iter().map(|(_, id, _, _)| *id).collect();

        let (min_timestamp, max_timestamp) = if frames.is_empty() {
            (0, 0)
        } else {
            let min = frames.iter().map(|(ts, _, _, _)| *ts).min().unwrap_or(0);
            let max = frames.iter().map(|(ts, _, _, _)| *ts).max().unwrap_or(0);
            (min, max)
        };

        // Count how many messages from DBC are present
        let mut dbc_messages_found = 0;
        for msg in self.dbc.messages().iter() {
            if unique_ids.contains(&msg.id()) {
                dbc_messages_found += 1;
            }
        }

        Ok(OverlayStatistics {
            total_frames,
            unique_can_ids: unique_ids.len(),
            dbc_messages_found,
            dbc_messages_total: self.dbc.messages().len(),
            min_timestamp_us: min_timestamp,
            max_timestamp_us: max_timestamp,
            duration_us: max_timestamp.saturating_sub(min_timestamp),
        })
    }

    /// List all messages from the DBC that have data in this capture.
    pub fn available_messages<R: ByteRangeReader<Error = Error>>(
        &self,
        reader: &mut R,
    ) -> Result<Vec<String>> {
        let can_ids = self.can_ids(reader)?;
        let can_id_set: alloc::collections::BTreeSet<u32> = can_ids.into_iter().collect();

        let mut messages = Vec::new();
        for msg in self.dbc.messages().iter() {
            if can_id_set.contains(&msg.id()) {
                messages.push(String::from(msg.name()));
            }
        }

        Ok(messages)
    }

    /// List all signals that can be decoded from this capture.
    pub fn available_signals<R: ByteRangeReader<Error = Error>>(
        &self,
        reader: &mut R,
    ) -> Result<Vec<String>> {
        let can_ids = self.can_ids(reader)?;
        let can_id_set: alloc::collections::BTreeSet<u32> = can_ids.into_iter().collect();

        let mut signals = Vec::new();
        for msg in self.dbc.messages().iter() {
            if can_id_set.contains(&msg.id()) {
                for sig in msg.signals().iter() {
                    signals.push(String::from(sig.name()));
                }
            }
        }

        Ok(signals)
    }
}

/// Statistics about a raw CAN capture with DBC overlay.
#[derive(Debug, Clone)]
pub struct OverlayStatistics {
    /// Total number of CAN frames in the capture
    pub total_frames: usize,
    /// Number of unique CAN IDs
    pub unique_can_ids: usize,
    /// Number of DBC messages that have data in this capture
    pub dbc_messages_found: usize,
    /// Total number of messages defined in the DBC
    pub dbc_messages_total: usize,
    /// Earliest timestamp in microseconds
    pub min_timestamp_us: u64,
    /// Latest timestamp in microseconds
    pub max_timestamp_us: u64,
    /// Duration of capture in microseconds
    pub duration_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dbc() -> dbc_rs::Dbc {
        dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
 SG_ Temp : 16|8@1+ (1,-40) [-40|215] "C" Vector__XXX

BO_ 512 Transmission : 8 ECM
 SG_ Gear : 0|4@1+ (1,0) [0|6] "" Vector__XXX
 SG_ Speed : 8|16@1+ (0.01,0) [0|300] "km/h" Vector__XXX
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_overlay_with_raw_capture() {
        use crate::can::RawCanLogger;

        // Create a raw CAN capture
        let mut logger = RawCanLogger::new().unwrap();

        // Log Engine frames (ID 0x100 = 256)
        // RPM = 2000 (raw 8000 = 0x1F40), Temp = 50°C (raw 90 = 0x5A)
        logger.log(256, 1000, &[0x40, 0x1F, 0x5A, 0, 0, 0, 0, 0]);
        logger.log(256, 2000, &[0x80, 0x3E, 0x64, 0, 0, 0, 0, 0]); // RPM=4000, Temp=60

        // Log Transmission frames (ID 0x200 = 512)
        // Gear = 3, Speed = 50 km/h (raw 5000 = 0x1388)
        logger.log(512, 1500, &[0x03, 0x88, 0x13, 0, 0, 0, 0, 0]);

        let mdf_bytes = logger.finalize().unwrap();

        // Write to temp file
        let temp_path = std::env::temp_dir().join("overlay_test.mf4");
        std::fs::write(&temp_path, &mdf_bytes).unwrap();

        // Create overlay reader
        let dbc = create_test_dbc();
        let overlay = DbcOverlayReader::from_file(temp_path.to_str().unwrap(), &dbc).unwrap();

        // Check raw group detection
        assert_eq!(overlay.raw_group_count(), 2); // One per CAN ID

        // Read with file reader
        let mut reader = crate::FileRangeReader::new(temp_path.to_str().unwrap()).unwrap();

        // Check statistics
        let stats = overlay.statistics(&mut reader).unwrap();
        assert_eq!(stats.total_frames, 3);
        assert_eq!(stats.unique_can_ids, 2);
        assert_eq!(stats.dbc_messages_found, 2);

        // Check available messages
        let messages = overlay.available_messages(&mut reader).unwrap();
        assert!(messages.contains(&String::from("Engine")));
        assert!(messages.contains(&String::from("Transmission")));

        // Read Engine frames
        let engine_frames = overlay.frames("Engine", &mut reader).unwrap();
        assert_eq!(engine_frames.len(), 2);

        // Check first frame
        let frame = &engine_frames[0];
        assert_eq!(frame.timestamp_us, 1000);
        assert_eq!(frame.can_id, 256);

        // Find RPM signal
        let rpm = frame
            .signals
            .iter()
            .find(|(name, _)| name == "RPM")
            .unwrap();
        assert!((rpm.1 - 2000.0).abs() < 0.1);

        // Read RPM signal values directly
        let rpm_values = overlay.signal_values("RPM", &mut reader).unwrap();
        assert_eq!(rpm_values.len(), 2);
        assert!((rpm_values[0].value - 2000.0).abs() < 0.1);
        assert!((rpm_values[1].value - 4000.0).abs() < 0.1);

        // Cleanup
        std::fs::remove_file(&temp_path).ok();
    }

    #[test]
    fn test_overlay_signal_not_found() {
        use crate::can::RawCanLogger;

        let mut logger = RawCanLogger::new().unwrap();
        logger.log(256, 1000, &[0x40, 0x1F, 0x5A, 0, 0, 0, 0, 0]);

        let mdf_bytes = logger.finalize().unwrap();

        let temp_path = std::env::temp_dir().join("overlay_notfound.mf4");
        std::fs::write(&temp_path, &mdf_bytes).unwrap();

        let dbc = create_test_dbc();
        let overlay = DbcOverlayReader::from_file(temp_path.to_str().unwrap(), &dbc).unwrap();

        let mut reader = crate::FileRangeReader::new(temp_path.to_str().unwrap()).unwrap();

        // Try to read non-existent signal
        let result = overlay.signal_values("NonExistent", &mut reader);
        assert!(result.is_err());

        // Try to read non-existent message
        let result = overlay.frames("NonExistent", &mut reader);
        assert!(result.is_err());

        std::fs::remove_file(&temp_path).ok();
    }

    #[test]
    fn test_overlay_extended_ids() {
        use crate::can::RawCanLogger;

        // Create DBC with extended ID message
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 2365587201 J1939_EEC1 : 8 ECM
 SG_ EngineSpeed : 24|16@1+ (0.125,0) [0|8031.875] "rpm" Vector__XXX
"#,
        )
        .unwrap();

        let mut logger = RawCanLogger::new().unwrap();

        // Log extended ID frame (J1939 PGN)
        // Note: 2365587201 = 0x8CF00401, but we store without the extended bit
        let pgn_id = 0x0CF00401; // Without extended bit
        logger.log_extended(pgn_id, 1000, &[0, 0, 0, 0x00, 0x20, 0, 0, 0]); // EngineSpeed = 1024 rpm

        let mdf_bytes = logger.finalize().unwrap();

        let temp_path = std::env::temp_dir().join("overlay_extended.mf4");
        std::fs::write(&temp_path, &mdf_bytes).unwrap();

        let overlay = DbcOverlayReader::from_file(temp_path.to_str().unwrap(), &dbc).unwrap();
        let mut reader = crate::FileRangeReader::new(temp_path.to_str().unwrap()).unwrap();

        // Check raw frames
        let raw_frames = overlay.read_raw_frames(&mut reader).unwrap();
        assert_eq!(raw_frames.len(), 1);
        assert!(raw_frames[0].2); // is_extended = true

        std::fs::remove_file(&temp_path).ok();
    }
}
