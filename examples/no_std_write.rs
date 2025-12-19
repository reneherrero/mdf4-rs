//! Example: Writing CAN bus data to MDF4 in a no_std environment
//!
//! This example demonstrates how to log CAN bus data to MDF4 files using only the
//! `alloc` feature, without requiring `std`. This is useful for embedded systems
//! like automotive ECUs, data loggers, and other resource-constrained devices.
//!
//! # no_std Usage
//!
//! In your embedded project's `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! mdf4-rs = { version = "0.1", default-features = false, features = ["alloc", "embedded-can"] }
//! ```
//!
//! # Running this example
//!
//! ```bash
//! cargo run --example no_std_write --features embedded-can
//! ```

use embedded_can::{ExtendedId, Frame as CanFrameTrait, Id, StandardId};
use mdf4_rs::can::{CanDataBuffer, SignalDefinition, SignalExtractor};
use mdf4_rs::writer::VecWriter;
use mdf4_rs::{DataType, DecodedValue, MdfWriter, Result};

/// A simple CAN frame implementation for demonstration.
/// In a real embedded system, you'd use your HAL's CAN frame type.
#[derive(Debug, Clone)]
struct CanFrame {
    id: Id,
    data: [u8; 8],
    dlc: usize,
}

impl CanFrame {
    fn new_standard(id: u16, data: &[u8]) -> Self {
        let mut frame_data = [0u8; 8];
        let dlc = data.len().min(8);
        frame_data[..dlc].copy_from_slice(&data[..dlc]);
        Self {
            id: Id::Standard(StandardId::new(id).unwrap()),
            data: frame_data,
            dlc,
        }
    }

    fn new_extended(id: u32, data: &[u8]) -> Self {
        let mut frame_data = [0u8; 8];
        let dlc = data.len().min(8);
        frame_data[..dlc].copy_from_slice(&data[..dlc]);
        Self {
            id: Id::Extended(ExtendedId::new(id).unwrap()),
            data: frame_data,
            dlc,
        }
    }
}

impl CanFrameTrait for CanFrame {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        let mut frame_data = [0u8; 8];
        let dlc = data.len().min(8);
        frame_data[..dlc].copy_from_slice(&data[..dlc]);
        Some(Self {
            id: id.into(),
            data: frame_data,
            dlc,
        })
    }

    fn new_remote(_id: impl Into<Id>, _dlc: usize) -> Option<Self> {
        None // Remote frames not supported in this example
    }

    fn is_extended(&self) -> bool {
        matches!(self.id, Id::Extended(_))
    }

    fn is_remote_frame(&self) -> bool {
        false
    }

    fn id(&self) -> Id {
        self.id
    }

    fn dlc(&self) -> usize {
        self.dlc
    }

    fn data(&self) -> &[u8] {
        &self.data[..self.dlc]
    }
}

/// Timestamped CAN frame for logging
struct TimestampedCanFrame {
    timestamp_us: u64,
    frame: CanFrame,
}

