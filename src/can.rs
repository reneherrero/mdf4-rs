//! CAN bus integration for MDF4 files.
//!
//! This module provides utilities for efficiently logging CAN bus data to MDF4 files.
//! It integrates with the [`embedded-can`](https://crates.io/crates/embedded-can) crate
//! to provide a hardware-agnostic interface for CAN frame logging.
//!
//! # Features
//!
//! - Zero-copy signal extraction from CAN frames
//! - Batch processing for efficient logging
//! - Support for both Standard (11-bit) and Extended (29-bit) CAN IDs
//! - DBC file integration (with `dbc` feature)
//!
//! # Example with DBC file
//!
//! ```ignore
//! use mdf4_rs::can::DbcMdfLogger;
//! use dbc_rs::Dbc;
//!
//! // Load signal definitions from a DBC file
//! let dbc = Dbc::parse(dbc_content)?;
//! let mut logger = DbcMdfLogger::new(&dbc)?;
//!
//! // Log CAN frames
//! for frame in can_frames {
//!     logger.log_frame(timestamp, frame)?;
//! }
//!
//! // Get the MDF file bytes
//! let mdf_bytes = logger.finalize()?;
//! ```

use alloc::string::String;
use alloc::vec::Vec;

// Re-export dbc-rs types when available
#[cfg(feature = "dbc")]
pub use dbc_rs::{Dbc, Message, Signal};

/// Byte order for CAN signal extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// Little-endian (Intel) byte order
    LittleEndian,
    /// Big-endian (Motorola) byte order
    BigEndian,
}

#[cfg(feature = "dbc")]
impl From<dbc_rs::ByteOrder> for ByteOrder {
    fn from(order: dbc_rs::ByteOrder) -> Self {
        match order {
            dbc_rs::ByteOrder::LittleEndian => ByteOrder::LittleEndian,
            dbc_rs::ByteOrder::BigEndian => ByteOrder::BigEndian,
        }
    }
}

/// Definition of a CAN signal within a frame.
///
/// This follows the DBC file format conventions for signal definition.
#[derive(Debug, Clone)]
pub struct SignalDefinition {
    /// CAN ID this signal belongs to (11-bit or 29-bit)
    pub can_id: u32,
    /// Signal name for the MDF channel
    pub name: String,
    /// Start bit position within the CAN frame data (0-63)
    pub start_bit: u16,
    /// Number of bits for this signal (1-64)
    pub bit_length: u16,
    /// Scale factor: physical_value = raw_value * scale + offset
    pub scale: f64,
    /// Offset: physical_value = raw_value * scale + offset
    pub offset: f64,
    /// Whether the signal is signed
    pub is_signed: bool,
    /// Byte order of the signal
    pub byte_order: ByteOrder,
    /// Unit string (optional)
    pub unit: Option<String>,
}

impl SignalDefinition {
    /// Create a new unsigned signal definition with default scale (1.0) and offset (0.0).
    pub fn new(can_id: u32, name: &str, start_bit: u16, bit_length: u16) -> Self {
        Self {
            can_id,
            name: String::from(name),
            start_bit,
            bit_length,
            scale: 1.0,
            offset: 0.0,
            is_signed: false,
            byte_order: ByteOrder::LittleEndian,
            unit: None,
        }
    }

    /// Set the scale factor.
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Set the offset.
    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }

    /// Set whether the signal is signed.
    pub fn signed(mut self) -> Self {
        self.is_signed = true;
        self
    }

    /// Set the byte order to big-endian (Motorola).
    pub fn big_endian(mut self) -> Self {
        self.byte_order = ByteOrder::BigEndian;
        self
    }

    /// Set the unit string.
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(String::from(unit));
        self
    }
}

/// Create a SignalDefinition from a dbc-rs Signal.
#[cfg(feature = "dbc")]
impl SignalDefinition {
    /// Create a SignalDefinition from a dbc-rs Signal and message ID.
    pub fn from_dbc_signal(signal: &dbc_rs::Signal, message_id: u32) -> Self {
        Self {
            can_id: message_id,
            name: String::from(signal.name()),
            start_bit: signal.start_bit(),
            bit_length: signal.length(),
            scale: signal.factor(),
            offset: signal.offset(),
            is_signed: !signal.is_unsigned(),
            byte_order: signal.byte_order().into(),
            unit: signal.unit().map(String::from),
        }
    }
}

