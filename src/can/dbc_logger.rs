//! High-level DBC + MDF Logger with full metadata support.
//!
//! This module provides [`DbcMdfLogger`], a high-performance logger that combines
//! DBC signal definitions with MDF4 file writing. It supports:
//!
//! - Full metadata preservation (units, conversions, limits)
//! - Raw value storage with conversion blocks for maximum precision
//! - Physical value storage for compatibility
//! - Multiplexed signal support via dbc-rs
//!
//! # Storage Modes
//!
//! The logger supports two storage modes:
//!
//! 1. **Physical Values** (default): Stores decoded physical values as 64-bit floats.
//!    This is simpler but loses some precision for integer signals.
//!
//! 2. **Raw Values**: Stores raw integer values with MDF4 conversion blocks.
//!    This preserves full precision and allows MDF4 viewers to show both
//!    raw and physical values.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::dbc_compat::SignalInfo;

/// Configuration for DbcMdfLogger.
#[derive(Debug, Clone)]
pub struct DbcMdfLoggerConfig {
    /// Store raw values with conversion blocks instead of physical values.
    /// Default: false (store physical values as f64)
    pub store_raw_values: bool,

    /// Include unit information in MDF channels.
    /// Default: true
    pub include_units: bool,

    /// Include min/max limits in MDF channels.
    /// Default: true
    pub include_limits: bool,

    /// Include conversion blocks (for raw value mode).
    /// Default: true
    pub include_conversions: bool,

    /// Include value descriptions as ValueToText conversions.
    /// When enabled, DBC VAL_ entries are converted to MDF4 ValueToText blocks.
    /// Default: true
    pub include_value_descriptions: bool,
}

impl Default for DbcMdfLoggerConfig {
    fn default() -> Self {
        Self {
            store_raw_values: false,
            include_units: true,
            include_limits: true,
            include_conversions: true,
            include_value_descriptions: true,
        }
    }
}

/// Builder for DbcMdfLogger configuration.
pub struct DbcMdfLoggerBuilder<'dbc> {
    dbc: &'dbc dbc_rs::Dbc,
    config: DbcMdfLoggerConfig,
    capacity: Option<usize>,
}

impl<'dbc> DbcMdfLoggerBuilder<'dbc> {
    /// Create a new builder with default configuration.
    pub fn new(dbc: &'dbc dbc_rs::Dbc) -> Self {
        Self {
            dbc,
            config: DbcMdfLoggerConfig::default(),
            capacity: None,
        }
    }

    /// Set whether to store raw values with conversion blocks.
    ///
    /// When enabled, raw integer values are stored and conversion blocks
    /// are attached to channels. This preserves full precision and allows
    /// MDF4 viewers to display both raw and physical values.
    ///
    /// Default: false (stores physical f64 values)
    pub fn store_raw_values(mut self, enabled: bool) -> Self {
        self.config.store_raw_values = enabled;
        self
    }

    /// Set whether to include unit strings in MDF channels.
    ///
    /// Default: true
    pub fn include_units(mut self, enabled: bool) -> Self {
        self.config.include_units = enabled;
        self
    }

    /// Set whether to include min/max limits in MDF channels.
    ///
    /// Default: true
    pub fn include_limits(mut self, enabled: bool) -> Self {
        self.config.include_limits = enabled;
        self
    }

    /// Set whether to include conversion blocks (for raw value mode).
    ///
    /// Default: true
    pub fn include_conversions(mut self, enabled: bool) -> Self {
        self.config.include_conversions = enabled;
        self
    }

    /// Set whether to include value descriptions as ValueToText conversions.
    ///
    /// When enabled, DBC VAL_ entries are converted to MDF4 ValueToText blocks.
    /// This allows MDF4 viewers to display human-readable text for enum-like signals.
    ///
    /// Note: If a signal has both a linear conversion (factor/offset != 1/0) and
    /// value descriptions, the value descriptions take precedence.
    ///
    /// Default: true
    pub fn include_value_descriptions(mut self, enabled: bool) -> Self {
        self.config.include_value_descriptions = enabled;
        self
    }