/// Simulate receiving CAN frames from a vehicle bus
fn simulate_can_traffic() -> Vec<TimestampedCanFrame> {
    let mut frames = Vec::new();
    let mut timestamp = 0u64;

    // Simulate 100ms of CAN traffic at ~1000 frames/second
    for i in 0..100 {
        timestamp += 1000; // 1ms between frames

        // Engine data (CAN ID 0x100) - every 10ms
        if i % 10 == 0 {
            let rpm = 2500 + (i as u16 * 10); // RPM increases
            let speed = 60 + (i / 5) as u16; // Speed in km/h
            let throttle = 25 + (i / 4) as u8; // Throttle %

            let data = [
                (rpm & 0xFF) as u8,
                (rpm >> 8) as u8,
                (speed & 0xFF) as u8,
                (speed >> 8) as u8,
                throttle,
                0,
                0,
                0,
            ];
            frames.push(TimestampedCanFrame {
                timestamp_us: timestamp,
                frame: CanFrame::new_standard(0x100, &data),
            });
        }

        // Temperature data (CAN ID 0x200) - every 100ms
        if i % 100 == 0 {
            // Store temperatures with +40 offset (common automotive convention)
            // Raw value = temp_celsius + 40, so 0 = -40°C, 255 = 215°C
            let coolant_raw = 125u8; // 85°C + 40 = 125
            let oil_raw = 135u8; // 95°C + 40 = 135
            let intake_raw = 75u8; // 35°C + 40 = 75

            let data = [
                coolant_raw,
                oil_raw,
                intake_raw,
                0,
                0,
                0,
                0,
                0,
            ];
            frames.push(TimestampedCanFrame {
                timestamp_us: timestamp,
                frame: CanFrame::new_standard(0x200, &data),
            });
        }

        // Wheel speeds (CAN ID 0x300) - every 20ms, big-endian signals
        if i % 20 == 0 {
            let fl_speed: u16 = 6000 + (i as u16 * 5); // 0.01 km/h resolution
            let fr_speed: u16 = 6005 + (i as u16 * 5);

            // Big-endian encoding
            let data = [
                (fl_speed >> 8) as u8,
                (fl_speed & 0xFF) as u8,
                (fr_speed >> 8) as u8,
                (fr_speed & 0xFF) as u8,
                0,
                0,
                0,
                0,
            ];
            frames.push(TimestampedCanFrame {
                timestamp_us: timestamp,
                frame: CanFrame::new_standard(0x300, &data),
            });
        }

        // Extended ID frame (29-bit) - diagnostic data every 50ms
        if i % 50 == 0 {
            let battery_voltage: u16 = 1380; // 13.8V * 100
            let battery_current: i16 = 250; // 2.5A * 100

            let data = [
                (battery_voltage & 0xFF) as u8,
                (battery_voltage >> 8) as u8,
                (battery_current & 0xFF) as u8,
                (battery_current >> 8) as u8,
                0,
                0,
                0,
                0,
            ];
            frames.push(TimestampedCanFrame {
                timestamp_us: timestamp,
                frame: CanFrame::new_extended(0x18DAF110, &data), // OBD-II extended ID
            });
        }
    }

    frames
}

/// Define CAN signals we want to extract (like a DBC file)
fn define_signals() -> Vec<SignalDefinition> {
    vec![
        // Engine data (0x100)
        SignalDefinition::new(0x100, "EngineRPM", 0, 16).with_scale(1.0),
        SignalDefinition::new(0x100, "VehicleSpeed", 16, 16).with_scale(1.0),
        SignalDefinition::new(0x100, "ThrottlePosition", 32, 8).with_scale(1.0),
        // Temperature data (0x200) - with offset for signed temps stored as unsigned
        SignalDefinition::new(0x200, "CoolantTemp", 0, 8).with_offset(-40.0),
        SignalDefinition::new(0x200, "OilTemp", 8, 8).with_offset(-40.0),
        SignalDefinition::new(0x200, "IntakeTemp", 16, 8).with_offset(-40.0),
        // Wheel speeds (0x300) - big-endian
        SignalDefinition::new(0x300, "WheelSpeed_FL", 0, 16)
            .with_scale(0.01)
            .big_endian(),
        SignalDefinition::new(0x300, "WheelSpeed_FR", 16, 16)
            .with_scale(0.01)
            .big_endian(),
        // Battery data (extended ID 0x18DAF110)
        SignalDefinition::new(0x18DAF110, "BatteryVoltage", 0, 16).with_scale(0.01),
        SignalDefinition::new(0x18DAF110, "BatteryCurrent", 16, 16)
            .with_scale(0.01)
            .signed(),
    ]
}

/// Get the raw CAN ID as u32 from an embedded_can::Id
fn get_can_id(id: Id) -> u32 {
    match id {
        Id::Standard(sid) => sid.as_raw() as u32,
        Id::Extended(eid) => eid.as_raw(),
    }
}

