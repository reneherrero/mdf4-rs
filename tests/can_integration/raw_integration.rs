//! End-to-end integration test: Raw CAN logging -> MDF4 -> Read and decode with DBC

use mdf4_rs::{MDF, Result, DecodedValue};
use mdf4_rs::can::RawCanLogger;

use super::VEHICLE_DBC;

#[test]
fn end_to_end_raw_can_to_mdf4_then_decode() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: Raw CAN -> MDF4 -> Read -> DBC Decode");
    println!("{}\n", "=".repeat(80));

    // Step 1: Create raw CAN logger (NO DBC at capture time)
    println!("Step 1: Creating raw CAN logger (no DBC needed)...");
    let mut logger = RawCanLogger::new()?;

    // Step 2: Simulate CAN traffic and log raw frames
    println!("\nStep 2: Logging raw CAN frames...");

    // Simulate Engine messages (0x100)
    let engine_frames = [
        (1000u64, [0x40, 0x1F, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00]), // RPM=2000, Temp=20°C
        (2000u64, [0x80, 0x3E, 0x5A, 0x64, 0x00, 0x00, 0x00, 0x00]), // RPM=4000, Temp=50°C
        (3000u64, [0xC0, 0x5D, 0x64, 0xC8, 0x00, 0x00, 0x00, 0x00]), // RPM=6000, Temp=60°C
        (4000u64, [0x00, 0x4B, 0x5A, 0x00, 0x00, 0x00, 0x00, 0x00]), // RPM=4800, Temp=50°C
        (5000u64, [0x40, 0x1F, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00]), // RPM=2000, Temp=40°C
    ];

    // Simulate Vehicle messages (0x200)
    let vehicle_frames = [
        (1500u64, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Speed=0
        (2500u64, [0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Speed=10km/h
        (3500u64, [0x88, 0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Speed=50km/h
        (4500u64, [0x88, 0x13, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00]), // Speed=50km/h, Brake=50%
        (5500u64, [0xF4, 0x01, 0xC8, 0x00, 0x00, 0x00, 0x00, 0x00]), // Speed=5km/h, Brake=100%
    ];

    // Simulate Transmission messages (0x300)
    let transmission_frames = [
        (1000u64, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Park
        (1800u64, [0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Drive
        (5200u64, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Park
    ];

    for (ts, data) in &engine_frames {
        logger.log(0x100, *ts, data);
    }
    for (ts, data) in &vehicle_frames {
        logger.log(0x200, *ts, data);
    }
    for (ts, data) in &transmission_frames {
        logger.log(0x300, *ts, data);
    }

    println!("  - Logged {} Engine frames (0x100)", engine_frames.len());
    println!("  - Logged {} Vehicle frames (0x200)", vehicle_frames.len());
    println!("  - Logged {} Transmission frames (0x300)", transmission_frames.len());
    println!("  - Total: {} frames, {} unique IDs",
             logger.total_frame_count(), logger.unique_id_count());

    // Step 3: Finalize and write MDF4 file
    println!("\nStep 3: Finalizing MDF4 file...");
    let mdf_bytes = logger.finalize()?;
    println!("  - MDF4 file size: {} bytes", mdf_bytes.len());

    let temp_path = std::env::temp_dir().join("raw_can_integration_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;
    println!("  - Written to: {:?}", temp_path);

    // Step 4: Read MDF4 file back
    println!("\nStep 4: Reading MDF4 file...");
    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();
    println!("  - Found {} channel groups", groups.len());

    // Print raw data from MDF
    println!("\n{}", "=".repeat(80));
    println!("RAW MDF4 DATA (before DBC decoding)");
    println!("{}", "=".repeat(80));

    for group in groups.iter() {
        let group_name = group.name()?.unwrap_or_else(|| "(unnamed)".to_string());
        println!("\n{}", "-".repeat(60));
        println!("Channel Group: {}", group_name);
        println!("{}", "-".repeat(60));

        let channels = group.channels();
        let mut channel_values: Vec<(String, Vec<Option<DecodedValue>>)> = Vec::new();
        for channel in channels.iter() {
            let name = channel.name()?.unwrap_or_default();
            let vals = channel.values()?;
            channel_values.push((name, vals));
        }

        if !channel_values.is_empty() {
            let num_records = channel_values[0].1.len();
            println!("  Records: {}", num_records);

            for i in 0..num_records.min(3) {
                print!("  [{}] ", i);
                for (name, vals) in &channel_values {
                    if let Some(val) = &vals[i] {
                        match val {
                            DecodedValue::UnsignedInteger(v) => {
                                if name == "Timestamp" {
                                    print!("{}={}us ", name, v);
                                } else if name == "CAN_ID" {
                                    print!("{}=0x{:03X} ", name, v);
                                } else {
                                    print!("{}={} ", name, v);
                                }
                            }
                            _ => print!("{}={:?} ", name, val),
                        }
                    }
                }
                println!();
            }
            if num_records > 3 {
                println!("  ...");
            }
        }
    }

    // Step 5: Apply DBC decoding to raw data
    println!("\n{}", "=".repeat(80));
    println!("DECODED DATA (after applying DBC)");
    println!("{}", "=".repeat(80));

    let dbc = dbc_rs::Dbc::parse(VEHICLE_DBC).expect("Failed to parse DBC");

    for group in groups.iter() {
        let group_name = group.name()?.unwrap_or_default();
        let channels = group.channels();

        let mut timestamp_vals: Vec<u64> = Vec::new();
        let mut can_id_vals: Vec<u32> = Vec::new();
        let mut data_channels: [Vec<u8>; 8] = Default::default();

        for channel in channels.iter() {
            let name = channel.name()?.unwrap_or_default();
            let vals = channel.values()?;

            match name.as_str() {
                "Timestamp" => {
                    for v in vals.iter() {
                        if let Some(DecodedValue::UnsignedInteger(ts)) = v {
                            timestamp_vals.push(*ts);
                        }
                    }
                }
                "CAN_ID" => {
                    for v in vals.iter() {
                        if let Some(DecodedValue::UnsignedInteger(id)) = v {
                            can_id_vals.push(*id as u32);
                        }
                    }
                }
                name if name.starts_with("Data_") => {
                    if let Ok(idx) = name.strip_prefix("Data_").unwrap_or("0").parse::<usize>() {
                        if idx < 8 {
                            for v in vals.iter() {
                                if let Some(DecodedValue::UnsignedInteger(byte)) = v {
                                    data_channels[idx].push(*byte as u8);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Reconstruct frames
        let num_frames = timestamp_vals.len();
        let mut data_bytes: Vec<[u8; 8]> = Vec::new();
        for i in 0..num_frames {
            let mut frame = [0u8; 8];
            for (j, channel) in data_channels.iter().enumerate() {
                if i < channel.len() {
                    frame[j] = channel[i];
                }
            }
            data_bytes.push(frame);
        }

        println!("\n{}", "-".repeat(60));
        println!("Decoded: {} ({} frames)", group_name, num_frames);
        println!("{}", "-".repeat(60));

        for i in 0..num_frames.min(5) {
            if i < can_id_vals.len() && i < data_bytes.len() && i < timestamp_vals.len() {
                let can_id = can_id_vals[i];
                let data = &data_bytes[i];
                let timestamp = timestamp_vals[i];

                print!("  [{}] ts={}us id=0x{:03X}: ", i, timestamp, can_id);

                match dbc.decode(can_id, data, false) {
                    Ok(decoded_signals) => {
                        for signal in decoded_signals.iter() {
                            if let Some(text) = signal.description {
                                print!("{}={} ", signal.name, text);
                            } else {
                                print!("{}={:.2}{} ",
                                    signal.name,
                                    signal.value,
                                    signal.unit.unwrap_or(""));
                            }
                        }
                        println!();
                    }
                    Err(_) => {
                        println!("(unknown message)");
                    }
                }
            }
        }
        if num_frames > 5 {
            println!("  ...");
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("END OF DECODED DATA");
    println!("{}\n", "=".repeat(80));

    // Cleanup
    std::fs::remove_file(&temp_path)?;
    println!("Temporary file removed.");

    assert_eq!(groups.len(), 3, "Expected 3 channel groups (one per CAN ID)");

    println!("\nRaw CAN integration test PASSED!\n");
    Ok(())
}