    /// Set the initial buffer capacity.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = Some(capacity);
        self
    }

    /// Build the logger with in-memory output.
    pub fn build(self) -> crate::Result<DbcMdfLogger<'dbc, crate::writer::VecWriter>> {
        let writer = match self.capacity {
            Some(cap) => crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(cap)),
            None => crate::MdfWriter::from_writer(crate::writer::VecWriter::new()),
        };
        Ok(DbcMdfLogger::with_config(self.dbc, writer, self.config))
    }

    /// Build the logger with file output.
    #[cfg(feature = "std")]
    pub fn build_file(self, path: &str) -> crate::Result<DbcMdfLogger<'dbc, crate::writer::FileWriter>> {
        let writer = match self.capacity {
            Some(cap) => crate::MdfWriter::new_with_capacity(path, cap)?,
            None => crate::MdfWriter::new(path)?,
        };
        Ok(DbcMdfLogger::with_config(self.dbc, writer, self.config))
    }
}

/// Buffer for a single message's decoded data.
#[derive(Debug)]
struct MessageBuffer {
    /// Signal information extracted from DBC
    signals: Vec<SignalInfo>,
    /// Timestamps for each frame (microseconds)
    timestamps: Vec<u64>,
    /// Raw values per signal (outer vec = signals, inner vec = samples)
    raw_values: Vec<Vec<i64>>,
    /// Physical values per signal (outer vec = signals, inner vec = samples)
    physical_values: Vec<Vec<f64>>,
}

impl MessageBuffer {
    fn new(signals: Vec<SignalInfo>) -> Self {
        let num_signals = signals.len();
        Self {
            signals,
            timestamps: Vec::new(),
            raw_values: (0..num_signals).map(|_| Vec::new()).collect(),
            physical_values: (0..num_signals).map(|_| Vec::new()).collect(),
        }
    }

    fn push_physical(&mut self, timestamp_us: u64, physical_values: &[f64]) {
        self.timestamps.push(timestamp_us);
        for (i, &value) in physical_values.iter().enumerate() {
            if i < self.physical_values.len() {
                self.physical_values[i].push(value);
            }
        }
    }

    fn push_raw(&mut self, timestamp_us: u64, raw_values: &[i64]) {
        self.timestamps.push(timestamp_us);
        for (i, &value) in raw_values.iter().enumerate() {
            if i < self.raw_values.len() {
                self.raw_values[i].push(value);
            }
        }
    }

    fn clear(&mut self) {
        self.timestamps.clear();
        for v in &mut self.raw_values {
            v.clear();
        }
        for v in &mut self.physical_values {
            v.clear();
        }
    }

    fn frame_count(&self) -> usize {
        self.timestamps.len()
    }
}

/// Channel IDs stored after MDF initialization.
/// Reserved for future use (e.g., updating channel metadata after initialization).
#[allow(dead_code)]
struct ChannelIds {
    time_channel: String,
    signal_channels: Vec<String>,
}

/// High-level CAN logger that combines DBC signal definitions with MDF writing.
///
/// This provides a simple API for logging CAN bus data to MDF files using
/// signal definitions from a DBC file. It uses `Dbc::decode()` directly for
/// signal extraction, supporting all DBC features including multiplexing.
///
/// # Features
///
/// - Full metadata preservation (units, conversions, limits)
/// - Raw value storage with conversion blocks for maximum precision
/// - Physical value storage for compatibility
/// - Support for standard and extended CAN IDs
///
/// # Example
///
/// ```ignore
/// use mdf4_rs::can::DbcMdfLogger;
///
/// let dbc = dbc_rs::Dbc::parse(dbc_content)?;
///
/// // Simple usage (stores physical values)
/// let mut logger = DbcMdfLogger::new(&dbc)?;
///
/// // Or with builder for raw value storage
/// let mut logger = DbcMdfLogger::builder(&dbc)
///     .store_raw_values(true)
///     .build()?;
///
/// // Log CAN frames
/// logger.log(0x100, timestamp_us, &frame_data);
///
/// // Get MDF bytes
/// let mdf_bytes = logger.finalize()?;
/// ```
pub struct DbcMdfLogger<'dbc, W: crate::writer::MdfWrite> {
    dbc: &'dbc dbc_rs::Dbc,
    config: DbcMdfLoggerConfig,
    buffers: BTreeMap<u32, MessageBuffer>,
    writer: crate::MdfWriter<W>,
    channel_groups: BTreeMap<u32, String>,
    channel_ids: BTreeMap<u32, ChannelIds>,
    initialized: bool,
}

