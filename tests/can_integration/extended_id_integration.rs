//! End-to-end integration test: Extended CAN ID (29-bit) logging -> MDF4 -> Read

use mdf4_rs::{MDF, Result, DecodedValue};
use mdf4_rs::can::{RawCanLogger, FdFlags};

/// Test extended CAN ID logging with raw logger
#[test]
fn end_to_end_extended_can_ids() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: Extended CAN IDs (29-bit) -> MDF4 -> Read");
    println!("{}\n", "=".repeat(80));

    let mut logger = RawCanLogger::new()?;

    // Log standard 11-bit IDs
    logger.log(0x100, 1000, &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    logger.log(0x200, 2000, &[0x11, 0x12, 0x13, 0x14]);
    println!("  - Logged 2 standard 11-bit ID frames (0x100, 0x200)");

    // Log extended 29-bit IDs (common in J1939)
    // PGN 0xFEF1 = Engine Temperature (typical J1939)
    logger.log_extended(0x18FEF100, 3000, &[0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28]);
    // PGN 0xF004 = Electronic Engine Controller 1
    logger.log_extended(0x0CF00400, 4000, &[0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38]);
    // PGN 0xFECA = DM1 Active Diagnostic Trouble Codes
    logger.log_extended(0x18FECA00, 5000, &[0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48]);
    println!("  - Logged 3 extended 29-bit ID frames (J1939 PGNs)");

    assert_eq!(logger.standard_id_count(), 2);
    assert_eq!(logger.extended_id_count(), 3);
    assert!(logger.has_extended_frames());
    println!("  - Standard IDs: {}, Extended IDs: {}",
             logger.standard_id_count(), logger.extended_id_count());

    let mdf_bytes = logger.finalize()?;
    println!("  - MDF4 file size: {} bytes", mdf_bytes.len());

    let temp_path = std::env::temp_dir().join("extended_can_id_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    // Read back and verify
    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    assert_eq!(groups.len(), 5, "Expected 5 channel groups (2 standard + 3 extended)");

    println!("\n{}", "=".repeat(80));
    println!("VERIFYING IDE CHANNEL VALUES");
    println!("{}", "=".repeat(80));

    for group in groups.iter() {
        let group_name = group.name()?.unwrap_or_default();
        let channels = group.channels();

        let mut ide_vals: Vec<u8> = Vec::new();
        let mut can_id_vals: Vec<u32> = Vec::new();

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
                "IDE" => {
                    for v in vals.iter().flatten() {
                        if let DecodedValue::UnsignedInteger(ide) = v {
                            ide_vals.push(*ide as u8);
                        }
                    }
                }
                _ => {}
            }
        }

        if !can_id_vals.is_empty() && !ide_vals.is_empty() {
            let can_id = can_id_vals[0];
            let ide = ide_vals[0];
            let is_extended = ide != 0;

            // The stored CAN_ID has bit 31 set for extended IDs
            let actual_id = can_id & 0x1FFFFFFF;

            println!("  {}: CAN_ID=0x{:08X}, IDE={} ({})",
                     group_name,
                     actual_id,
                     ide,
                     if is_extended { "extended 29-bit" } else { "standard 11-bit" });

            // Verify IDE flag matches the ID type
            if can_id & 0x80000000 != 0 {
                assert_eq!(ide, 1, "Extended ID should have IDE=1");
            } else {
                assert_eq!(ide, 0, "Standard ID should have IDE=0");
            }
        }
    }

    std::fs::remove_file(&temp_path)?;
    println!("\n\nExtended CAN ID integration test PASSED!\n");

    Ok(())
}