/// A CAN frame with timestamp for logging.
///
/// This is a simple container that pairs a CAN frame with a timestamp.
/// Use this when you need to associate timing information with frames.
#[derive(Debug, Clone)]
pub struct TimestampedFrame<F> {
    /// Timestamp in microseconds since start of logging
    pub timestamp_us: u64,
    /// The CAN frame
    pub frame: F,
}

impl<F> TimestampedFrame<F> {
    /// Create a new timestamped frame.
    pub fn new(timestamp_us: u64, frame: F) -> Self {
        Self { timestamp_us, frame }
    }
}

// =============================================================================
// Signal extraction - uses dbc-rs when available, otherwise our implementation
// =============================================================================

/// Extract a raw signal value from CAN frame data.
#[inline]
pub fn extract_signal_raw(data: &[u8], start_bit: u16, bit_length: u16, byte_order: ByteOrder) -> u64 {
    if data.is_empty() || bit_length == 0 || bit_length > 64 {
        return 0;
    }

    match byte_order {
        ByteOrder::LittleEndian => extract_le(data, start_bit, bit_length),
        ByteOrder::BigEndian => extract_be(data, start_bit, bit_length),
    }
}

/// Extract little-endian signal.
#[inline]
fn extract_le(data: &[u8], start_bit: u16, bit_length: u16) -> u64 {
    let start_byte = (start_bit / 8) as usize;
    let bit_offset = start_bit % 8;

    let mut value: u64 = 0;
    let bytes_needed = ((bit_offset as usize + bit_length as usize) + 7) / 8;

    for i in 0..bytes_needed.min(8) {
        if start_byte + i < data.len() {
            value |= (data[start_byte + i] as u64) << (i * 8);
        }
    }

    value >>= bit_offset;
    value & ((1u64 << bit_length) - 1)
}

/// Extract big-endian (Motorola) signal.
#[inline]
fn extract_be(data: &[u8], start_bit: u16, bit_length: u16) -> u64 {
    let start_byte = (start_bit / 8) as usize;
    let bit_in_byte = start_bit % 8;

    let end_bit = start_bit as i16 - bit_length as i16 + 1;
    if end_bit < 0 {
        let mut value: u64 = 0;
        let mut bits_remaining = bit_length;
        let mut current_byte = start_byte;
        let mut current_bit = bit_in_byte as i8;

        while bits_remaining > 0 && current_byte < data.len() {
            let bits_in_this_byte = ((current_bit + 1) as u16).min(bits_remaining);
            let mask = ((1u64 << bits_in_this_byte) - 1) as u8;
            let shift = (current_bit + 1) as u16 - bits_in_this_byte;
            let byte_value = (data[current_byte] >> shift as u8) & mask;

            value = (value << bits_in_this_byte) | byte_value as u64;
            bits_remaining -= bits_in_this_byte;
            current_byte += 1;
            current_bit = 7;
        }

        value
    } else {
        let mut value: u64 = 0;
        let mut bits_remaining = bit_length;
        let mut current_byte = start_byte;
        let mut current_bit = bit_in_byte;

        while bits_remaining > 0 && current_byte < data.len() {
            let bits_available = current_bit + 1;
            let bits_to_take = bits_available.min(bits_remaining);
            let shift = current_bit + 1 - bits_to_take;
            let mask = ((1u64 << bits_to_take) - 1) as u8;
            let byte_value = (data[current_byte] >> shift) & mask;

            value = (value << bits_to_take) | byte_value as u64;
            bits_remaining -= bits_to_take;
            current_byte += 1;
            current_bit = 7;
        }

        value
    }
}

/// Convert a raw signal value to a signed integer if needed.
#[inline]
pub fn sign_extend(value: u64, bit_length: u16) -> i64 {
    let sign_bit = 1u64 << (bit_length - 1);
    if value & sign_bit != 0 {
        let mask = !((1u64 << bit_length) - 1);
        (value | mask) as i64
    } else {
        value as i64
    }
}

