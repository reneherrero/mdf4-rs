//! Tests for ASAM MDF4 Bus Logging CAN_DataFrame format.

use mdf4_rs::can::FdFlags;
use mdf4_rs::can::RawCanLogger;
use mdf4_rs::{DecodedValue, MDF, Result};

/// Parse CAN_DataFrame ByteArray: ID(4) + DLC(1) + Data(N)
fn parse_dataframe(bytes: &[u8]) -> Option<(u32, bool, u8, Vec<u8>)> {
    if bytes.len() < 5 {
        return None;
    }
    let raw_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let is_extended = (raw_id & 0x8000_0000) != 0;
    let can_id = raw_id & 0x1FFF_FFFF;
    let dlc = bytes[4];
    let data = bytes[5..].to_vec();
    Some((can_id, is_extended, dlc, data))
}

#[test]
fn asam_classic_can_format() -> Result<()> {
    let mut logger = RawCanLogger::new()?;

    logger.log(
        0x100,
        100_000,
        &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
    );
    logger.log(0x200, 200_000, &[0xAA, 0xBB, 0xCC, 0xDD]);
    logger.log_extended(
        0x18FEF100,
        300_000,
        &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
    );

    let mdf_bytes = logger.finalize()?;
    let temp_path = std::env::temp_dir().join("asam_classic_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // Should have 2 groups: CAN_DataFrame (standard) and CAN_DataFrame_IDE (extended)
    assert_eq!(groups.len(), 2);

    let mut found_standard = false;
    let mut found_extended = false;

    for group in groups.iter() {
        let name = group.name()?.unwrap_or_default();
        let channels = group.channels();

        // Find Timestamp and CAN_DataFrame channels
        let mut ts_vals = Vec::new();
        let mut df_vals = Vec::new();

        for ch in channels.iter() {
            let ch_name = ch.name()?.unwrap_or_default();
            match ch_name.as_str() {
                "Timestamp" => ts_vals = ch.values()?,
                "CAN_DataFrame" => df_vals = ch.values()?,
                _ => {}
            }
        }

        for (ts, df) in ts_vals.iter().zip(df_vals.iter()) {
            if let (Some(DecodedValue::Float(timestamp)), Some(DecodedValue::ByteArray(bytes))) =
                (ts, df)
            {
                let (can_id, is_extended, _dlc, _data) = parse_dataframe(bytes).unwrap();

                if name.contains("IDE") {
                    assert!(is_extended);
                    assert_eq!(can_id, 0x18FEF100);
                    found_extended = true;
                } else {
                    assert!(!is_extended);
                    assert!(can_id == 0x100 || can_id == 0x200);
                    found_standard = true;
                }

                // Timestamp should be in seconds
                assert!(*timestamp < 1.0, "Timestamp should be < 1 second");
            }
        }
    }

    assert!(found_standard && found_extended);
    std::fs::remove_file(&temp_path)?;
    Ok(())
}

#[test]
fn asam_can_fd_format() -> Result<()> {
    let mut logger = RawCanLogger::new()?;

    // Classic CAN (8 bytes)
    logger.log(0x100, 100_000, &[0x11; 8]);

    // CAN FD small (8 bytes with BRS)
    logger.log_fd(0x200, 200_000, &[0x22; 8], FdFlags::new(true, false));

    // CAN FD large (32 bytes with BRS+ESI)
    logger.log_fd(0x300, 300_000, &[0x33; 32], FdFlags::new(true, true));

    // Extended ID with FD
    logger.log_fd_extended(0x18FEF100, 400_000, &[0x44; 24], FdFlags::new(true, false));

    assert!(logger.has_fd_frames());
    assert!(logger.has_extended_frames());

    let mdf_bytes = logger.finalize()?;
    let temp_path = std::env::temp_dir().join("asam_fd_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // Groups: CAN_DataFrame, CAN_DataFrame_FD, CAN_DataFrame_FD_DLC_over_8, CAN_DataFrame_FD_IDE_DLC_over_8
    assert!(groups.len() >= 3);

    for group in groups.iter() {
        let name = group.name()?.unwrap_or_default();
        let channels = group.channels();

        for ch in channels.iter() {
            let ch_name = ch.name()?.unwrap_or_default();
            if ch_name == "CAN_DataFrame" {
                let vals = ch.values()?;
                for v in vals.iter().flatten() {
                    if let DecodedValue::ByteArray(bytes) = v {
                        let (_can_id, is_extended, _dlc, data) = parse_dataframe(bytes).unwrap();

                        // Verify based on group type
                        if name.contains("DLC_over_8") {
                            assert!(data.len() > 8);
                        }
                        if name.contains("IDE") {
                            assert!(is_extended);
                        }
                    }
                }
            }
        }
    }

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

// Note: DbcOverlayReader needs to be updated to support ASAM CAN_DataFrame format.
// For now, use CanDbcLogger for decoded logging, or decode CAN_DataFrame manually.

#[test]
fn asam_source_metadata() -> Result<()> {
    let logger = RawCanLogger::with_bus_name("Vehicle_CAN")?;
    assert_eq!(logger.total_frame_count(), 0);

    let mut logger = RawCanLogger::with_bus_name("CAN1")?;
    logger.log(0x100, 1000, &[0x01, 0x02]);

    let mdf_bytes = logger.finalize()?;
    let temp_path = std::env::temp_dir().join("asam_source_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // Group name should contain bus name
    let name = groups[0].name()?.unwrap_or_default();
    assert!(
        name.starts_with("CAN1"),
        "Group name should start with bus name"
    );

    // Source info should be present
    if let Ok(Some(source)) = groups[0].source() {
        assert!(source.name.is_some() || source.path.is_some());
    }

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

#[test]
fn asam_j1939_extended_ids() -> Result<()> {
    let mut logger = RawCanLogger::new()?;

    // J1939 PGNs (29-bit extended IDs)
    let j1939_ids = [
        (0x18FEF100, "Engine Temperature 1"),
        (0x0CF00400, "Electronic Engine Controller 1"),
        (0x18FEBF00, "Wheel Speed Information"),
        (0x18FECA00, "DM1 Active DTCs"),
    ];

    for (idx, (can_id, _name)) in j1939_ids.iter().enumerate() {
        logger.log_extended(*can_id, (idx as u64 + 1) * 100_000, &[0x7D; 8]);
    }

    assert_eq!(logger.extended_frame_count(), 4);
    assert_eq!(logger.standard_frame_count(), 0);

    let mdf_bytes = logger.finalize()?;
    let temp_path = std::env::temp_dir().join("asam_j1939_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // All J1939 frames go into CAN_DataFrame_IDE group
    assert_eq!(groups.len(), 1);
    let name = groups[0].name()?.unwrap_or_default();
    assert!(name.contains("IDE"), "J1939 should use extended ID group");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}
