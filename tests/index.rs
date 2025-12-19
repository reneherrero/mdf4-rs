use mdf4_rs::{
    BufferedRangeReader, DataType, DecodedValue, FileRangeReader, MDF, MdfIndex, MdfWriter, Result,
};
use std::fs;

#[test]
fn test_index_roundtrip() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("index_test.mf4");
    let index_path = std::env::temp_dir().join("index_test.json");

    if mdf_path.exists() {
        fs::remove_file(&mdf_path)?;
    }
    if index_path.exists() {
        fs::remove_file(&index_path)?;
    }

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;

    let time_ch_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".to_string());
        ch.bit_count = 64;
    })?;
    writer.set_time_channel(&time_ch_id)?;

    writer.add_channel(&cg_id, Some(&time_ch_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Value".to_string());
        ch.bit_count = 32;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;

    let test_values = vec![(0.0, 100u64), (0.1, 200u64), (0.2, 300u64)];

    for (time, value) in &test_values {
        writer.write_record(
            &cg_id,
            &[
                DecodedValue::Float(*time),
                DecodedValue::UnsignedInteger(*value),
            ],
        )?;
    }

    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;
    index.save_to_file(index_path.to_str().unwrap())?;

    let loaded_index = MdfIndex::load_from_file(index_path.to_str().unwrap())?;

    assert_eq!(loaded_index.channel_groups.len(), 1);

    let group = &loaded_index.channel_groups[0];
    assert_eq!(group.channels.len(), 2);
    assert_eq!(group.record_count, test_values.len() as u64);

    let time_channel = &group.channels[0];
    assert_eq!(time_channel.name, Some("Time".to_string()));
    assert_eq!(time_channel.data_type, DataType::FloatLE);
    assert_eq!(time_channel.channel_type, 2);

    let value_channel = &group.channels[1];
    assert_eq!(value_channel.name, Some("Value".to_string()));
    assert_eq!(value_channel.data_type, DataType::UnsignedIntegerLE);
    assert_eq!(value_channel.channel_type, 0);

    assert!(!group.data_blocks.is_empty());
    let data_block = &group.data_blocks[0];
    assert!(!data_block.is_compressed);
    assert!(data_block.size > 0);

    let mut reader = FileRangeReader::new(mdf_path.to_str().unwrap())?;
    let time_values = loaded_index.read_channel_values(0, 0, &mut reader)?;
    let value_values = loaded_index.read_channel_values(0, 1, &mut reader)?;

    assert_eq!(time_values.len(), test_values.len());
    assert_eq!(value_values.len(), test_values.len());

    for (i, (expected_time, expected_value)) in test_values.iter().enumerate() {
        if let Some(DecodedValue::Float(actual_time)) = time_values[i] {
            assert!((actual_time - expected_time).abs() < 1e-10);
        } else {
            panic!("Expected Float value for time channel");
        }

        if let Some(DecodedValue::UnsignedInteger(actual_value)) = value_values[i] {
            assert_eq!(actual_value, *expected_value);
        } else {
            panic!("Expected UnsignedInteger value for value channel");
        }
    }

    fs::remove_file(mdf_path)?;
    fs::remove_file(index_path)?;

    Ok(())
}

#[test]
fn test_index_vs_direct_read() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("comparison_test.mf4");
    let index_path = std::env::temp_dir().join("comparison_index.json");

    let _ = fs::remove_file(&mdf_path);
    let _ = fs::remove_file(&index_path);

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("TestChannel".to_string());
        ch.bit_count = 32;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;

    let test_data = vec![42u64, 123u64, 456u64, 789u64];
    for &value in &test_data {
        writer.write_record(&cg_id, &[DecodedValue::UnsignedInteger(value)])?;
    }

    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    let mdf = MDF::from_file(mdf_path.to_str().unwrap())?;
    let direct_values = mdf.channel_groups()[0].channels()[0].values()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;
    let mut reader = FileRangeReader::new(mdf_path.to_str().unwrap())?;
    let indexed_values = index.read_channel_values(0, 0, &mut reader)?;

    assert_eq!(direct_values.len(), indexed_values.len());
    assert_eq!(direct_values.len(), test_data.len());

    for i in 0..test_data.len() {
        assert_eq!(direct_values[i], indexed_values[i]);

        if let Some(DecodedValue::UnsignedInteger(value)) = indexed_values[i] {
            assert_eq!(value, test_data[i]);
        } else {
            panic!("Expected UnsignedInteger");
        }
    }

    let _ = fs::remove_file(mdf_path);
    let _ = fs::remove_file(index_path);

    Ok(())
}