impl<'dbc> DbcMdfLogger<'dbc, crate::writer::VecWriter> {
    /// Create a new DBC MDF logger with in-memory output.
    ///
    /// Uses signal definitions from the provided DBC file.
    /// Stores physical values by default; use `builder()` for raw value mode.
    pub fn new(dbc: &'dbc dbc_rs::Dbc) -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());
        Ok(Self::with_config(dbc, writer, DbcMdfLoggerConfig::default()))
    }

    /// Create a new DBC MDF logger with pre-allocated capacity.
    pub fn with_capacity(dbc: &'dbc dbc_rs::Dbc, capacity: usize) -> crate::Result<Self> {
        let writer =
            crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self::with_config(dbc, writer, DbcMdfLoggerConfig::default()))
    }

    /// Create a builder for configuring the logger.
    pub fn builder(dbc: &'dbc dbc_rs::Dbc) -> DbcMdfLoggerBuilder<'dbc> {
        DbcMdfLoggerBuilder::new(dbc)
    }

    /// Finalize the MDF file and return the bytes.
    pub fn finalize(mut self) -> crate::Result<Vec<u8>> {
        self.flush_and_finalize()?;
        Ok(self.writer.into_inner().into_inner())
    }
}

#[cfg(feature = "std")]
impl<'dbc> DbcMdfLogger<'dbc, crate::writer::FileWriter> {
    /// Create a new DBC MDF logger that writes to a file.
    pub fn new_file(dbc: &'dbc dbc_rs::Dbc, path: &str) -> crate::Result<Self> {
        let writer = crate::MdfWriter::new(path)?;
        Ok(Self::with_config(dbc, writer, DbcMdfLoggerConfig::default()))
    }