/// Test mixed extended CAN IDs with CAN FD
#[test]
fn end_to_end_extended_can_fd() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: Extended CAN FD -> MDF4 -> Read");
    println!("{}\n", "=".repeat(80));

    let mut logger = RawCanLogger::new()?;

    // Standard ID with FD
    let fd_data_16 = [0xAA; 16];
    logger.log_fd(0x100, 1000, &fd_data_16, FdFlags::new(true, false));
    println!("  - Logged standard FD frame (16 bytes) at 0x100");

    // Extended ID with FD (J1939-style)
    let fd_data_32 = [0xBB; 32];
    logger.log_fd_extended(0x18FEF100, 2000, &fd_data_32, FdFlags::new(true, false));
    println!("  - Logged extended FD frame (32 bytes) at 0x18FEF100");

    // Extended ID with FD and ESI
    let fd_data_64 = [0xCC; 64];
    logger.log_fd_extended(0x18FECA00, 3000, &fd_data_64, FdFlags::new(true, true));
    println!("  - Logged extended FD frame (64 bytes, BRS+ESI) at 0x18FECA00");

    assert!(logger.has_fd_frames());
    assert!(logger.has_extended_frames());
    assert_eq!(logger.standard_id_count(), 1);
    assert_eq!(logger.extended_id_count(), 2);

    let mdf_bytes = logger.finalize()?;

    let temp_path = std::env::temp_dir().join("extended_can_fd_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    assert_eq!(groups.len(), 3, "Expected 3 channel groups");

    // Verify each group has IDE and FD_Flags channels
    for group in groups.iter() {
        let channels = group.channels();
        let channel_names: Vec<String> = channels.iter()
            .map(|c| c.name().ok().flatten().unwrap_or_default())
            .collect();

        assert!(channel_names.contains(&"IDE".to_string()), "Missing IDE channel");
        assert!(channel_names.contains(&"FD_Flags".to_string()), "Missing FD_Flags channel");
    }

    std::fs::remove_file(&temp_path)?;
    println!("\nExtended CAN FD integration test PASSED!\n");

    Ok(())
}

/// Test J1939-style PGN addressing
#[test]
fn end_to_end_j1939_pgns() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Test: J1939 PGN Addressing -> MDF4");
    println!("{}\n", "=".repeat(80));

    let mut logger = RawCanLogger::new()?;

    // J1939 CAN ID format: Priority(3) + Reserved(1) + DP(1) + PF(8) + PS(8) + SA(8) = 29 bits
    // Example: 0x18FEF100 = Priority 6, PGN 0xFEF1 (Engine Temperature 1), SA 0x00

    // Engine Temperature 1 (PGN 65265 = 0xFEF1)
    let engine_temp_id = 0x18FEF100; // Priority 6, PGN 0xFEF1, SA 0x00
    logger.log_extended(engine_temp_id, 1000, &[0x7D, 0x7D, 0x7D, 0x7D, 0x7D, 0x7D, 0x7D, 0x7D]);

    // Electronic Engine Controller 1 (PGN 61444 = 0xF004)
    let eec1_id = 0x0CF00400; // Priority 3, PGN 0xF004, SA 0x00
    logger.log_extended(eec1_id, 2000, &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Wheel Speed Information (PGN 65215 = 0xFEBF)
    let wheel_speed_id = 0x18FEBF00;
    logger.log_extended(wheel_speed_id, 3000, &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

    // DM1 Active Diagnostic Trouble Codes (PGN 65226 = 0xFECA)
    let dm1_id = 0x18FECA00;
    logger.log_extended(dm1_id, 4000, &[0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF]);

    println!("  - Logged 4 J1939 messages with extended IDs");
    println!("    - Engine Temperature 1 (PGN 0xFEF1)");
    println!("    - Electronic Engine Controller 1 (PGN 0xF004)");
    println!("    - Wheel Speed Information (PGN 0xFEBF)");
    println!("    - DM1 Active DTCs (PGN 0xFECA)");

    assert_eq!(logger.extended_id_count(), 4);
    assert_eq!(logger.standard_id_count(), 0);

    let mdf_bytes = logger.finalize()?;

    let temp_path = std::env::temp_dir().join("j1939_pgn_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    assert_eq!(groups.len(), 4, "Expected 4 channel groups");

    // All groups should have IDE=1
    for group in groups.iter() {
        let channels = group.channels();

        for channel in channels.iter() {
            let name = channel.name()?.unwrap_or_default();
            if name == "IDE" {
                let vals = channel.values()?;
                for v in vals.iter().flatten() {
                    if let DecodedValue::UnsignedInteger(ide) = v {
                        assert_eq!(*ide, 1, "J1939 frames should have IDE=1");
                    }
                }
            }
        }
    }

    std::fs::remove_file(&temp_path)?;
    println!("\nJ1939 PGN integration test PASSED!\n");

    Ok(())
}