/// Apply scale and offset conversion to a raw value.
#[inline]
pub fn apply_conversion(raw: u64, is_signed: bool, bit_length: u16, scale: f64, offset: f64) -> f64 {
    let value = if is_signed {
        sign_extend(raw, bit_length) as f64
    } else {
        raw as f64
    };
    value * scale + offset
}

/// Extract a signal from CAN frame data and apply conversion.
#[inline]
pub fn extract_signal(data: &[u8], def: &SignalDefinition) -> f64 {
    let raw = extract_signal_raw(data, def.start_bit, def.bit_length, def.byte_order);
    apply_conversion(raw, def.is_signed, def.bit_length, def.scale, def.offset)
}

/// Pre-computed signal extractor for optimized batch processing.
#[derive(Debug, Clone)]
pub struct SignalExtractor {
    signals: Vec<SignalDefinition>,
    can_ids: Vec<u32>,
}

impl SignalExtractor {
    /// Create a new signal extractor with the given definitions.
    pub fn new(signals: Vec<SignalDefinition>) -> Self {
        let mut can_ids: Vec<u32> = signals.iter().map(|s| s.can_id).collect();
        can_ids.sort_unstable();
        can_ids.dedup();

        Self { signals, can_ids }
    }

    /// Create a signal extractor from a DBC file.
    #[cfg(feature = "dbc")]
    pub fn from_dbc(dbc: &dbc_rs::Dbc) -> Self {
        let mut signals = Vec::new();

        for message in dbc.messages().iter() {
            let msg_id = message.id();
            for signal in message.signals().iter() {
                signals.push(SignalDefinition::from_dbc_signal(signal, msg_id));
            }
        }

        Self::new(signals)
    }

    /// Check if we're interested in a given CAN ID.
    #[inline]
    pub fn has_can_id(&self, can_id: u32) -> bool {
        self.can_ids.binary_search(&can_id).is_ok()
    }

    /// Get all signals for a given CAN ID.
    pub fn signals_for_id(&self, can_id: u32) -> impl Iterator<Item = &SignalDefinition> {
        self.signals.iter().filter(move |s| s.can_id == can_id)
    }

    /// Extract all signals from a CAN frame.
    pub fn extract_all<'a>(&'a self, can_id: u32, data: &'a [u8]) -> impl Iterator<Item = (&'a str, f64)> + 'a {
        self.signals_for_id(can_id).map(move |def| {
            let value = extract_signal(data, def);
            (def.name.as_str(), value)
        })
    }

    /// Get all signal definitions.
    pub fn signals(&self) -> &[SignalDefinition] {
        &self.signals
    }

    /// Get all CAN IDs.
    pub fn can_ids(&self) -> &[u32] {
        &self.can_ids
    }
}

/// Efficient buffer for accumulating CAN data before writing to MDF.
#[derive(Debug)]
pub struct CanDataBuffer {
    entries: Vec<CanIdBuffer>,
    extractor: SignalExtractor,
}

#[derive(Debug)]
struct CanIdBuffer {
    can_id: u32,
    timestamps: Vec<u64>,
    signal_values: Vec<Vec<f64>>,
}

impl CanDataBuffer {
    /// Create a new CAN data buffer with the given signal definitions.
    pub fn new(signals: Vec<SignalDefinition>) -> Self {
        let extractor = SignalExtractor::new(signals);
        Self::from_extractor(extractor)
    }

    /// Create a CAN data buffer from a signal extractor.
    pub fn from_extractor(extractor: SignalExtractor) -> Self {
        let mut entries = Vec::new();

        for &can_id in extractor.can_ids() {
            let signal_count = extractor.signals_for_id(can_id).count();
            entries.push(CanIdBuffer {
                can_id,
                timestamps: Vec::new(),
                signal_values: (0..signal_count).map(|_| Vec::new()).collect(),
            });
        }

        Self { entries, extractor }
    }