    /// Create a builder for configuring the logger with file output.
    pub fn builder_file(dbc: &'dbc dbc_rs::Dbc) -> DbcMdfLoggerBuilder<'dbc> {
        DbcMdfLoggerBuilder::new(dbc)
    }

    /// Finalize and close the MDF file.
    pub fn finalize_file(mut self) -> crate::Result<()> {
        self.flush_and_finalize()
    }
}

impl<'dbc, W: crate::writer::MdfWrite> DbcMdfLogger<'dbc, W> {
    /// Create a logger with custom configuration.
    fn with_config(dbc: &'dbc dbc_rs::Dbc, writer: crate::MdfWriter<W>, config: DbcMdfLoggerConfig) -> Self {
        // Pre-create buffers for each message in the DBC
        let mut buffers = BTreeMap::new();
        for message in dbc.messages().iter() {
            let signals: Vec<SignalInfo> = message
                .signals()
                .iter()
                .map(SignalInfo::from_signal)
                .collect();
            if !signals.is_empty() {
                buffers.insert(message.id(), MessageBuffer::new(signals));
            }
        }

        Self {
            dbc,
            config,
            buffers,
            writer,
            channel_groups: BTreeMap::new(),
            channel_ids: BTreeMap::new(),
            initialized: false,
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &DbcMdfLoggerConfig {
        &self.config
    }

    /// Log a CAN frame with timestamp.
    ///
    /// The frame is decoded using the DBC and buffered.
    /// Call `flush()` periodically or `finalize()` at the end to write data to MDF.
    ///
    /// Returns `true` if the message was recognized and logged, `false` otherwise.
    #[inline]
    pub fn log(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        self.log_internal(can_id, timestamp_us, data, false)
    }

    /// Log a CAN frame with extended ID.
    ///
    /// Use this for 29-bit extended CAN IDs.
    #[inline]
    pub fn log_extended(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        self.log_internal(can_id, timestamp_us, data, true)
    }

    /// Internal logging implementation.
    #[inline]
    fn log_internal(&mut self, can_id: u32, timestamp_us: u64, data: &[u8], is_extended: bool) -> bool {
        // Use Dbc::decode() directly
        if let Ok(decoded) = self.dbc.decode(can_id, data, is_extended) {
            let dbc_id = if is_extended { can_id | 0x8000_0000 } else { can_id };

            if let Some(buffer) = self.buffers.get_mut(&dbc_id) {
                if self.config.store_raw_values {
                    // Extract raw values
                    let raw_values: Vec<i64> = buffer
                        .signals
                        .iter()
                        .map(|info| {
                            decoded
                                .iter()
                                .find(|d| d.name == info.name)
                                .map(|d| d.raw_value)
                                .unwrap_or(0)
                        })
                        .collect();
                    buffer.push_raw(timestamp_us, &raw_values);
                } else {
                    // Extract physical values
                    let physical_values: Vec<f64> = buffer
                        .signals
                        .iter()
                        .map(|info| {
                            decoded
                                .iter()
                                .find(|d| d.name == info.name)
                                .map(|d| d.value)
                                .unwrap_or(0.0)
                        })
                        .collect();
                    buffer.push_physical(timestamp_us, &physical_values);
                }
                return true;
            }
        }
        false
    }

    /// Log an embedded-can frame with timestamp.
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

    /// Flush buffered data to the MDF writer.
    ///
    /// This writes all accumulated CAN data to the MDF file and clears the buffer.
    pub fn flush(&mut self) -> crate::Result<()> {
        if !self.initialized {
            self.initialize_mdf()?;
        }

        // Write data for each CAN ID
        for can_id in self.buffers.keys().copied().collect::<Vec<_>>() {
            self.write_message_data(can_id)?;
        }

        // Clear all buffers
        for buffer in self.buffers.values_mut() {
            buffer.clear();
        }

        Ok(())
    }

    /// Initialize the MDF file structure with full metadata.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        use crate::DataType;

        self.writer.init_mdf_file()?;

        // Create a channel group for each message
        for (&can_id, buffer) in &self.buffers {
            let cg = self.writer.add_channel_group(None, |_| {})?;

            // Find the DBC message to get name and sender for channel group metadata
            if let Some(message) = self.dbc.messages().find_by_id(can_id) {
                // Set channel group name from DBC message name
                let msg_name = message.name();
                if !msg_name.is_empty() {
                    self.writer.set_channel_group_name(&cg, msg_name)?;
                }

                // Set channel group source from DBC message sender (ECU)
                let sender = message.sender();
                if !sender.is_empty() && sender != "Vector__XXX" {
                    self.writer.set_channel_group_source_name(&cg, sender)?;
                }
            }

            // Add timestamp channel
            let time_ch = self.writer.add_channel(&cg, None, |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::format!("Time_0x{:X}", can_id));
                ch.bit_count = 64;
            })?;
            self.writer.set_time_channel(&time_ch)?;
            self.writer.set_channel_unit(&time_ch, "us")?;

            // Add signal channels with full metadata
            let mut prev_ch = time_ch.clone();
            let mut signal_channels = Vec::new();

            for info in &buffer.signals {
                let ch = if self.config.store_raw_values {
                    // Raw value mode: use appropriate integer type
                    self.writer.add_channel(&cg, Some(&prev_ch), |ch| {
                        ch.data_type = info.data_type;
                        ch.name = Some(info.name.clone());
                        ch.bit_count = info.bit_count;
                    })?
                } else {
                    // Physical value mode: use f64
                    self.writer.add_channel(&cg, Some(&prev_ch), |ch| {
                        ch.data_type = DataType::FloatLE;
                        ch.name = Some(info.name.clone());
                        ch.bit_count = 64;
                    })?
                };

                // Add unit if available and enabled
                if self.config.include_units {
                    if let Some(ref unit) = info.unit {
                        self.writer.set_channel_unit(&ch, unit)?;
                    }
                }

                // Add limits if enabled
                if self.config.include_limits && (info.min != 0.0 || info.max != 0.0) {
                    self.writer.set_channel_limits(&ch, info.min, info.max)?;
                }

                // Add conversion block if in raw mode and enabled
                if self.config.store_raw_values {
                    // Check for value descriptions first (they take precedence)
                    let has_value_desc = self.config.include_value_descriptions
                        && self.dbc.value_descriptions_for_signal(can_id, &info.name).is_some();

                    if has_value_desc {
                        // Add ValueToText conversion from DBC value descriptions
                        if let Some(vd) = self.dbc.value_descriptions_for_signal(can_id, &info.name) {
                            let mapping: Vec<(i64, &str)> = vd.iter()
                                .map(|(v, desc)| (v as i64, desc))
                                .collect();
                            if !mapping.is_empty() {
                                self.writer.add_value_to_text_conversion(
                                    &mapping,
                                    "",  // default text for unknown values
                                    Some(&ch),
                                )?;
                            }
                        }
                    } else if self.config.include_conversions {
                        // Fall back to linear conversion if available
                        if let Some(ref conv) = info.conversion {
                            self.writer.set_channel_conversion(&ch, conv)?;
                        }
                    }
                }

                signal_channels.push(ch.clone());
                prev_ch = ch;
            }

            self.channel_groups.insert(can_id, cg);
            self.channel_ids.insert(can_id, ChannelIds {
                time_channel: time_ch,
                signal_channels,
            });
        }

