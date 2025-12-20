//! Example: Writing CAN bus data to MDF4 using DBC definitions
//!
//! This example demonstrates how to log CAN bus data to MDF4 files using
//! signal definitions from a DBC file. The `can` module uses `dbc-rs` for
//! signal decoding, supporting all DBC features including multiplexing.
//!
//! # Usage
//!
//! ```toml
//! [dependencies]
//! mdf4-rs = { version = "0.1", features = ["dbc", "can"] }
//! ```
//!
//! # Running this example
//!
//! ```bash
//! cargo run --example no_std_write --features dbc,can
//! ```

use embedded_can::{ExtendedId, Frame as CanFrameTrait, Id, StandardId};
use mdf4_rs::Result;
use mdf4_rs::can::CanDbcLogger;

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

    #[allow(dead_code)]
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
        None
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

/// DBC file content defining our CAN signals
const DBC_CONTENT: &str = r#"VERSION "1.0"

BU_: ECU

BO_ 256 EngineData : 8 ECU
 SG_ EngineRPM : 0|16@1+ (1,0) [0|8000] "rpm" Vector__XXX
 SG_ VehicleSpeed : 16|16@1+ (1,0) [0|300] "km/h" Vector__XXX
 SG_ ThrottlePosition : 32|8@1+ (1,0) [0|100] "%" Vector__XXX

BO_ 512 TemperatureData : 8 ECU
 SG_ CoolantTemp : 0|8@1+ (1,-40) [-40|215] "C" Vector__XXX
 SG_ OilTemp : 8|8@1+ (1,-40) [-40|215] "C" Vector__XXX
 SG_ IntakeTemp : 16|8@1+ (1,-40) [-40|215] "C" Vector__XXX

BO_ 768 WheelSpeeds : 8 ECU
 SG_ WheelSpeed_FL : 7|16@0+ (0.01,0) [0|300] "km/h" Vector__XXX
 SG_ WheelSpeed_FR : 23|16@0+ (0.01,0) [0|300] "km/h" Vector__XXX
"#;

/// Simulate receiving CAN frames from a vehicle bus
fn simulate_can_traffic() -> Vec<TimestampedCanFrame> {
    let mut frames = Vec::new();
    let mut timestamp = 0u64;

    // Simulate 100ms of CAN traffic
    for i in 0..100 {
        timestamp += 1000; // 1ms between iterations

        // Engine data (CAN ID 0x100) - every 10ms
        if i % 10 == 0 {
            let rpm = 2500 + (i as u16 * 10);
            let speed = 60 + (i / 5) as u16;
            let throttle = 25 + (i / 4) as u8;

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
            // Raw value = temp_celsius + 40
            let coolant_raw = 125u8; // 85°C
            let oil_raw = 135u8; // 95°C
            let intake_raw = 75u8; // 35°C

            let data = [coolant_raw, oil_raw, intake_raw, 0, 0, 0, 0, 0];
            frames.push(TimestampedCanFrame {
                timestamp_us: timestamp,
                frame: CanFrame::new_standard(0x200, &data),
            });
        }

        // Wheel speeds (CAN ID 0x300) - every 20ms
        if i % 20 == 0 {
            let fl_speed: u16 = 6000 + (i as u16 * 5); // 0.01 km/h resolution
            let fr_speed: u16 = 6005 + (i as u16 * 5);

            // Big-endian encoding (MSB first)
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
    }

    frames
}

fn main() -> Result<()> {
    println!("MDF4 CAN Bus Logging Example (DBC-based)");
    println!("=========================================\n");

    // Parse DBC file
    let dbc = mdf4_rs::can::Dbc::parse(DBC_CONTENT).expect("Failed to parse DBC");
    println!("Loaded DBC with {} messages:", dbc.messages().len());
    for msg in dbc.messages().iter() {
        println!(
            "  0x{:X} {}: {} signals",
            msg.id(),
            msg.name(),
            msg.signals().len()
        );
    }

    // Simulate CAN traffic
    let frames = simulate_can_traffic();
    println!("\nSimulated {} CAN frames", frames.len());

    // Create logger
    let mut logger = CanDbcLogger::new(&dbc)?;

    // Log all frames
    for tsf in &frames {
        logger.log_frame(tsf.timestamp_us, &tsf.frame);
    }

    // Show frame counts
    println!("\nFrames logged per message:");
    for can_id in logger.can_ids() {
        println!("  0x{:X}: {} frames", can_id, logger.frame_count(can_id));
    }

    // Finalize and get MDF bytes
    let mdf_bytes = logger.finalize()?;
    println!("\nMDF size: {} bytes", mdf_bytes.len());

    // Write to file for verification (std only)
    #[cfg(feature = "std")]
    {
        let path = std::env::temp_dir().join("can_dbc_example.mf4");
        std::fs::write(&path, &mdf_bytes)?;
        println!("MDF file written to: {}", path.display());

        // Verify by reading it back
        let mdf = mdf4_rs::MDF::from_file(path.to_str().unwrap())?;
        println!("\nVerification - channels in file:");
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

    println!("\nThe DBC-based logger:");
    println!("  - Uses Dbc::decode() for full DBC support");
    println!("  - Handles multiplexing, value descriptions, etc.");
    println!("  - Works with the embedded-can Frame trait");

    Ok(())
}