/// Process CAN frames and write to MDF - using CanDataBuffer for efficiency
fn create_mdf_from_can_buffered(frames: &[TimestampedCanFrame]) -> Result<Vec<u8>> {
    let signals = define_signals();

    // Create buffer for accumulating CAN data
    let mut buffer = CanDataBuffer::new(signals);

    // Process all frames into the buffer
    for tsf in frames {
        let can_id = get_can_id(tsf.frame.id());
        buffer.push(can_id, tsf.timestamp_us, tsf.frame.data());
    }

    // Now write the buffered data to MDF
    let writer = VecWriter::with_capacity(8192);
    let mut mdf = MdfWriter::from_writer(writer);
    mdf.init_mdf_file()?;

    // Create a channel group for each CAN ID
    for can_id in buffer.can_ids() {
        let timestamps = match buffer.timestamps(can_id) {
            Some(ts) if !ts.is_empty() => ts,
            _ => continue,
        };

        let cg = mdf.add_channel_group(None, |_| {})?;

        // Add timestamp channel
        let time_ch = mdf.add_channel(&cg, None, |ch| {
            ch.data_type = DataType::UnsignedIntegerLE;
            ch.name = Some(format!("Time_0x{:X}", can_id));
            ch.bit_count = 64;
        })?;
        mdf.set_time_channel(&time_ch)?;

        // Add signal channels
        let signal_defs: Vec<_> = buffer.signals_for_id(can_id).collect();
        let mut prev_ch = time_ch.clone();

        for (idx, sig_def) in signal_defs.iter().enumerate() {
            let ch = mdf.add_channel(&cg, Some(&prev_ch), |ch| {
                ch.data_type = DataType::FloatLE;
                ch.name = Some(sig_def.name.clone());
                ch.bit_count = 64;
            })?;
            prev_ch = ch;

            // Store signal index for later use
            let _ = idx; // Used implicitly via enumeration
        }

        // Write data records
        mdf.start_data_block_for_cg(&cg, 0)?;

        for (record_idx, &ts) in timestamps.iter().enumerate() {
            let mut values = vec![DecodedValue::UnsignedInteger(ts)];

            for sig_idx in 0..signal_defs.len() {
                if let Some(sig_values) = buffer.signal_values(can_id, sig_idx) {
                    if record_idx < sig_values.len() {
                        values.push(DecodedValue::Float(sig_values[record_idx]));
                    }
                }
            }

            mdf.write_record(&cg, &values)?;
        }

        mdf.finish_data_block(&cg)?;
    }

    mdf.finalize()?;
    Ok(mdf.into_inner().into_inner())
}

