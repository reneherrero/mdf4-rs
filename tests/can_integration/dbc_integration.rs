//! End-to-end integration test: CAN Driver -> DBC Decode -> MDF4 -> Read

use mdf4_rs::can::CanDbcLogger;
use mdf4_rs::{MDF, Result};

use super::{FakeCanDriver, VEHICLE_DBC};

#[test]
fn end_to_end_can_to_mdf4_integration() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Integration Test: CAN -> DBC Decode -> MDF4 -> Read");
    println!("{}\n", "=".repeat(80));

    // Step 1: Parse DBC and create logger
    println!("Step 1: Parsing DBC file...");
    let dbc = dbc_rs::Dbc::parse(VEHICLE_DBC).expect("Failed to parse DBC");
    println!("  - Found {} messages:", dbc.messages().len());
    for msg in dbc.messages().iter() {
        println!(
            "    - 0x{:03X} {} ({} signals)",
            msg.id(),
            msg.name(),
            msg.signals().iter().count()
        );
    }

    // Step 2: Create MDF logger with full metadata
    println!("\nStep 2: Creating MDF4 logger with DBC integration...");
    let mut logger = CanDbcLogger::builder(&dbc)
        .store_raw_values(true)
        .include_units(true)
        .include_limits(true)
        .include_conversions(true)
        .include_value_descriptions(true)
        .build()?;
    println!("  - Logger configured for raw value storage with conversions");

    // Step 3: Create fake CAN driver and simulate vehicle
    println!("\nStep 3: Simulating vehicle CAN bus traffic...");
    let mut can_driver = FakeCanDriver::new();

    let scenario = [
        (500, "Starting in Park, engine idle"),
        (200, "Shift to Drive"),
        (1000, "Accelerating (50% throttle)"),
        (500, "Accelerating (100% throttle)"),
        (500, "Coasting"),
        (500, "Braking (50%)"),
        (300, "Stopped"),
    ];

    let mut total_frames = 0;
    for (duration_ms, action) in &scenario {
        println!("  - {}", action);

        match *action {
            "Shift to Drive" => can_driver.ecu.set_gear(3),
            "Accelerating (50% throttle)" => can_driver.ecu.set_throttle(0.5),
            "Accelerating (100% throttle)" => can_driver.ecu.set_throttle(1.0),
            "Coasting" => {
                can_driver.ecu.set_throttle(0.0);
                can_driver.ecu.set_brake(0.0);
            }
            "Braking (50%)" => can_driver.ecu.set_brake(0.5),
            _ => {}
        }

        for _ in 0..(*duration_ms / 10) {
            can_driver.step(10);

            for (timestamp, frame) in can_driver.drain_frames() {
                if logger.log_frame(timestamp, &frame) {
                    total_frames += 1;
                }
            }
        }
    }

    println!("  - Total frames logged: {}", total_frames);
    println!("  - Frames per message:");
    for msg in dbc.messages().iter() {
        let count = logger.frame_count(msg.id());
        if count > 0 {
            println!("    - 0x{:03X} {}: {} frames", msg.id(), msg.name(), count);
        }
    }

    // Step 4: Finalize and get MDF4 bytes
    println!("\nStep 4: Finalizing MDF4 file...");
    let mdf_bytes = logger.finalize()?;
    println!("  - MDF4 file size: {} bytes", mdf_bytes.len());

    // Step 5: Write to temporary file and read back
    let temp_path = std::env::temp_dir().join("can_integration_test.mf4");
    println!("\nStep 5: Writing to temporary file: {:?}", temp_path);
    std::fs::write(&temp_path, &mdf_bytes)?;

    // Step 6: Read MDF4 file and print contents
    println!("\nStep 6: Reading MDF4 file and printing contents...");
    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;

    println!("\n{}", "=".repeat(80));
    println!("MDF4 FILE CONTENTS");
    println!("{}", "=".repeat(80));

    let groups = mdf.channel_groups();
    println!("\nTotal Channel Groups: {}", groups.len());

    for (gidx, group) in groups.iter().enumerate() {
        let group_name = group.name()?.unwrap_or_else(|| "(unnamed)".to_string());
        println!("\n{}", "-".repeat(60));
        println!("Channel Group [{}]: {}", gidx, group_name);
        println!("{}", "-".repeat(60));

        let channels = group.channels();
        println!("  Channels: {}", channels.len());

        for (cidx, channel) in channels.iter().enumerate() {
            let name = channel
                .name()?
                .unwrap_or_else(|| format!("Channel{}", cidx));
            let unit = channel.unit()?.unwrap_or_default();
            let data_type = format!("{:?}", channel.block().data_type);
            let bit_count = channel.block().bit_count;

            print!("\n  [{}] {} ", cidx, name);
            if !unit.is_empty() {
                print!("[{}] ", unit);
            }
            println!("({}, {} bits)", data_type, bit_count);

            match channel.values() {
                Ok(vals) => {
                    let valid_count = vals.iter().filter(|v| v.is_some()).count();
                    println!("      Samples: {}/{} valid", valid_count, vals.len());

                    if !vals.is_empty() {
                        let show_count = 3.min(vals.len());
                        print!("      First {}: ", show_count);
                        for i in 0..show_count {
                            if let Some(val) = &vals[i] {
                                print!("{:?} ", val);
                            } else {
                                print!("None ");
                            }
                        }
                        println!();

                        if vals.len() > show_count * 2 {
                            println!("      ...");
                            print!("      Last {}:  ", show_count);
                            for i in (vals.len() - show_count)..vals.len() {
                                if let Some(val) = &vals[i] {
                                    print!("{:?} ", val);
                                } else {
                                    print!("None ");
                                }
                            }
                            println!();
                        }
                    }
                }
                Err(e) => println!("      Error reading values: {}", e),
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("END OF MDF4 FILE CONTENTS");
    println!("{}\n", "=".repeat(80));

    // Cleanup
    std::fs::remove_file(&temp_path)?;
    println!("Temporary file removed.\n");

    // Verify we got meaningful data
    assert!(groups.len() >= 3, "Expected at least 3 channel groups");
    for group in groups.iter() {
        let channels = group.channels();
        assert!(!channels.is_empty(), "Each group should have channels");
        for channel in channels.iter() {
            let vals = channel.values()?;
            assert!(!vals.is_empty(), "Each channel should have values");
        }
    }

    println!("Integration test PASSED!\n");
    Ok(())
}
