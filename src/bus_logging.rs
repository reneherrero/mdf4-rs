//! Shared utilities for ASAM MDF4 Bus Logging.
//!
//! This module provides common traits and helper functions used by
//! CAN, Ethernet, LIN, and FlexRay loggers.

use alloc::string::String;
use alloc::vec::Vec;

use crate::blocks::SourceBlock;
use crate::writer::MdfWrite;
use crate::{DataType, DecodedValue, MdfWriter, Result};

/// Conversion factor from microseconds to seconds.
pub const MICROS_TO_SECONDS: f64 = 1.0 / 1_000_000.0;

/// Convert timestamp from microseconds to seconds (ASAM standard).
#[inline]
pub fn timestamp_to_seconds(timestamp_us: u64) -> f64 {
    timestamp_us as f64 * MICROS_TO_SECONDS
}

/// A buffered frame entry with timestamp.
pub struct TimestampedFrame<F> {
    /// Timestamp in seconds (ASAM uses float64 seconds).
    pub timestamp_s: f64,
    /// The frame data.
    pub frame: F,
}

impl<F> TimestampedFrame<F> {
    /// Create a new timestamped frame from microseconds.
    #[inline]
    pub fn new(timestamp_us: u64, frame: F) -> Self {
        Self {
            timestamp_s: timestamp_to_seconds(timestamp_us),
            frame,
        }
    }
}

/// Trait for frame types that can be serialized to bytes.
pub trait BusFrame: Clone {
    /// Serialize the frame to bytes for MDF storage.
    fn to_mdf_bytes(&self) -> Vec<u8>;

    /// Get the frame size in bytes.
    fn mdf_size(&self) -> usize {
        self.to_mdf_bytes().len()
    }
}

/// Common configuration for bus loggers.
pub struct BusLoggerConfig {
    /// The source/bus/interface name.
    pub source_name: String,
    /// Channel group name pattern (e.g., "{source}_CAN_DataFrame").
    pub group_name: String,
    /// Data channel name (e.g., "CAN_DataFrame", "LIN_Frame").
    pub data_channel_name: String,
    /// Size of the data channel in bits.
    pub data_channel_bits: u32,
    /// Source block for the bus type.
    pub source_block: SourceBlock,
}

/// Initialize a bus logging channel group with timestamp and data channels.
///
/// This is the common pattern used by all bus loggers:
/// 1. Create channel group with name
/// 2. Set source metadata
/// 3. Add Float64 timestamp channel (seconds)
/// 4. Add ByteArray data channel
///
/// Returns the channel group ID and data channel ID.
pub fn init_bus_channel_group<W: MdfWrite>(
    writer: &mut MdfWriter<W>,
    config: &BusLoggerConfig,
) -> Result<(String, String)> {
    let cg = writer.add_channel_group(None, |_| {})?;
    writer.set_channel_group_name(&cg, &config.group_name)?;
    writer.set_channel_group_source(&cg, &config.source_block, Some(&config.source_name))?;

    // Add Timestamp channel (Float64 in seconds - ASAM standard)
    let time_ch = writer.add_channel(&cg, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some(String::from("Timestamp"));
        ch.bit_count = 64;
    })?;
    writer.set_time_channel(&time_ch)?;
    writer.set_channel_unit(&time_ch, "s")?;

    // Add data channel (ByteArray - ASAM composite format)
    let data_ch = writer.add_channel(&cg, Some(&time_ch), |ch| {
        ch.data_type = DataType::ByteArray;
        ch.name = Some(config.data_channel_name.clone());
        ch.bit_count = config.data_channel_bits;
    })?;

    Ok((cg, data_ch))
}

/// Write timestamped frames to an MDF channel group.
///
/// # Arguments
/// * `writer` - The MDF writer instance
/// * `channel_group` - Channel group ID returned by [`init_bus_channel_group`]
/// * `frames` - Iterator of timestamped frames to write
///
/// # Type Parameters
/// * `W` - Writer backend implementing [`MdfWrite`]
/// * `F` - Frame type implementing [`BusFrame`]
/// * `I` - Iterator yielding [`TimestampedFrame<F>`]
pub fn write_timestamped_frames<W, F, I>(
    writer: &mut MdfWriter<W>,
    channel_group: &str,
    frames: I,
) -> Result<()>
where
    W: MdfWrite,
    F: BusFrame,
    I: Iterator<Item = TimestampedFrame<F>>,
{
    // Collect frames to avoid borrowing issues
    let frames: Vec<_> = frames.collect();

    if frames.is_empty() {
        return Ok(());
    }

    writer.start_data_block_for_cg(channel_group, 0)?;

    for entry in &frames {
        let values = [
            DecodedValue::Float(entry.timestamp_s),
            DecodedValue::ByteArray(entry.frame.to_mdf_bytes()),
        ];
        writer.write_record(channel_group, &values)?;
    }

    writer.finish_data_block(channel_group)?;
    Ok(())
}

/// Statistics for a bus logger.
#[derive(Debug, Clone, Default)]
pub struct BusLoggerStats {
    /// Total number of frames logged.
    pub total_frames: usize,
    /// Number of transmitted frames.
    pub tx_frames: usize,
    /// Number of received frames.
    pub rx_frames: usize,
    /// Number of error frames.
    pub error_frames: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_conversion() {
        assert_eq!(timestamp_to_seconds(0), 0.0);
        assert_eq!(timestamp_to_seconds(1_000_000), 1.0);
        assert_eq!(timestamp_to_seconds(500_000), 0.5);
        assert_eq!(timestamp_to_seconds(1_500_000), 1.5);
    }

    #[test]
    fn test_timestamped_frame() {
        let frame = TimestampedFrame::new(2_500_000u64, vec![1u8, 2, 3]);
        assert_eq!(frame.timestamp_s, 2.5);
        assert_eq!(frame.frame, vec![1, 2, 3]);
    }
}