/// Alternative: Process frames directly without buffering (lower memory, streaming)
fn create_mdf_from_can_streaming(frames: &[TimestampedCanFrame]) -> Result<Vec<u8>> {
    let signals = define_signals();
    let extractor = SignalExtractor::new(signals);

    let writer = VecWriter::with_capacity(8192);
    let mut mdf = MdfWriter::from_writer(writer);
    mdf.init_mdf_file()?;

    // Create a single channel group for raw CAN data
    let cg = mdf.add_channel_group(None, |_| {})?;

    // Channels: Timestamp, CAN_ID, DLC, Data[8]
    let time_ch = mdf.add_channel(&cg, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Timestamp".into());
        ch.bit_count = 64;
    })?;
    mdf.set_time_channel(&time_ch)?;

    let id_ch = mdf.add_channel(&cg, Some(&time_ch), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("CAN_ID".into());
        ch.bit_count = 32;
    })?;

    let dlc_ch = mdf.add_channel(&cg, Some(&id_ch), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("DLC".into());
        ch.bit_count = 8;
    })?;

    // Add 8 data byte channels
    let mut prev_ch = dlc_ch;
    for i in 0..8 {
        let ch = mdf.add_channel(&cg, Some(&prev_ch), |ch| {
            ch.data_type = DataType::UnsignedIntegerLE;
            ch.name = Some(format!("Data{}", i));
            ch.bit_count = 8;
        })?;
        prev_ch = ch;
    }

    // Write raw CAN data
    mdf.start_data_block_for_cg(&cg, 0)?;

    for tsf in frames {
        let can_id = get_can_id(tsf.frame.id());
        let data = tsf.frame.data();

        let mut values = vec![
            DecodedValue::UnsignedInteger(tsf.timestamp_us),
            DecodedValue::UnsignedInteger(can_id as u64),
            DecodedValue::UnsignedInteger(tsf.frame.dlc() as u64),
        ];

        // Add 8 data bytes (padded with zeros)
        for i in 0..8 {
            let byte_val = if i < data.len() { data[i] } else { 0 };
            values.push(DecodedValue::UnsignedInteger(byte_val as u64));
        }

        mdf.write_record(&cg, &values)?;
    }

    mdf.finish_data_block(&cg)?;

    // Also demonstrate signal extraction for display
    println!("\nExtracted signals from first few frames:");
    for tsf in frames.iter().take(5) {
        let can_id = get_can_id(tsf.frame.id());
        if extractor.has_can_id(can_id) {
            print!("  t={}us ID=0x{:X}: ", tsf.timestamp_us, can_id);
            for (name, value) in extractor.extract_all(can_id, tsf.frame.data()) {
                print!("{}={:.2} ", name, value);
            }
            println!();
        }
    }

    mdf.finalize()?;
    Ok(mdf.into_inner().into_inner())
}

fn main() -> Result<()> {
    println!("MDF4 CAN Bus Logging Example");
    println!("============================\n");

    // Simulate receiving CAN frames
    let frames = simulate_can_traffic();
    println!("Simulated {} CAN frames", frames.len());

    // Count frames by ID
    let mut id_counts = std::collections::BTreeMap::new();
    for f in &frames {
        let id = get_can_id(f.frame.id());
        *id_counts.entry(id).or_insert(0) += 1;
    }
    println!("\nFrames by CAN ID:");
    for (id, count) in &id_counts {
        let id_type = if *id > 0x7FF { "extended" } else { "standard" };
        println!("  0x{:X} ({}): {} frames", id, id_type, count);
    }

    // Method 1: Buffered approach (groups signals by CAN ID)
    println!("\n--- Method 1: Buffered (signals grouped by CAN ID) ---");
    let mdf_buffered = create_mdf_from_can_buffered(&frames)?;
    println!("MDF size: {} bytes", mdf_buffered.len());

    // Method 2: Streaming approach (raw CAN frames)
    println!("\n--- Method 2: Streaming (raw CAN frames) ---");
    let mdf_streaming = create_mdf_from_can_streaming(&frames)?;
    println!("MDF size: {} bytes", mdf_streaming.len());

    // Write to file for verification (std only)
    #[cfg(feature = "std")]
    {
        let path = std::env::temp_dir().join("can_logging_example.mf4");
        std::fs::write(&path, &mdf_buffered)?;
        println!("\nMDF file written to: {}", path.display());

        // Verify by reading it back
        let mdf = mdf4_rs::MDF::from_file(path.to_str().unwrap())?;
        println!("\nVerification - channel groups in file:");
        for group in mdf.channel_groups() {
            println!("  Group: {:?}", group.name()?);
            for channel in group.channels() {
                let name = channel.name()?.unwrap_or_default();
                let values = channel.values()?;
                let valid_count = values.iter().filter(|v| v.is_some()).count();
                println!("    {}: {} samples", name, valid_count);
            }
        }
    }

    println!("\nIn a real embedded system:");
    println!("  - CAN frames come from hardware (bxCAN, FDCAN, MCP2515, etc.)");
    println!("  - MDF bytes are written to SD card, flash, or transmitted");
    println!("  - The file can be analyzed in CANape, INCA, or other tools");

    Ok(())
}

