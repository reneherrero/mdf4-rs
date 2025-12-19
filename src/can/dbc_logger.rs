//! High-level DBC + MDF Logger.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Buffer for a single message's decoded data.
#[derive(Debug)]
struct MessageBuffer {
    /// Signal names in order (copied from DBC for MDF channel creation)
    signal_names: Vec<String>,
    /// Timestamps for each frame
    timestamps: Vec<u64>,
    /// Decoded values per signal (outer vec = signals, inner vec = samples)
    values: Vec<Vec<f64>>,
}

impl MessageBuffer {
    fn new(signal_names: Vec<String>) -> Self {
        let num_signals = signal_names.len();
        Self {
            signal_names,
            timestamps: Vec::new(),
            values: (0..num_signals).map(|_| Vec::new()).collect(),
        }
    }

    fn push(&mut self, timestamp_us: u64, decoded_values: &[f64]) {
        self.timestamps.push(timestamp_us);
        for (i, &value) in decoded_values.iter().enumerate() {
            if i < self.values.len() {
                self.values[i].push(value);
            }
        }
    }

    fn clear(&mut self) {
        self.timestamps.clear();
        for v in &mut self.values {
            v.clear();
        }
    }

    fn frame_count(&self) -> usize {
        self.timestamps.len()
    }
}

/// High-level CAN logger that combines DBC signal definitions with MDF writing.
///
/// This provides a simple API for logging CAN bus data to MDF files using
/// signal definitions from a DBC file. It uses `Dbc::decode()` directly for
/// signal extraction, supporting all DBC features including multiplexing.
///
/// # Example
///
/// ```ignore
/// use mdf4_rs::can::DbcMdfLogger;
///
/// let dbc = dbc_rs::Dbc::parse(dbc_content)?;
/// let mut logger = DbcMdfLogger::new(&dbc)?;
///
/// // Log CAN frames
/// logger.log(0x100, timestamp_us, &frame_data);
///
/// // Get MDF bytes
/// let mdf_bytes = logger.finalize()?;
/// ```
pub struct DbcMdfLogger<'dbc, W: crate::writer::MdfWrite> {
    dbc: &'dbc dbc_rs::Dbc,
    buffers: BTreeMap<u32, MessageBuffer>,
    writer: crate::MdfWriter<W>,
    channel_groups: BTreeMap<u32, String>,
    initialized: bool,
}

impl<'dbc> DbcMdfLogger<'dbc, crate::writer::VecWriter> {
    /// Create a new DBC MDF logger with in-memory output.
    ///
    /// Uses signal definitions from the provided DBC file.
    pub fn new(dbc: &'dbc dbc_rs::Dbc) -> crate::Result<Self> {
        let writer = crate::MdfWriter::from_writer(crate::writer::VecWriter::new());
        Ok(Self::with_writer(dbc, writer))
    }

    /// Create a new DBC MDF logger with pre-allocated capacity.
    pub fn with_capacity(dbc: &'dbc dbc_rs::Dbc, capacity: usize) -> crate::Result<Self> {
        let writer =
            crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(capacity));
        Ok(Self::with_writer(dbc, writer))
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
        Ok(Self::with_writer(dbc, writer))
    }
}

impl<'dbc, W: crate::writer::MdfWrite> DbcMdfLogger<'dbc, W> {
    /// Create a logger with a custom writer.
    fn with_writer(dbc: &'dbc dbc_rs::Dbc, writer: crate::MdfWriter<W>) -> Self {
        // Pre-create buffers for each message in the DBC
        let mut buffers = BTreeMap::new();
        for message in dbc.messages().iter() {
            let signal_names: Vec<String> = message
                .signals()
                .iter()
                .map(|s| String::from(s.name()))
                .collect();
            if !signal_names.is_empty() {
                buffers.insert(message.id(), MessageBuffer::new(signal_names));
            }
        }

        Self {
            dbc,
            buffers,
            writer,
            channel_groups: BTreeMap::new(),
            initialized: false,
        }
    }

