//! Tests for CanDbcLogger (decoded signal logging).

use dbc_rs::Dbc;
use mdf4_rs::can::CanDbcLogger;
use mdf4_rs::{DecodedValue, MDF, Result};

use super::COMPLETE_DBC;

#[test]
fn dbc_logger_basic() -> Result<()> {
    let dbc = Dbc::parse(COMPLETE_DBC).expect("Failed to parse DBC");

    let mut logger = CanDbcLogger::builder(&dbc)
        .store_raw_values(false)
        .include_units(true)
        .build()?;

    // EngineData (256): RPM, Temperature
    let signals = vec![
        ("RPM", 2500.0),
        ("Temperature", 85.0),
        ("ThrottlePosition", 50.0),
        ("OilPressure", 350.0),
    ];
    let payload = dbc.encode(256, &signals, false).expect("Encode failed");
    logger.log(256, 100_000, &payload.as_slice()[..8]);

    // TransmissionData (512): Gear, Torque
    let signals = vec![
        ("GearPosition", 4.0),
        ("ClutchEngaged", 0.0),
        ("Torque", 200.0),
        ("TransmissionTemp", 70.0),
    ];
    let payload = dbc.encode(512, &signals, false).expect("Encode failed");
    logger.log(512, 200_000, &payload.as_slice()[..8]);

    assert_eq!(logger.frame_count(256), 1);
    assert_eq!(logger.frame_count(512), 1);

    let mdf_bytes = logger.finalize()?;
    let temp_path = std::env::temp_dir().join("dbc_logger_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // Should have groups for messages we logged
    assert!(groups.len() >= 2);

    let mut found_engine = false;
    let mut found_transmission = false;

    for group in groups.iter() {
        let name = group.name()?.unwrap_or_default();
        let channels = group.channels();

        if name == "EngineData" {
            found_engine = true;
            for ch in channels.iter() {
                let ch_name = ch.name()?.unwrap_or_default();
                let vals = ch.values()?;
                if !vals.is_empty() {
                    if let Some(DecodedValue::Float(v)) = &vals[0] {
                        match ch_name.as_str() {
                            "RPM" => assert!((*v - 2500.0).abs() < 1.0),
                            "Temperature" => assert!((*v - 85.0).abs() < 1.0),
                            _ => {}
                        }
                    }
                }
            }
        } else if name == "TransmissionData" {
            found_transmission = true;
            for ch in channels.iter() {
                let ch_name = ch.name()?.unwrap_or_default();
                let vals = ch.values()?;
                if !vals.is_empty() {
                    if let Some(DecodedValue::Float(v)) = &vals[0] {
                        match ch_name.as_str() {
                            "GearPosition" => assert!((*v - 4.0).abs() < 0.1),
                            "Torque" => assert!((*v - 200.0).abs() < 1.0),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    assert!(found_engine, "EngineData group not found");
    assert!(found_transmission, "TransmissionData group not found");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

#[test]
fn dbc_logger_with_raw_values() -> Result<()> {
    let dbc = Dbc::parse(COMPLETE_DBC).expect("Failed to parse DBC");

    let mut logger = CanDbcLogger::builder(&dbc)
        .store_raw_values(true)
        .include_conversions(true)
        .build()?;

    let signals = vec![("RPM", 1000.0), ("Temperature", 25.0)];
    let payload = dbc.encode(256, &signals, false).expect("Encode failed");
    logger.log(256, 100_000, &payload.as_slice()[..8]);

    let mdf_bytes = logger.finalize()?;
    let temp_path = std::env::temp_dir().join("dbc_raw_test.mf4");
    std::fs::write(&temp_path, &mdf_bytes)?;

    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;
    let groups = mdf.channel_groups();

    // With raw values, there should be at least one group with channels
    assert!(!groups.is_empty());
    let channels = groups[0].channels();
    assert!(!channels.is_empty());

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

#[test]
fn dbc_encode_decode_accuracy() -> Result<()> {
    let dbc = Dbc::parse(COMPLETE_DBC).expect("Failed to parse DBC");

    // Test values within signal ranges
    let test_cases = [
        (
            256,
            vec![
                ("RPM", 3000.0),
                ("Temperature", -10.0),
                ("OilPressure", 500.0),
            ],
        ),
        (512, vec![("Torque", -150.0), ("TransmissionTemp", 80.0)]),
        (768, vec![("BrakePressure", 100.0)]),
        (1024, vec![("Voltage", 14.5), ("Humidity", 60.0)]),
    ];

    for (can_id, signals) in &test_cases {
        let payload = dbc.encode(*can_id, signals, false).expect("Encode failed");
        let dlc = dbc
            .messages()
            .iter()
            .find(|m| m.id() == *can_id)
            .map(|m| m.dlc())
            .unwrap_or(8);
        let decoded = dbc
            .decode(*can_id, &payload.as_slice()[..dlc as usize], false)
            .expect("Decode failed");

        for (name, expected) in signals {
            if let Some(sig) = decoded.iter().find(|s| s.name == *name) {
                let error = (sig.value - expected).abs();
                let tolerance = expected.abs() * 0.02 + 1.0; // 2% + 1.0 for quantization
                assert!(
                    error <= tolerance,
                    "Signal {} error {} exceeds tolerance {}",
                    name,
                    error,
                    tolerance
                );
            }
        }
    }

    Ok(())
}
