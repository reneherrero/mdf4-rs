//! End-to-end integration test: CAN FD logging -> MDF4 -> Read

use mdf4_rs::{MDF, Result, DecodedValue};
use mdf4_rs::can::{RawCanLogger, FdFlags, SimpleFdFrame};
use embedded_can::StandardId;

/// Test CAN FD frame logging with raw logger
#[test]
fn end_to_end_raw_can_fd_to_mdf4() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: CAN FD Raw Logging -> MDF4 -> Read");
    println!("{}\n", "=".repeat(80));

    // Step 1: Create raw CAN logger
    println!("Step 1: Creating raw CAN logger for FD frames...");
    let mut logger = RawCanLogger::new()?;

    // Step 2: Log various CAN FD frame sizes
    println!("\nStep 2: Logging CAN FD frames of various sizes...");

    // Classic CAN frame (8 bytes)
    logger.log(0x100, 1000, &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    println!("  - Logged classic CAN frame (8 bytes) at 0x100");

    // CAN FD frames with increasing sizes
    let fd_12_bytes = [0xAA; 12];
    let fd_16_bytes = [0xBB; 16];
    let fd_24_bytes = [0xCC; 24];
    let fd_32_bytes = [0xDD; 32];
    let fd_48_bytes = [0xEE; 48];
    let fd_64_bytes = [0xFF; 64];

    // Log FD frames with BRS flag
    let brs_flags = FdFlags::new(true, false);
    logger.log_fd(0x200, 2000, &fd_12_bytes, brs_flags);
    println!("  - Logged CAN FD frame (12 bytes, BRS) at 0x200");

    logger.log_fd(0x201, 3000, &fd_16_bytes, brs_flags);
    println!("  - Logged CAN FD frame (16 bytes, BRS) at 0x201");

    logger.log_fd(0x202, 4000, &fd_24_bytes, brs_flags);
    println!("  - Logged CAN FD frame (24 bytes, BRS) at 0x202");

    logger.log_fd(0x203, 5000, &fd_32_bytes, brs_flags);
    println!("  - Logged CAN FD frame (32 bytes, BRS) at 0x203");

    logger.log_fd(0x204, 6000, &fd_48_bytes, brs_flags);
    println!("  - Logged CAN FD frame (48 bytes, BRS) at 0x204");

    logger.log_fd(0x205, 7000, &fd_64_bytes, brs_flags);
    println!("  - Logged CAN FD frame (64 bytes, BRS) at 0x205");

    // Log FD frame with ESI flag
    let esi_flags = FdFlags::new(false, true);
    logger.log_fd(0x300, 8000, &[0x12; 20], esi_flags);
    println!("  - Logged CAN FD frame (20 bytes, ESI) at 0x300");

    // Log FD frame with both BRS and ESI
    let both_flags = FdFlags::new(true, true);
    logger.log_fd(0x400, 9000, &[0x34; 48], both_flags);
    println!("  - Logged CAN FD frame (48 bytes, BRS+ESI) at 0x400");

    println!("\n  - Total: {} frames, {} unique IDs",
             logger.total_frame_count(), logger.unique_id_count());
    println!("  - Has FD frames: {}", logger.has_fd_frames());
    println!("  - Max data length: {} bytes", logger.max_data_length());

    // Step 3: Finalize and write MDF4 file
    println!("\nStep 3: Finalizing MDF4 file...");
    let mdf_bytes = logger.finalize()?;
    println!("  - MDF4 file size: {} bytes", mdf_bytes.len());

    let temp_path = std::env::temp_dir().join("can_fd_integration_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;
    println!("  - Written to: {:?}", temp_path);

    // Step 4: Read MDF4 file back and verify
    println!("\nStep 4: Reading MDF4 file and verifying...");
    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();
    println!("  - Found {} channel groups", groups.len());

    // Verify we have the expected number of groups (one per CAN ID)
    assert_eq!(groups.len(), 9, "Expected 9 channel groups (one per CAN ID)");

    // Verify channel structure for FD frames
    for group in groups.iter() {
        let group_name = group.name()?.unwrap_or_else(|| "(unnamed)".to_string());
        let channels = group.channels();

        println!("\n  Channel Group: {} ({} channels)", group_name, channels.len());

        // FD frames should have: Timestamp, CAN_ID, DLC, FD_Flags, Data_0..Data_N
        let channel_names: Vec<String> = channels.iter()
            .map(|c| c.name().ok().flatten().unwrap_or_default())
            .collect();

        assert!(channel_names.contains(&"Timestamp".to_string()));
        assert!(channel_names.contains(&"CAN_ID".to_string()));
        assert!(channel_names.contains(&"DLC".to_string()));
        assert!(channel_names.contains(&"FD_Flags".to_string()));
        assert!(channel_names.contains(&"Data_0".to_string()));

        // Print data channels present
        let data_channels: Vec<_> = channel_names.iter()
            .filter(|n| n.starts_with("Data_"))
            .collect();
        println!("    Data channels: {} (Data_0 to Data_{})",
                 data_channels.len(), data_channels.len() - 1);
    }

    // Step 5: Verify specific frame data
    println!("\n{}", "=".repeat(80));
    println!("VERIFYING FRAME DATA");
    println!("{}", "=".repeat(80));

    for group in groups.iter() {
        let group_name = group.name()?.unwrap_or_default();
        let channels = group.channels();

        let mut fd_flags_vals: Vec<u8> = Vec::new();
        let mut dlc_vals: Vec<u8> = Vec::new();
        let mut can_id_vals: Vec<u32> = Vec::new();
        let mut data_vals: Vec<Vec<u8>> = vec![Vec::new(); 64];

        for channel in channels.iter() {
            let name = channel.name()?.unwrap_or_default();
            let vals = channel.values()?;

            match name.as_str() {
                "CAN_ID" => {
                    for v in vals.iter().flatten() {
                        if let DecodedValue::UnsignedInteger(id) = v {
                            can_id_vals.push(*id as u32);
                        }
                    }
                }
                "DLC" => {
                    for v in vals.iter().flatten() {
                        if let DecodedValue::UnsignedInteger(d) = v {
                            dlc_vals.push(*d as u8);
                        }
                    }
                }
                "FD_Flags" => {
                    for v in vals.iter().flatten() {
                        if let DecodedValue::UnsignedInteger(f) = v {
                            fd_flags_vals.push(*f as u8);
                        }
                    }
                }
                name if name.starts_with("Data_") => {
                    if let Ok(idx) = name.strip_prefix("Data_").unwrap_or("0").parse::<usize>() {
                        if idx < 64 {
                            for v in vals.iter().flatten() {
                                if let DecodedValue::UnsignedInteger(byte) = v {
                                    data_vals[idx].push(*byte as u8);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if !can_id_vals.is_empty() {
            let can_id = can_id_vals[0];
            let flags = if !fd_flags_vals.is_empty() { fd_flags_vals[0] } else { 0 };
            let dlc = if !dlc_vals.is_empty() { dlc_vals[0] } else { 0 };

            let fd_flags = FdFlags::from_byte(flags);
            println!("\n  {} (ID=0x{:03X}): DLC={}, BRS={}, ESI={}",
                     group_name, can_id, dlc, fd_flags.brs(), fd_flags.esi());

            // Verify specific frame contents
            match can_id {
                0x100 => {
                    assert_eq!(dlc, 8, "Classic CAN should have DLC=8");
                    assert!(!fd_flags.brs(), "Classic CAN should not have BRS");
                    assert!(!fd_flags.esi(), "Classic CAN should not have ESI");
                }
                0x200 => {
                    assert!(fd_flags.brs(), "FD frame 0x200 should have BRS");
                    assert!(!fd_flags.esi(), "FD frame 0x200 should not have ESI");
                    // Verify data content
                    for i in 0..12 {
                        if !data_vals[i].is_empty() {
                            assert_eq!(data_vals[i][0], 0xAA, "Data byte {} should be 0xAA", i);
                        }
                    }
                }
                0x300 => {
                    assert!(!fd_flags.brs(), "FD frame 0x300 should not have BRS");
                    assert!(fd_flags.esi(), "FD frame 0x300 should have ESI");
                }
                0x400 => {
                    assert!(fd_flags.brs(), "FD frame 0x400 should have BRS");
                    assert!(fd_flags.esi(), "FD frame 0x400 should have ESI");
                }
                _ => {}
            }
        }
    }

    // Cleanup
    std::fs::remove_file(&temp_path)?;
    println!("\n\nTemporary file removed.");
    println!("\nCAN FD integration test PASSED!\n");

    Ok(())
}

/// Test CAN FD frame logging using the FdFrame trait
#[test]
fn end_to_end_fd_frame_trait() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: FdFrame Trait -> MDF4 -> Read");
    println!("{}\n", "=".repeat(80));

    let mut logger = RawCanLogger::new()?;

    // Create frames using SimpleFdFrame
    let id = StandardId::new(0x500).unwrap();

    // Classic CAN via SimpleFdFrame
    let classic = SimpleFdFrame::new_classic(id, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    logger.log_fd_frame(1000, &classic);

    // CAN FD via SimpleFdFrame
    let fd_data = [0x42; 32];
    let fd_flags = FdFlags::new(true, false);
    let fd_frame = SimpleFdFrame::new_fd_frame(id, &fd_data, fd_flags).unwrap();
    logger.log_fd_frame(2000, &fd_frame);

    println!("  - Logged classic frame (8 bytes) via FdFrame trait");
    println!("  - Logged FD frame (32 bytes, BRS) via FdFrame trait");

    let mdf_bytes = logger.finalize()?;

    let temp_path = std::env::temp_dir().join("fd_frame_trait_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // Should have 1 group (single CAN ID)
    assert_eq!(groups.len(), 1, "Expected 1 channel group");

    let group = &groups[0];
    let channels = group.channels();

    // Find record count
    let first_channel = channels.first();
    let record_count = first_channel.map(|c| c.values().ok())
        .flatten()
        .map(|v| v.len())
        .unwrap_or(0);

    assert_eq!(record_count, 2, "Expected 2 records (classic + FD)");

    std::fs::remove_file(&temp_path)?;

    println!("\nFdFrame trait integration test PASSED!\n");
    Ok(())
}

/// Test mixed classic CAN and CAN FD logging
#[test]
fn end_to_end_mixed_classic_and_fd() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: Mixed Classic CAN + CAN FD -> MDF4");
    println!("{}\n", "=".repeat(80));

    let mut logger = RawCanLogger::new()?;

    // Interleave classic and FD frames on the same CAN ID
    let can_id = 0x600u32;

    // Classic frame
    logger.log(can_id, 1000, &[0x11; 8]);

    // FD frame (12 bytes)
    logger.log_fd(can_id, 2000, &[0x22; 12], FdFlags::new(true, false));

    // Another classic frame
    logger.log(can_id, 3000, &[0x33; 8]);

    // FD frame (24 bytes)
    logger.log_fd(can_id, 4000, &[0x44; 24], FdFlags::new(true, true));

    // FD frame (64 bytes - max)
    logger.log_fd(can_id, 5000, &[0x55; 64], FdFlags::default());

    println!("  - Logged 2 classic + 3 FD frames on same CAN ID");
    println!("  - Max data length seen: {} bytes", logger.max_data_length());

    let mdf_bytes = logger.finalize()?;

    let temp_path = std::env::temp_dir().join("mixed_can_fd_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    assert_eq!(groups.len(), 1, "Expected 1 channel group");

    let group = &groups[0];
    let channels = group.channels();

    // Count data channels - should have up to 64 for FD support
    let data_channel_count = channels.iter()
        .filter(|c| c.name().ok().flatten().map(|n| n.starts_with("Data_")).unwrap_or(false))
        .count();

    assert!(data_channel_count >= 64, "Expected at least 64 data channels for FD support, got {}", data_channel_count);

    // Verify record count
    let record_count = channels.first()
        .map(|c| c.values().ok())
        .flatten()
        .map(|v| v.len())
        .unwrap_or(0);

    assert_eq!(record_count, 5, "Expected 5 records");

    std::fs::remove_file(&temp_path)?;

    println!("\nMixed Classic+FD integration test PASSED!\n");
    Ok(())
}
