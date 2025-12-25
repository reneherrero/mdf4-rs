//! Example: CAN bus logging to MDF4 files.
//!
//! This example demonstrates three workflows for logging CAN data:
//!
//! 1. **Decoded logging** - Use CanDbcLogger with a DBC file for decoded signals
//! 2. **Raw logging** - Use RawCanLogger without DBC, decode later
//! 3. **Post-processing** - Use DbcOverlayReader to decode raw captures
//!
//! Run with: `cargo run --example can_logging --features dbc`

use mdf4_rs::can::{CanDbcLogger, DbcOverlayReader, RawCanLogger};
use mdf4_rs::{FileRangeReader, FlushPolicy, MDF};

fn main() -> Result<(), mdf4_rs::Error> {
    // Sample DBC content
    let dbc_content = r#"VERSION "1.0"

BU_: ECM TCM

BO_ 256 Engine : 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
 SG_ Temp : 16|8@1+ (1,-40) [-40|215] "C" Vector__XXX
 SG_ Throttle : 24|8@1+ (0.392157,0) [0|100] "%" Vector__XXX

BO_ 512 Transmission : 8 TCM
 SG_ Gear : 0|4@1+ (1,0) [0|6] "" Vector__XXX
 SG_ Speed : 8|16@1+ (0.01,0) [0|300] "km/h" Vector__XXX

VAL_ 512 Gear 0 "Park" 1 "Reverse" 2 "Neutral" 3 "Drive" 4 "Sport" ;
"#;

    let dbc = dbc_rs::Dbc::parse(dbc_content).expect("Failed to parse DBC");

    // Simulated CAN frames (timestamp_us, can_id, data)
    let frames = [
        (1000, 256u32, [0x40, 0x1F, 0x5A, 0x80, 0, 0, 0, 0]), // RPM=2000, Temp=50, Throttle=50%
        (2000, 512, [0x03, 0x88, 0x13, 0, 0, 0, 0, 0]),       // Gear=Drive, Speed=50 km/h
        (3000, 256, [0x80, 0x3E, 0x64, 0xC0, 0, 0, 0, 0]),    // RPM=4000, Temp=60, Throttle=75%
        (4000, 512, [0x04, 0x10, 0x27, 0, 0, 0, 0, 0]),       // Gear=Sport, Speed=100 km/h
        (5000, 256, [0xC0, 0x5D, 0x6E, 0xFF, 0, 0, 0, 0]),    // RPM=6000, Temp=70, Throttle=100%
    ];

    println!("=== Workflow 1: Decoded Logging (CanDbcLogger) ===\n");
    decoded_logging(&dbc, &frames)?;

    println!("\n=== Workflow 2: Raw Logging (RawCanLogger) ===\n");
    raw_logging(&frames)?;

    println!("\n=== Workflow 3: Post-Processing (DbcOverlayReader) ===\n");
    post_processing(&dbc, &frames)?;

    println!("\n=== Workflow 4: Streaming with FlushPolicy ===\n");
    streaming_logging(&dbc)?;

    Ok(())
}

/// Workflow 1: Log decoded CAN signals directly to MDF4
fn decoded_logging(
    dbc: &dbc_rs::Dbc,
    frames: &[(u64, u32, [u8; 8])],
) -> Result<(), mdf4_rs::Error> {
    // Create logger with DBC - signals are decoded immediately
    let mut logger = CanDbcLogger::builder(dbc.clone())
        .store_raw_values(false) // Store physical values (f64)
        .include_units(true)
        .build()?;

    // Log frames
    for (timestamp, can_id, data) in frames {
        logger.log(*can_id, *timestamp, data);
    }

    println!("Logged {} Engine frames", logger.frame_count(256));
    println!("Logged {} Transmission frames", logger.frame_count(512));

    // Finalize and save
    let mdf_bytes = logger.finalize()?;
    let path = std::env::temp_dir().join("decoded_can.mf4");
    std::fs::write(&path, &mdf_bytes)?;
    println!("Saved to: {}", path.display());

    // Read back and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    for group in mdf.channel_groups() {
        let name = group.name()?.unwrap_or_default();
        let channels: Vec<_> = group
            .channels()
            .iter()
            .filter_map(|c| c.name().ok().flatten())
            .collect();
        println!("  Group '{}': {:?}", name, channels);
    }

    Ok(())
}