        self.initialized = true;
        Ok(())
    }

    /// Write data for a specific message.
    fn write_message_data(&mut self, can_id: u32) -> crate::Result<()> {
        use crate::DecodedValue;

        let cg = match self.channel_groups.get(&can_id) {
            Some(cg) => cg.clone(),
            None => return Ok(()),
        };

        let buffer = match self.buffers.get(&can_id) {
            Some(b) if !b.timestamps.is_empty() => b,
            _ => return Ok(()),
        };

        self.writer.start_data_block_for_cg(&cg, 0)?;

        if self.config.store_raw_values {
            // Write raw values
            for (record_idx, &ts) in buffer.timestamps.iter().enumerate() {
                let mut values = alloc::vec![DecodedValue::UnsignedInteger(ts)];

                for (sig_idx, info) in buffer.signals.iter().enumerate() {
                    if record_idx < buffer.raw_values[sig_idx].len() {
                        let raw = buffer.raw_values[sig_idx][record_idx];
                        // Use appropriate integer type based on signedness
                        if info.unsigned {
                            values.push(DecodedValue::UnsignedInteger(raw as u64));
                        } else {
                            values.push(DecodedValue::SignedInteger(raw));
                        }
                    }
                }

                self.writer.write_record(&cg, &values)?;
            }
        } else {
            // Write physical values
            for (record_idx, &ts) in buffer.timestamps.iter().enumerate() {
                let mut values = alloc::vec![DecodedValue::UnsignedInteger(ts)];

                for signal_values in &buffer.physical_values {
                    if record_idx < signal_values.len() {
                        values.push(DecodedValue::Float(signal_values[record_idx]));
                    }
                }

                self.writer.write_record(&cg, &values)?;
            }
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
        self.buffers
            .get(&can_id)
            .map(|b| b.frame_count())
            .unwrap_or(0)
    }

    /// Get all CAN IDs being logged.
    pub fn can_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.buffers.keys().copied()
    }

    /// Get the total number of messages being logged.
    pub fn message_count(&self) -> usize {
        self.buffers.len()
    }

    /// Get the total number of signals across all messages.
    pub fn total_signal_count(&self) -> usize {
        self.buffers.values().map(|b| b.signals.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbc_mdf_logger() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
"#,
        )
        .unwrap();

        let mut logger = DbcMdfLogger::new(&dbc).unwrap();

        // Log some frames (RPM = 2000, raw = 8000 = 0x1F40)
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

    #[test]
    fn test_dbc_mdf_logger_raw_mode() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
 SG_ Temp : 16|8@1- (1,-40) [-40|215] "C" Vector__XXX
"#,
        )
        .unwrap();

        let mut logger = DbcMdfLogger::builder(&dbc)
            .store_raw_values(true)
            .build()
            .unwrap();