    /// Log a CAN frame with timestamp.
    ///
    /// The frame is decoded using the DBC and buffered.
    /// Call `flush()` periodically or `finalize()` at the end to write data to MDF.
    ///
    /// Returns `true` if the message was recognized and logged, `false` otherwise.
    #[inline]
    pub fn log(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        // Use Dbc::decode() directly
        if let Ok(decoded) = self.dbc.decode(can_id, data, false) {
            if let Some(buffer) = self.buffers.get_mut(&can_id) {
                // Extract values in signal order (matching buffer.signal_names)
                let values: Vec<f64> = buffer
                    .signal_names
                    .iter()
                    .map(|name| {
                        decoded
                            .iter()
                            .find(|d| d.name == name)
                            .map(|d| d.value)
                            .unwrap_or(0.0)
                    })
                    .collect();

                buffer.push(timestamp_us, &values);
                return true;
            }
        }
        false
    }

    /// Log a CAN frame with extended ID.
    ///
    /// Use this for 29-bit extended CAN IDs.
    #[inline]
    pub fn log_extended(&mut self, can_id: u32, timestamp_us: u64, data: &[u8]) -> bool {
        if let Ok(decoded) = self.dbc.decode(can_id, data, true) {
            // Extended IDs have bit 31 set in the DBC (0x8000_0000)
            let dbc_id = can_id | 0x8000_0000;
            if let Some(buffer) = self.buffers.get_mut(&dbc_id) {
                let values: Vec<f64> = buffer
                    .signal_names
                    .iter()
                    .map(|name| {
                        decoded
                            .iter()
                            .find(|d| d.name == name)
                            .map(|d| d.value)
                            .unwrap_or(0.0)
                    })
                    .collect();

                buffer.push(timestamp_us, &values);
                return true;
            }
        }
        false
    }

    /// Log an embedded-can frame with timestamp.
    #[cfg(feature = "embedded-can")]
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

    /// Initialize the MDF file structure.
    fn initialize_mdf(&mut self) -> crate::Result<()> {
        use crate::DataType;

        self.writer.init_mdf_file()?;

        // Create a channel group for each message
        for (&can_id, buffer) in &self.buffers {
            let cg = self.writer.add_channel_group(None, |_| {})?;

            // Add timestamp channel
            let time_ch = self.writer.add_channel(&cg, None, |ch| {
                ch.data_type = DataType::UnsignedIntegerLE;
                ch.name = Some(alloc::format!("Time_0x{:X}", can_id));
                ch.bit_count = 64;
            })?;
            self.writer.set_time_channel(&time_ch)?;

            // Add signal channels
            let mut prev_ch = time_ch;
            for signal_name in &buffer.signal_names {
                let ch = self.writer.add_channel(&cg, Some(&prev_ch), |ch| {
                    ch.data_type = DataType::FloatLE;
                    ch.name = Some(signal_name.clone());
                    ch.bit_count = 64;
                })?;
                prev_ch = ch;
            }

            self.channel_groups.insert(can_id, cg);
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

        for (record_idx, &ts) in buffer.timestamps.iter().enumerate() {
            let mut values = alloc::vec![DecodedValue::UnsignedInteger(ts)];

            for signal_values in &buffer.values {
                if record_idx < signal_values.len() {
                    values.push(DecodedValue::Float(signal_values[record_idx]));
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
        self.buffers
            .get(&can_id)
            .map(|b| b.frame_count())
            .unwrap_or(0)
    }

    /// Get all CAN IDs being logged.
    pub fn can_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.buffers.keys().copied()
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
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" *
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
    fn test_dbc_mdf_logger_multiple_signals() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"

BU_: ECM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" *
 SG_ Temp : 16|8@1- (1,-40) [-40|215] "C" *
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
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" *
"#,
        )
        .unwrap();

        let mut logger = DbcMdfLogger::new(&dbc).unwrap();

        // Try to log unknown message ID
        let data = [0x00; 8];
        assert!(!logger.log(999, 1000, &data));

        assert_eq!(logger.frame_count(999), 0);
    }
}
