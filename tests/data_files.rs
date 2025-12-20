//! Integration tests for MDF4 data files.

use mdf4_rs::{DataType, DecodedValue, Error, MDF, MdfWriter, Result};
use std::path::Path;

const TEST_DATA_DIR: &str = "tests/data";

fn test_data_path(filename: &str) -> String {
    Path::new(TEST_DATA_DIR)
        .join(filename)
        .to_string_lossy()
        .to_string()
}

// ============================================================================
// Tests for UnFinMF format files (now supported)
// ============================================================================

#[test]
fn unfinmf_11bit_obd2_loads() {
    let path = test_data_path("11-bit-obd2.MF4");
    let result = MDF::from_file(&path);

    assert!(
        result.is_ok(),
        "UnFinMF 11-bit OBD2 file should load successfully"
    );
    let mdf = result.unwrap();
    let groups = mdf.channel_groups();
    assert!(!groups.is_empty(), "Should have at least one channel group");
}

#[test]
fn unfinmf_29bit_obd2_loads() {
    let path = test_data_path("29-bit-obd2.MF4");
    let result = MDF::from_file(&path);

    assert!(
        result.is_ok(),
        "UnFinMF 29-bit OBD2 file should load successfully"
    );
    let mdf = result.unwrap();
    let groups = mdf.channel_groups();
    assert!(!groups.is_empty(), "Should have at least one channel group");
}

#[test]
fn unfinmf_29bit_wwh_obd_loads() {
    let path = test_data_path("29-bit-wwh-obd.MF4");
    let result = MDF::from_file(&path);

    assert!(
        result.is_ok(),
        "UnFinMF 29-bit WWH-OBD file should load successfully"
    );
    let mdf = result.unwrap();
    let groups = mdf.channel_groups();
    assert!(!groups.is_empty(), "Should have at least one channel group");
}

// ============================================================================
// Tests for valid MDF4 files (created by the writer)
// ============================================================================