#[test]
fn test_index_metadata() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("metadata_test.mf4");

    if mdf_path.exists() {
        fs::remove_file(&mdf_path)?;
    }

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;
    let float_ch_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("TestFloat".to_string());
        ch.bit_count = 32;
    })?;

    writer.add_channel(&cg_id, Some(&float_ch_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("TestInt".to_string());
        ch.bit_count = 16;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    writer.write_record(
        &cg_id,
        &[DecodedValue::Float(3.14), DecodedValue::UnsignedInteger(42)],
    )?;
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;

    let groups = index.list_channel_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].1, "<unnamed>");
    assert_eq!(groups[0].2, 2);

    let channels = index.list_channels(0).unwrap();
    assert_eq!(channels.len(), 2);

    assert_eq!(channels[0].1, "TestFloat");
    assert_eq!(channels[0].2, &DataType::FloatLE);

    assert_eq!(channels[1].1, "TestInt");
    assert_eq!(channels[1].2, &DataType::UnsignedIntegerLE);

    let float_info = index.get_channel_info(0, 0).unwrap();
    assert_eq!(float_info.name, Some("TestFloat".to_string()));
    assert_eq!(float_info.bit_count, 32);

    let int_info = index.get_channel_info(0, 1).unwrap();
    assert_eq!(int_info.name, Some("TestInt".to_string()));
    assert_eq!(int_info.bit_count, 16);

    fs::remove_file(mdf_path)?;
    Ok(())
}

#[test]
fn test_byte_ranges() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("byte_ranges_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;

    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Ch1".to_string());
        ch.bit_count = 32;
    })?;

    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Ch2".to_string());
        ch.bit_count = 16;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;

    for i in 0..5 {
        writer.write_record(
            &cg_id,
            &[
                DecodedValue::UnsignedInteger(i * 100),
                DecodedValue::UnsignedInteger(i * 10),
            ],
        )?;
    }

    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;

    let ranges = index.get_channel_byte_ranges(0, 0)?;
    assert!(!ranges.is_empty(), "Should have at least one byte range");

    let (total_bytes, range_count) = index.get_channel_byte_summary(0, 0)?;
    assert!(total_bytes > 0, "Should have positive total bytes");
    assert_eq!(range_count, ranges.len(), "Range count should match");

    let partial_ranges = index.get_channel_byte_ranges_for_records(0, 0, 1, 3)?;
    assert!(
        !partial_ranges.is_empty(),
        "Should have byte ranges for partial records"
    );

    let partial_total: u64 = partial_ranges.iter().map(|(_, len)| len).sum();
    assert!(
        partial_total <= total_bytes,
        "Partial range should be <= total range"
    );

    assert!(
        index.get_channel_byte_ranges(99, 0).is_err(),
        "Invalid group index should error"
    );
    assert!(
        index.get_channel_byte_ranges(0, 99).is_err(),
        "Invalid channel index should error"
    );
    assert!(
        index
            .get_channel_byte_ranges_for_records(0, 0, 10, 1)
            .is_err(),
        "Out of range records should error"
    );

    let _ = fs::remove_file(mdf_path);
    Ok(())
}

#[test]
fn test_byte_ranges_accuracy() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("byte_accuracy_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.bit_count = 32;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    writer.write_record(&cg_id, &[DecodedValue::UnsignedInteger(0x12345678)])?;
    writer.write_record(&cg_id, &[DecodedValue::UnsignedInteger(0xABCDEF00)])?;
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;
    let mut reader = FileRangeReader::new(mdf_path.to_str().unwrap())?;
    let direct_values = index.read_channel_values(0, 0, &mut reader)?;

    assert_eq!(direct_values.len(), 2);
    if let Some(DecodedValue::UnsignedInteger(val)) = direct_values[0] {
        assert_eq!(val, 0x12345678);
    } else {
        panic!("Expected UnsignedInteger");
    }

    if let Some(DecodedValue::UnsignedInteger(val)) = direct_values[1] {
        assert_eq!(val, 0xABCDEF00);
    } else {
        panic!("Expected UnsignedInteger");
    }

    let _ranges = index.get_channel_byte_ranges(0, 0)?;
    let (total_bytes, _) = index.get_channel_byte_summary(0, 0)?;

    assert!(
        total_bytes >= 8,
        "Should have at least 8 bytes for 2x32-bit values"
    );

    let _ = fs::remove_file(mdf_path);
    Ok(())
}