    /// Create a CAN data buffer from a DBC file.
    #[cfg(feature = "dbc")]
    pub fn from_dbc(dbc: &dbc_rs::Dbc) -> Self {
        let extractor = SignalExtractor::from_dbc(dbc);
        Self::from_extractor(extractor)
    }

    /// Add a CAN frame to the buffer.
    #[inline]
    pub fn push(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.can_id == can_id) {
            entry.timestamps.push(timestamp_us);

            for (i, def) in self.extractor.signals_for_id(can_id).enumerate() {
                let value = extract_signal(data, def);
                entry.signal_values[i].push(value);
            }
            true
        } else {
            false
        }
    }

    /// Get the number of frames buffered for a given CAN ID.
    pub fn frame_count(&self, can_id: u32) -> usize {
        self.entries
            .iter()
            .find(|e| e.can_id == can_id)
            .map(|e| e.timestamps.len())
            .unwrap_or(0)
    }

    /// Get all unique CAN IDs in the buffer.
    pub fn can_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|e| e.can_id)
    }

    /// Get timestamps for a given CAN ID.
    pub fn timestamps(&self, can_id: u32) -> Option<&[u64]> {
        self.entries
            .iter()
            .find(|e| e.can_id == can_id)
            .map(|e| e.timestamps.as_slice())
    }

    /// Get extracted signal values for a given CAN ID and signal index.
    pub fn signal_values(&self, can_id: u32, signal_index: usize) -> Option<&[f64]> {
        self.entries
            .iter()
            .find(|e| e.can_id == can_id)
            .and_then(|e| e.signal_values.get(signal_index))
            .map(|v| v.as_slice())
    }

    /// Get signal definitions for a given CAN ID.
    pub fn signals_for_id(&self, can_id: u32) -> impl Iterator<Item = &SignalDefinition> {
        self.extractor.signals_for_id(can_id)
    }

    /// Clear all buffered data.
    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            entry.timestamps.clear();
            for values in &mut entry.signal_values {
                values.clear();
            }
        }
    }

    /// Get the signal extractor.
    pub fn extractor(&self) -> &SignalExtractor {
        &self.extractor
    }
}

// =============================================================================
// High-level DBC + MDF Logger
// =============================================================================

/// High-level CAN logger that combines DBC signal definitions with MDF writing.
///
/// This provides a simple API for logging CAN bus data to MDF files using
/// signal definitions from a DBC file.
#[cfg(feature = "dbc")]
pub struct DbcMdfLogger<W: crate::writer::MdfWrite> {
    buffer: CanDataBuffer,
    writer: crate::MdfWriter<W>,
    channel_groups: alloc::collections::BTreeMap<u32, String>,
    initialized: bool,
}

#[cfg(feature = "dbc")]
impl DbcMdfLogger<crate::writer::VecWriter> {
    /// Create a new DBC MDF logger with in-memory output.
    ///
    /// Uses signal definitions from the provided DBC file.
    pub fn new(dbc: &dbc_rs::Dbc) -> crate::Result<Self> {
        let buffer = CanDataBuffer::from_dbc(dbc);
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());