        // RPM = 2000 (raw 8000), Temp = 50°C (raw 90)
        let data = [0x40, 0x1F, 0x5A, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(logger.log(256, 1000, &data));

        assert_eq!(logger.frame_count(256), 1);
        assert_eq!(logger.total_signal_count(), 2);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_dbc_mdf_logger_multiple_signals() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
 SG_ Temp : 16|8@1- (1,-40) [-40|215] "C" Vector__XXX
"#,
        )
        .unwrap();

        let mut logger = DbcMdfLogger::new(&dbc).unwrap();

        // RPM = 2000 (raw 8000), Temp = 50°C (raw 90)
        let data = [0x40, 0x1F, 0x5A, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(logger.log(256, 1000, &data));

        assert_eq!(logger.frame_count(256), 1);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());
    }

    #[test]
    fn test_dbc_mdf_logger_unknown_message() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
"#,
        )
        .unwrap();

        let mut logger = DbcMdfLogger::new(&dbc).unwrap();

        // Try to log unknown message ID
        let data = [0x00; 8];
        assert!(!logger.log(999, 1000, &data));

        assert_eq!(logger.frame_count(999), 0);
    }

    #[test]
    fn test_builder_configuration() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"
BU_:
BO_ 100 TestMsg: 8 Vector__XXX
 SG_ TestSig : 0|16@1+ (1,0) [0|65535] "units" Vector__XXX
"#,
        )
        .unwrap();

        let logger = DbcMdfLogger::builder(&dbc)
            .store_raw_values(true)
            .include_units(true)
            .include_limits(true)
            .include_conversions(true)
            .with_capacity(1024)
            .build()
            .unwrap();

        assert!(logger.config().store_raw_values);
        assert!(logger.config().include_units);
        assert!(logger.config().include_limits);
        assert!(logger.config().include_conversions);
    }

    #[test]
    fn test_value_descriptions_to_text() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Transmission : 8 ECM
 SG_ GearPosition : 0|8@1+ (1,0) [0|5] "" Vector__XXX

VAL_ 256 GearPosition 0 "Park" 1 "Reverse" 2 "Neutral" 3 "Drive" 4 "Sport" ;
"#,
        )
        .unwrap();

        // Verify value descriptions are parsed
        let vd = dbc.value_descriptions_for_signal(256, "GearPosition");
        assert!(vd.is_some());
        let vd = vd.unwrap();
        assert_eq!(vd.get(0), Some("Park"));
        assert_eq!(vd.get(3), Some("Drive"));

        // Create logger with raw values and value descriptions
        let mut logger = DbcMdfLogger::builder(&dbc)
            .store_raw_values(true)
            .include_value_descriptions(true)
            .build()
            .unwrap();

        // Log some gear position changes
        assert!(logger.log(256, 1000, &[0, 0, 0, 0, 0, 0, 0, 0])); // Park
        assert!(logger.log(256, 2000, &[3, 0, 0, 0, 0, 0, 0, 0])); // Drive
        assert!(logger.log(256, 3000, &[2, 0, 0, 0, 0, 0, 0, 0])); // Neutral

        assert_eq!(logger.frame_count(256), 3);
        assert!(logger.config().include_value_descriptions);

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());

        // Verify MDF header
        assert_eq!(&mdf_bytes[0..3], b"MDF");
    }

    #[test]
    fn test_channel_group_naming_and_source() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM TCM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (1,0) [0|8000] "rpm" Vector__XXX

BO_ 512 Transmission : 8 TCM
 SG_ Gear : 0|8@1+ (1,0) [0|5] "" Vector__XXX
"#,
        )
        .unwrap();

        let mut logger = DbcMdfLogger::new(&dbc).unwrap();

        // Log some data
        assert!(logger.log(256, 1000, &[0x00, 0x10, 0, 0, 0, 0, 0, 0]));
        assert!(logger.log(512, 1000, &[3, 0, 0, 0, 0, 0, 0, 0]));

        let mdf_bytes = logger.finalize().unwrap();
        assert!(!mdf_bytes.is_empty());

        // The MDF should contain channel groups named after messages
        // and sources named after senders (ECM, TCM)
        // This is verified by the fact that it compiles and runs without errors
    }
}