#[test]
fn test_name_based_lookup() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("name_lookup_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;

    let temp_ch_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Temperature".to_string());
        ch.bit_count = 32;
    })?;
    writer.set_time_channel(&temp_ch_id)?;

    writer.add_channel(&cg_id, Some(&temp_ch_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Speed".to_string());
        ch.bit_count = 16;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    writer.write_record(
        &cg_id,
        &[
            DecodedValue::Float(25.5),
            DecodedValue::UnsignedInteger(120),
        ],
    )?;
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;

    assert_eq!(
        index.find_channel_by_name_global("Temperature"),
        Some((0, 0))
    );
    assert_eq!(index.find_channel_by_name_global("Speed"), Some((0, 1)));
    assert_eq!(index.find_channel_by_name_global("NonExistent"), None);

    assert_eq!(index.find_channel_by_name(0, "Temperature"), Some(0));
    assert_eq!(index.find_channel_by_name(0, "Speed"), Some(1));
    assert_eq!(index.find_channel_by_name(0, "NonExistent"), None);
    assert_eq!(index.find_channel_by_name(99, "Temperature"), None);

    let (group_idx, channel_idx, channel_info) =
        index.get_channel_info_by_name("Temperature").unwrap();
    assert_eq!(group_idx, 0);
    assert_eq!(channel_idx, 0);
    assert_eq!(channel_info.name, Some("Temperature".to_string()));
    assert_eq!(channel_info.data_type, DataType::FloatLE);

    let mut reader = FileRangeReader::new(mdf_path.to_str().unwrap())?;
    let temp_values = index.read_channel_values_by_name("Temperature", &mut reader)?;
    assert_eq!(temp_values.len(), 1);
    if let Some(DecodedValue::Float(temp)) = temp_values[0] {
        assert!((temp - 25.5).abs() < 0.001);
    } else {
        panic!("Expected Float value");
    }

    let speed_values = index.read_channel_values_by_name("Speed", &mut reader)?;
    assert_eq!(speed_values.len(), 1);
    if let Some(DecodedValue::UnsignedInteger(speed)) = speed_values[0] {
        assert_eq!(speed, 120);
    } else {
        panic!("Expected UnsignedInteger value");
    }

    assert!(
        index
            .read_channel_values_by_name("NonExistent", &mut reader)
            .is_err()
    );

    let ranges = index.get_channel_byte_ranges_by_name("Temperature")?;
    assert!(!ranges.is_empty());

    let _ = fs::remove_file(mdf_path);
    Ok(())
}

#[test]
fn test_multiple_channels_same_name() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("duplicate_names_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg1_id = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg1_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Temperature".to_string());
        ch.bit_count = 32;
    })?;

    let cg2_id = writer.add_channel_group(Some(&cg1_id), |_| {})?;
    writer.add_channel(&cg2_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Temperature".to_string());
        ch.bit_count = 32;
    })?;

    writer.start_data_block_for_cg(&cg1_id, 0)?;
    writer.write_record(&cg1_id, &[DecodedValue::Float(25.0)])?;
    writer.finish_data_block(&cg1_id)?;

    writer.start_data_block_for_cg(&cg2_id, 0)?;
    writer.write_record(&cg2_id, &[DecodedValue::Float(30.0)])?;
    writer.finish_data_block(&cg2_id)?;

    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;

    let all_temp_channels = index.find_all_channels_by_name("Temperature");
    assert_eq!(all_temp_channels.len(), 2);
    assert!(all_temp_channels.contains(&(0, 0)));
    assert!(all_temp_channels.contains(&(1, 0)));

    assert_eq!(
        index.find_channel_by_name_global("Temperature"),
        Some((0, 0))
    );

    let _ = fs::remove_file(mdf_path);
    Ok(())
}