#[test]
fn roundtrip_single_channel_float() -> Result<()> {
    let path = std::env::temp_dir().join("test_single_float.mf4");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    // Write file with float channel
    let mut writer = MdfWriter::new(path.to_str().unwrap())?;
    writer.init_mdf_file()?;
    let cg_id = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Temperature".into());
        ch.bit_count = 64;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    let values: Vec<f64> = vec![20.5, 21.0, 21.5, 22.0, 22.5];
    for v in &values {
        writer.write_record(&cg_id, &[DecodedValue::Float(*v)])?;
    }
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Read and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let groups = mdf.channel_groups();
    assert_eq!(groups.len(), 1);

    let channels = groups[0].channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].name()?.as_deref(), Some("Temperature"));

    let read_values = channels[0].values()?;
    assert_eq!(read_values.len(), values.len());
    for (i, (expected, actual)) in values.iter().zip(read_values.iter()).enumerate() {
        if let Some(DecodedValue::Float(v)) = actual {
            assert!(
                (*v - expected).abs() < 1e-10,
                "Value mismatch at {i}: expected {expected}, got {v}"
            );
        } else {
            panic!("Expected Float at index {i}, got {:?}", actual);
        }
    }

    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn roundtrip_multiple_channels() -> Result<()> {
    let path = std::env::temp_dir().join("test_multi_channel.mf4");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    // Write file with multiple channels
    let mut writer = MdfWriter::new(path.to_str().unwrap())?;
    writer.init_mdf_file()?;
    let cg_id = writer.add_channel_group(None, |_| {})?;

    let time_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".into());
        ch.bit_count = 64;
    })?;
    writer.set_time_channel(&time_id)?;

    let rpm_id = writer.add_channel(&cg_id, Some(&time_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("RPM".into());
        ch.bit_count = 16;
    })?;

    writer.add_channel(&cg_id, Some(&rpm_id), |ch| {
        ch.data_type = DataType::SignedIntegerLE;
        ch.name = Some("Speed".into());
        ch.bit_count = 16;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    let records = vec![
        (0.0, 1000u64, 0i64),
        (0.1, 1500, 20),
        (0.2, 2000, 40),
        (0.3, 2500, 60),
        (0.4, 3000, 80),
    ];
    for (t, rpm, speed) in &records {
        writer.write_record(
            &cg_id,
            &[
                DecodedValue::Float(*t),
                DecodedValue::UnsignedInteger(*rpm),
                DecodedValue::SignedInteger(*speed),
            ],
        )?;
    }
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Read and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let groups = mdf.channel_groups();
    assert_eq!(groups.len(), 1);

    let channels = groups[0].channels();
    assert_eq!(channels.len(), 3);

    // Verify Time channel
    assert_eq!(channels[0].name()?.as_deref(), Some("Time"));
    let time_values = channels[0].values()?;
    assert_eq!(time_values.len(), 5);

    // Verify RPM channel
    assert_eq!(channels[1].name()?.as_deref(), Some("RPM"));
    let rpm_values = channels[1].values()?;
    assert_eq!(rpm_values.len(), 5);
    if let Some(DecodedValue::UnsignedInteger(v)) = &rpm_values[2] {
        assert_eq!(*v, 2000);
    } else {
        panic!("Expected UnsignedInteger for RPM");
    }

    // Verify Speed channel
    assert_eq!(channels[2].name()?.as_deref(), Some("Speed"));
    let speed_values = channels[2].values()?;
    assert_eq!(speed_values.len(), 5);
    if let Some(DecodedValue::SignedInteger(v)) = &speed_values[4] {
        assert_eq!(*v, 80);
    } else {
        panic!("Expected SignedInteger for Speed");
    }

    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn roundtrip_large_dataset() -> Result<()> {
    let path = std::env::temp_dir().join("test_large_dataset.mf4");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    const NUM_SAMPLES: usize = 10000;

    // Write file with many samples
    let mut writer = MdfWriter::new(path.to_str().unwrap())?;
    writer.init_mdf_file()?;
    let cg_id = writer.add_channel_group(None, |_| {})?;

    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Signal".into());
        ch.bit_count = 64;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    for i in 0..NUM_SAMPLES {
        let value = (i as f64 * 0.1).sin();
        writer.write_record(&cg_id, &[DecodedValue::Float(value)])?;
    }
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Read and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let groups = mdf.channel_groups();
    let channels = groups[0].channels();
    let values = channels[0].values()?;

    assert_eq!(values.len(), NUM_SAMPLES);

    // Spot check some values
    if let Some(DecodedValue::Float(v)) = &values[0] {
        assert!(v.abs() < 1e-10, "First value should be sin(0) = 0");
    }

    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn roundtrip_multiple_channel_groups() -> Result<()> {
    let path = std::env::temp_dir().join("test_multi_cg.mf4");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    // Write file with multiple channel groups
    let mut writer = MdfWriter::new(path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    // First channel group - engine data
    let cg1 = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg1, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("EngineRPM".into());
        ch.bit_count = 16;
    })?;

    writer.start_data_block_for_cg(&cg1, 0)?;
    for rpm in [1000u64, 1500, 2000] {
        writer.write_record(&cg1, &[DecodedValue::UnsignedInteger(rpm)])?;
    }
    writer.finish_data_block(&cg1)?;

    // Second channel group - transmission data
    let cg2 = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg2, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Gear".into());
        ch.bit_count = 8;
    })?;

    writer.start_data_block_for_cg(&cg2, 0)?;
    for gear in [1u64, 2, 3, 4, 5] {
        writer.write_record(&cg2, &[DecodedValue::UnsignedInteger(gear)])?;
    }
    writer.finish_data_block(&cg2)?;

    writer.finalize()?;

    // Read and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let groups = mdf.channel_groups();
    assert_eq!(groups.len(), 2);

    // Verify first group
    let cg1_channels = groups[0].channels();
    assert_eq!(cg1_channels.len(), 1);
    assert_eq!(cg1_channels[0].name()?.as_deref(), Some("EngineRPM"));
    let rpm_values = cg1_channels[0].values()?;
    assert_eq!(rpm_values.len(), 3);

    // Verify second group
    let cg2_channels = groups[1].channels();
    assert_eq!(cg2_channels.len(), 1);
    assert_eq!(cg2_channels[0].name()?.as_deref(), Some("Gear"));
    let gear_values = cg2_channels[0].values()?;
    assert_eq!(gear_values.len(), 5);

    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn roundtrip_all_integer_types() -> Result<()> {
    let path = std::env::temp_dir().join("test_int_types.mf4");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let mut writer = MdfWriter::new(path.to_str().unwrap())?;
    writer.init_mdf_file()?;
    let cg_id = writer.add_channel_group(None, |_| {})?;

    // Add channels with different integer sizes - chain them together
    let ch8 = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("U8".into());
        ch.bit_count = 8;
    })?;

    let ch16 = writer.add_channel(&cg_id, Some(&ch8), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("U16".into());
        ch.bit_count = 16;
    })?;

    let ch32 = writer.add_channel(&cg_id, Some(&ch16), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("U32".into());
        ch.bit_count = 32;
    })?;

    writer.add_channel(&cg_id, Some(&ch32), |ch| {
        ch.data_type = DataType::SignedIntegerLE;
        ch.name = Some("S16".into());
        ch.bit_count = 16;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    writer.write_record(
        &cg_id,
        &[
            DecodedValue::UnsignedInteger(255),
            DecodedValue::UnsignedInteger(65535),
            DecodedValue::UnsignedInteger(0xFFFFFFFF),
            DecodedValue::SignedInteger(-1000),
        ],
    )?;
    writer.write_record(
        &cg_id,
        &[
            DecodedValue::UnsignedInteger(0),
            DecodedValue::UnsignedInteger(0),
            DecodedValue::UnsignedInteger(0),
            DecodedValue::SignedInteger(1000),
        ],
    )?;
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Read and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let channels = mdf.channel_groups()[0].channels();

    // Check max U8
    if let Some(DecodedValue::UnsignedInteger(v)) = &channels[0].values()?[0] {
        assert_eq!(*v, 255);
    }

    // Check max U16
    if let Some(DecodedValue::UnsignedInteger(v)) = &channels[1].values()?[0] {
        assert_eq!(*v, 65535);
    }

    // Check max U32
    if let Some(DecodedValue::UnsignedInteger(v)) = &channels[2].values()?[0] {
        assert_eq!(*v, 0xFFFFFFFF);
    }

    // Check negative S16
    if let Some(DecodedValue::SignedInteger(v)) = &channels[3].values()?[0] {
        assert_eq!(*v, -1000);
    }

    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn test_nonexistent_file() {
    let result = MDF::from_file("nonexistent_file_12345.mf4");
    assert!(result.is_err());
    if let Err(Error::IOError(_)) = result {
        // Expected
    } else {
        panic!("Expected IOError for nonexistent file");
    }
}

#[test]
fn test_empty_channel_group() -> Result<()> {
    let path = std::env::temp_dir().join("test_empty_cg.mf4");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let mut writer = MdfWriter::new(path.to_str().unwrap())?;
    writer.init_mdf_file()?;
    let cg_id = writer.add_channel_group(None, |_| {})?;

    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Empty".into());
        ch.bit_count = 64;
    })?;

    // Start and finish data block without writing any records
    writer.start_data_block_for_cg(&cg_id, 0)?;
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Read and verify empty
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    let channels = mdf.channel_groups()[0].channels();
    let values = channels[0].values()?;
    assert!(values.is_empty());

    std::fs::remove_file(path)?;
    Ok(())
}