        Ok(Self {
            buffer,
            writer,
            channel_groups: alloc::collections::BTreeMap::new(),
            initialized: false,
        })
    }

    /// Create a new DBC MDF logger with pre-allocated capacity.
    pub fn with_capacity(dbc: &dbc_rs::Dbc, capacity: usize) -> crate::Result<Self> {
        let buffer = CanDataBuffer::from_dbc(dbc);
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));

        Ok(Self {
            buffer,
            writer,
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

#[cfg(all(feature = "dbc", feature = "std"))]
impl DbcMdfLogger<crate::writer::FileWriter> {
    /// Create a new DBC MDF logger that writes to a file.
    pub fn new_file(dbc: &dbc_rs::Dbc, path: &str) -> crate::Result<Self> {
        let buffer = CanDataBuffer::from_dbc(dbc);
        let writer = crate::MdfWriter::new(path)?;

        Ok(Self {
            buffer,
            writer,
            channel_groups: alloc::collections::BTreeMap::new(),
            initialized: false,
        })
    }
}

#[cfg(feature = "dbc")]
impl<W: crate::writer::MdfWrite> DbcMdfLogger<W> {
    /// Log a CAN frame with timestamp.
    ///
    /// The frame data will be buffered and signals extracted according to the DBC.
    /// Call `flush()` periodically or `finalize()` at the end to write data to MDF.
    #[inline]
    pub fn log(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        self.buffer.push(can_id, timestamp_us, data)
    }

    /// Log an embedded-can frame with timestamp.
    #[cfg(feature = "embedded-can")]
    #[inline]
    pub fn log_frame<F: embedded_can::Frame>(&mut self, timestamp_us: u64, frame: &F) -> bool {
        let can_id = match frame.id() {
            embedded_can::Id::Standard(id) => id.as_raw() as u32,
            embedded_can::Id::Extended(id) => id.as_raw(),
        };
        self.buffer.push(can_id, timestamp_us, frame.data())
    }

    /// Flush buffered data to the MDF writer.
    ///
    /// This writes all accumulated CAN data to the MDF file and clears the buffer.
    pub fn flush(&mut self) -> crate::Result<()> {
        if !self.initialized {
            self.initialize_mdf()?;
        }

        // Write data for each CAN ID
        for can_id in self.buffer.can_ids().collect::<Vec<_>>() {
            self.write_can_id_data(can_id)?;
        }

        self.buffer.clear();
        Ok(())
    }

    /// Initialize the MDF file structure.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        use crate::DataType;

        self.writer.init_mdf_file()?;

        // Create a channel group for each CAN ID
        for can_id in self.buffer.can_ids().collect::<Vec<_>>() {
            let cg = self.writer.add_channel_group(None, |_| {})?;

            // Add timestamp channel
            let time_ch = self.writer.add_channel(&cg, None, |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::format!("Time_0x{:X}", can_id));
                ch.bit_count = 64;
            })?;
            self.writer.set_time_channel(&time_ch)?;

            // Add signal channels
            let signals: Vec<_> = self.buffer.signals_for_id(can_id).cloned().collect();
            let mut prev_ch = time_ch.clone();

            for signal in &signals {
                let ch = self.writer.add_channel(&cg, Some(&prev_ch), |ch| {
                    ch.data_type = DataType::FloatLE;
                    ch.name = Some(signal.name.clone());
                    ch.bit_count = 64;
                })?;
                prev_ch = ch;
            }

            self.channel_groups.insert(can_id, cg);
        }

        self.initialized = true;
        Ok(())
    }

    /// Write data for a specific CAN ID.
    fn write_can_id_data(&mut self, can_id: u32) -> crate::Result<()> {
        use crate::DecodedValue;

        let cg = match self.channel_groups.get(&can_id) {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        let timestamps = match self.buffer.timestamps(can_id) {
            Some(ts) if !ts.is_empty() => ts.to_vec(),
            _ => return Ok(()),
        };

        let signal_count = self.buffer.signals_for_id(can_id).count();

        self.writer.start_data_block_for_cg(&cg, 0)?;

        for (record_idx, &ts) in timestamps.iter().enumerate() {
            let mut values = alloc::vec![DecodedValue::UnsignedInteger(ts)];

            for sig_idx in 0..signal_count {
                if let Some(sig_values) = self.buffer.signal_values(can_id, sig_idx) {
                    if record_idx < sig_values.len() {
                        values.push(DecodedValue::Float(sig_values[record_idx]));
                    }
                }
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
        self.buffer.frame_count(can_id)
    }

    /// Get all CAN IDs being logged.
    pub fn can_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.buffer.can_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_le_simple() {
        let data = [0xAB, 0xCD, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(extract_signal_raw(&data, 0, 8, ByteOrder::LittleEndian), 0xAB);
        assert_eq!(extract_signal_raw(&data, 0, 16, ByteOrder::LittleEndian), 0xCDAB);
    }

    #[test]
    fn test_extract_le_offset() {
        let data = [0xAB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(extract_signal_raw(&data, 4, 4, ByteOrder::LittleEndian), 0x0A);
    }

    #[test]
    fn test_sign_extend() {
        assert_eq!(sign_extend(0xFF, 8), -1);
        assert_eq!(sign_extend(0x7F, 8), 127);
        assert_eq!(sign_extend(0xFFF, 12), -1);
    }

    #[test]
    fn test_apply_conversion() {
        let result = apply_conversion(100, false, 8, 0.5, 10.0);
        assert!((result - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_signal_definition_builder() {
        let sig = SignalDefinition::new(0x123, "TestSignal", 0, 16)
            .with_scale(0.1)
            .with_offset(-40.0)
            .signed()
            .big_endian()
            .with_unit("km/h");

        assert_eq!(sig.can_id, 0x123);
        assert_eq!(sig.name, "TestSignal");
        assert_eq!(sig.start_bit, 0);
        assert_eq!(sig.bit_length, 16);
        assert!((sig.scale - 0.1).abs() < 0.0001);
        assert!((sig.offset - (-40.0)).abs() < 0.0001);
        assert!(sig.is_signed);
        assert_eq!(sig.byte_order, ByteOrder::BigEndian);
        assert_eq!(sig.unit, Some(String::from("km/h")));
    }

    #[test]
    fn test_can_data_buffer() {
        let signals = alloc::vec![
            SignalDefinition::new(0x100, "Speed", 0, 16).with_scale(0.01),
            SignalDefinition::new(0x100, "RPM", 16, 16).with_scale(0.25),
            SignalDefinition::new(0x200, "Temp", 0, 8).with_offset(-40.0),
        ];

        let mut buffer = CanDataBuffer::new(signals);

        let data1 = [0x10, 0x27, 0xE8, 0x03, 0x00, 0x00, 0x00, 0x00];
        assert!(buffer.push(0x100, 1000, &data1));

        let data2 = [0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(buffer.push(0x200, 2000, &data2));

        assert!(!buffer.push(0x999, 3000, &data1));

        assert_eq!(buffer.frame_count(0x100), 1);
        assert_eq!(buffer.frame_count(0x200), 1);
        assert_eq!(buffer.frame_count(0x999), 0);
    }

    #[cfg(feature = "dbc")]
    mod dbc_tests {
        use super::*;

        #[test]
        fn test_signal_extractor_from_dbc() {
            let dbc = dbc_rs::Dbc::parse(r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" *
 SG_ Temp : 16|8@1- (1,-40) [-40|215] "C" *
"#).unwrap();

            let extractor = SignalExtractor::from_dbc(&dbc);

            assert!(extractor.has_can_id(256));
            assert!(!extractor.has_can_id(512));

            let signals: Vec<_> = extractor.signals_for_id(256).collect();
            assert_eq!(signals.len(), 2);
            assert_eq!(signals[0].name, "RPM");
            assert_eq!(signals[1].name, "Temp");
        }

        #[test]
        fn test_can_data_buffer_from_dbc() {
            let dbc = dbc_rs::Dbc::parse(r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" *
"#).unwrap();

            let mut buffer = CanDataBuffer::from_dbc(&dbc);

            // RPM = 2000 (raw: 8000 = 0x1F40, little-endian: 0x40, 0x1F)
            let data = [0x40, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            assert!(buffer.push(256, 1000, &data));

            assert_eq!(buffer.frame_count(256), 1);

            let values = buffer.signal_values(256, 0).unwrap();
            assert!((values[0] - 2000.0).abs() < 0.001);
        }

        #[test]
        fn test_dbc_mdf_logger() {
            let dbc = dbc_rs::Dbc::parse(r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" *
"#).unwrap();

            let mut logger = DbcMdfLogger::new(&dbc).unwrap();

            // Log some frames
            let data = [0x40, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            assert!(logger.log(256, 1000, &data));
            assert!(logger.log(256, 2000, &data));

            assert_eq!(logger.frame_count(256), 2);

            // Finalize and get MDF bytes
            let mdf_bytes = logger.finalize().unwrap();
            assert!(!mdf_bytes.is_empty());

            // Verify MDF header
            assert_eq!(&mdf_bytes[0..3], b"MDF");
        }
    }
}
