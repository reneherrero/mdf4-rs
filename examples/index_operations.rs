use mdf4_rs::{
    DataType, DecodedValue, MdfWriter, Result,
    index::{FileRangeReader, MdfIndex},
};

fn main() -> Result<()> {
    let mdf_file = "index_example.mf4";
    let index_file = "index_example.json";

    println!("=== Index Operations Example ===");

    // Step 1: Create an MDF file with some test data
    println!("Creating MDF file with test data...");
    create_test_mdf_file(mdf_file)?;

    // Step 2: Create enhanced index that resolves all conversion dependencies
    println!("Creating enhanced index with resolved conversions...");
    let index = MdfIndex::from_file(mdf_file)?;
    index.save_to_file(index_file)?;

    println!("Index created and saved to '{}'", index_file);

    // Step 3: Load the index and demonstrate self-contained conversion capability
    println!("Loading index and testing self-contained conversions...");
    let loaded_index = MdfIndex::load_from_file(index_file)?;

    // Step 4: Read and convert data using only the index (no file access for conversions!)
    println!("Reading channel values via enhanced index...");
    let mut reader = FileRangeReader::new(mdf_file)?;

    // List available channels
    println!("\nAvailable channels:");
    if let Some(channels) = loaded_index.list_channels(0) {
        for (idx, name, data_type) in channels {
            println!("  Channel {}: {} ({:?})", idx, name, data_type);
        }
    }

    // Read channel data
    let values = loaded_index.read_channel_values(0, 0, &mut reader)?;

    println!("\nChannel values read from enhanced index:");
    for (i, value) in values.iter().enumerate().take(10) {
        println!("  Record {}: {:?}", i, value);
    }
    if values.len() > 10 {
        println!("  ... and {} more records", values.len() - 10);
    }

    // Step 5: Demonstrate byte range efficiency
    println!("\nByte Range Analysis (Partial Reading):");
    let byte_ranges = loaded_index.get_channel_byte_ranges(0, 0)?;
    let (total_bytes, range_count) = loaded_index.get_channel_byte_summary(0, 0)?;

    println!("  Total bytes needed: {}", total_bytes);
    println!("  Number of byte ranges: {}", range_count);
    println!("  Ranges: {:?}", byte_ranges);

    // Step 6: Compare with name-based access
    println!("\nTesting name-based access:");
    if let Some(channels) = loaded_index.list_channels(0) {
        if let Some((_, channel_name, _)) = channels.first() {
            let values_by_name =
                loaded_index.read_channel_values_by_name(channel_name, &mut reader)?;
            println!(
                "  Read {} values for channel '{}'",
                values_by_name.len(),
                channel_name
            );

            // Verify consistency
            if values == values_by_name {
                println!("  Index-based and name-based access produce identical results");
            } else {
                println!("  Mismatch between access methods");
            }
        }
    }

    // Step 7: Show resolved conversion information
    println!("\nConversion Resolution Status:");
    let group = &loaded_index.channel_groups[0];
    for (i, channel) in group.channels.iter().enumerate() {
        println!(
            "  Channel {}: {}",
            i,
            channel.name.as_deref().unwrap_or("<unnamed>")
        );
        if let Some(conversion) = &channel.conversion {
            println!("    Conversion type: {:?}", conversion.cc_type);
            if let Some(resolved_texts) = &conversion.resolved_texts {
                println!("    Resolved texts: {} entries", resolved_texts.len());
            }
            if let Some(resolved_conversions) = &conversion.resolved_conversions {
                println!(
                    "    Resolved nested conversions: {} entries",
                    resolved_conversions.len()
                );
            }
        } else {
            println!("    No conversion block");
        }
    }

    println!("\nEnhanced index example completed successfully!");

    // Clean up
    std::fs::remove_file(mdf_file).ok();
    std::fs::remove_file(index_file).ok();

    Ok(())
}

fn create_test_mdf_file(path: &str) -> Result<()> {
    let mut writer = MdfWriter::new(path)?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;

    // Create a time channel (master)
    let time_ch_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".to_string());
        ch.bit_count = 64;
    })?;
    writer.set_time_channel(&time_ch_id)?;

    // Create a temperature channel with linear conversion
    writer.add_channel(&cg_id, Some(&time_ch_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Temperature".to_string());
        ch.bit_count = 16;
    })?;

    // Create a status channel (could be enhanced with text conversion later)
    writer.add_channel(&cg_id, Some(&time_ch_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Status".to_string());
        ch.bit_count = 8;
    })?;

    // Write sample data
    writer.start_data_block_for_cg(&cg_id, 0)?;

    for i in 0..20 {
        let time = i as f64 * 0.1;
        let temp_raw = (i * 10 + 200) as u64; // Raw ADC value
        let status = if i % 5 == 0 { 1u64 } else { 0u64 };

        writer.write_record(
            &cg_id,
            &[
                DecodedValue::Float(time),
                DecodedValue::UnsignedInteger(temp_raw),
                DecodedValue::UnsignedInteger(status),
            ],
        )?;
    }

    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    Ok(())
}