#[test]
fn test_channel_group_name_lookup() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("group_name_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let _cg1_id = writer.add_channel_group(None, |_| {})?;
    writer.finalize()?;

    let index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;

    assert_eq!(index.find_channel_group_by_name("SomeGroup"), None);

    let _ = fs::remove_file(mdf_path);
    Ok(())
}

/// Test streaming index creation (minimal memory usage)
#[test]
fn test_streaming_index_creation() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("streaming_index_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    // Create a test file with multiple channels
    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;

    let time_ch_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".to_string());
        ch.bit_count = 64;
    })?;
    writer.set_time_channel(&time_ch_id)?;

    writer.add_channel(&cg_id, Some(&time_ch_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Sensor1".to_string());
        ch.bit_count = 32;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;

    for i in 0..100 {
        writer.write_record(
            &cg_id,
            &[
                DecodedValue::Float(i as f64 * 0.01),
                DecodedValue::UnsignedInteger(i * 10),
            ],
        )?;
    }

    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Create index using streaming (minimal memory)
    let streaming_index = MdfIndex::from_file_streaming(mdf_path.to_str().unwrap())?;

    // Create index using normal method (for comparison)
    let normal_index = MdfIndex::from_file(mdf_path.to_str().unwrap())?;

    // Both should produce equivalent indexes
    assert_eq!(
        streaming_index.channel_groups.len(),
        normal_index.channel_groups.len()
    );
    assert_eq!(streaming_index.file_size, normal_index.file_size);

    let streaming_group = &streaming_index.channel_groups[0];
    let normal_group = &normal_index.channel_groups[0];

    assert_eq!(streaming_group.channels.len(), normal_group.channels.len());
    assert_eq!(streaming_group.record_count, normal_group.record_count);
    assert_eq!(streaming_group.record_size, normal_group.record_size);

    // Verify channel metadata matches
    for (s_ch, n_ch) in streaming_group
        .channels
        .iter()
        .zip(normal_group.channels.iter())
    {
        assert_eq!(s_ch.name, n_ch.name);
        assert_eq!(s_ch.data_type, n_ch.data_type);
        assert_eq!(s_ch.bit_count, n_ch.bit_count);
        assert_eq!(s_ch.byte_offset, n_ch.byte_offset);
    }

    // Both should produce same values when reading
    let mut reader = BufferedRangeReader::new(mdf_path.to_str().unwrap())?;
    let streaming_values = streaming_index.read_channel_values(0, 1, &mut reader)?;

    let mut reader2 = FileRangeReader::new(mdf_path.to_str().unwrap())?;
    let normal_values = normal_index.read_channel_values(0, 1, &mut reader2)?;

    assert_eq!(streaming_values.len(), normal_values.len());
    assert_eq!(streaming_values.len(), 100);

    for (s_val, n_val) in streaming_values.iter().zip(normal_values.iter()) {
        assert_eq!(s_val, n_val);
    }

    let _ = fs::remove_file(mdf_path);
    Ok(())
}

/// Test BufferedRangeReader performance characteristics
#[test]
fn test_buffered_reader() -> Result<()> {
    let mdf_path = std::env::temp_dir().join("buffered_reader_test.mf4");

    let _ = fs::remove_file(&mdf_path);

    // Create test file
    let mut writer = MdfWriter::new(mdf_path.to_str().unwrap())?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;
    writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Counter".to_string());
        ch.bit_count = 32;
    })?;

    writer.start_data_block_for_cg(&cg_id, 0)?;
    for i in 0..1000 {
        writer.write_record(&cg_id, &[DecodedValue::UnsignedInteger(i)])?;
    }
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // Create index using buffered reader
    let index = MdfIndex::from_file_streaming(mdf_path.to_str().unwrap())?;

    // Read values using buffered reader
    let mut reader = BufferedRangeReader::new(mdf_path.to_str().unwrap())?;
    let values = index.read_channel_values(0, 0, &mut reader)?;

    assert_eq!(values.len(), 1000);

    // Verify values
    for (i, val) in values.iter().enumerate() {
        if let Some(DecodedValue::UnsignedInteger(v)) = val {
            assert_eq!(*v, i as u64);
        } else {
            panic!("Expected UnsignedInteger at index {}", i);
        }
    }

    let _ = fs::remove_file(mdf_path);
    Ok(())
}