/// Workflow 2: Log raw CAN frames without DBC
fn raw_logging(frames: &[(u64, u32, [u8; 8])]) -> Result<(), mdf4_rs::Error> {
    // Create raw logger - no DBC needed
    let mut logger = RawCanLogger::new()?;

    // Log frames
    for (timestamp, can_id, data) in frames {
        logger.log(*can_id, *timestamp, data);
    }

    println!("Logged {} total frames", logger.total_frame_count());
    println!("Unique CAN IDs: {}", logger.unique_id_count());

    // Finalize and save
    let mdf_bytes = logger.finalize()?;
    let path = std::env::temp_dir().join("raw_can.mf4");
    std::fs::write(&path, &mdf_bytes)?;
    println!("Saved to: {}", path.display());

    // Read back - shows raw structure
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    for group in mdf.channel_groups() {
        let name = group.name()?.unwrap_or_default();
        println!("  Group '{}': {} channels", name, group.channels().len());
    }

    Ok(())
}

/// Workflow 3: Post-process raw captures with DBC overlay
fn post_processing(
    dbc: &dbc_rs::Dbc,
    frames: &[(u64, u32, [u8; 8])],
) -> Result<(), mdf4_rs::Error> {
    // First, create a raw capture
    let mut logger = RawCanLogger::new()?;
    for (timestamp, can_id, data) in frames {
        logger.log(*can_id, *timestamp, data);
    }
    let mdf_bytes = logger.finalize()?;
    let path = std::env::temp_dir().join("raw_for_overlay.mf4");
    std::fs::write(&path, &mdf_bytes)?;

    // Now apply DBC overlay to decode
    let overlay = DbcOverlayReader::from_file(path.to_str().unwrap(), dbc)?;
    let mut reader = FileRangeReader::new(path.to_str().unwrap())?;

    // Get statistics
    let stats = overlay.statistics(&mut reader)?;
    println!("Capture statistics:");
    println!("  Total frames: {}", stats.total_frames);
    println!("  Unique CAN IDs: {}", stats.unique_can_ids);
    println!(
        "  DBC messages found: {}/{}",
        stats.dbc_messages_found, stats.dbc_messages_total
    );
    println!("  Duration: {} us", stats.duration_us);

    // List available messages
    let messages = overlay.available_messages(&mut reader)?;
    println!("\nAvailable messages: {:?}", messages);

    // Decode Engine frames
    println!("\nEngine frames:");
    for frame in overlay.frames("Engine", &mut reader)? {
        print!("  t={}: ", frame.timestamp_us);
        for (name, value) in &frame.signals {
            print!("{}={:.1} ", name, value);
        }
        println!();
    }

    // Get specific signal values
    println!("\nRPM values:");
    for sv in overlay.signal_values("RPM", &mut reader)? {
        println!(
            "  t={}: {} rpm (raw={})",
            sv.timestamp_us, sv.value, sv.raw_value
        );
    }

    Ok(())
}

/// Workflow 4: Streaming with automatic flush for long captures
fn streaming_logging(dbc: &dbc_rs::Dbc) -> Result<(), mdf4_rs::Error> {
    let path = std::env::temp_dir().join("streaming_can.mf4");

    // Create logger with flush policy
    let mut logger = CanDbcLogger::builder(dbc.clone())
        .with_flush_policy(FlushPolicy::EveryNRecords(100))
        .build_file(path.to_str().unwrap())?;

    // Simulate long capture
    for i in 0..500 {
        let rpm_raw = ((i % 32) * 250) as u16; // 0-8000 RPM cycle
        let data = [
            (rpm_raw & 0xFF) as u8,
            (rpm_raw >> 8) as u8,
            50,  // Temp
            128, // Throttle
            0,
            0,
            0,
            0,
        ];
        logger.log(256, i as u64 * 10_000, &data);
    }

    println!("Logged 500 frames with auto-flush every 100 records");

    logger.finalize_file()?;
    println!("Saved to: {}", path.display());

    // Verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let group = &mdf.channel_groups()[0];
    println!("  Recorded {} samples", group.channels()[0].values()?.len());

    Ok(())
}
