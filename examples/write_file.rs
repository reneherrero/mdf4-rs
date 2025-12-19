use mdf4_rs::{DataType, DecodedValue, MDF, MdfWriter, Result};

fn main() -> Result<()> {
    // Create writer and base structure
    let mut writer = MdfWriter::new("example.mf4")?;
    let (_id, _hd) = writer.init_mdf_file()?;
    // -------- Channel Group 1 with 2 channels --------
    let cg1_id = writer.add_channel_group(None, |_| {})?;
    let cn1_id = writer.add_channel(&cg1_id, None, |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Speed".into());
    })?;

    writer.add_channel(&cg1_id, Some(&cn1_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("RPM".into());
    })?;

    // -------- Channel Group 2 with 3 channels --------
    // A new data group is created automatically
    let cg2_id = writer.add_channel_group(None, |_| {})?;

    let cn3_id = writer.add_channel(&cg2_id, None, |ch| {
        ch.data_type = DataType::SignedIntegerLE;
        ch.name = Some("Temperature".into());
    })?;
    let cn4_id = writer.add_channel(&cg2_id, Some(&cn3_id), |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Pressure".into());
    })?;

    let cn5_id = writer.add_channel(&cg2_id, Some(&cn4_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Status".into());
    })?;
    let status_map = [(0i64, "OK"), (1i64, "WARN")];
    writer.add_value_to_text_conversion(&status_map, "UNKNOWN", Some(&cn5_id))?;

    // -------- Write sample data for both groups --------
    writer.start_data_block_for_cg(&cg1_id, 0)?;
    for i in 0u32..100 {
        writer.write_record(
            &cg1_id,
            &[
                DecodedValue::UnsignedInteger(i.into()),
                DecodedValue::UnsignedInteger((i * 2).into()),
            ],
        )?;
    }
    writer.finish_data_block(&cg1_id)?;

    writer.start_data_block_for_cg(&cg2_id, 0)?;
    for i in 0u32..100 {
        writer.write_record(
            &cg2_id,
            &[
                DecodedValue::SignedInteger(i as i64 - 50),
                DecodedValue::Float(i as f64 * 0.1),
                DecodedValue::UnsignedInteger((i % 2).into()),
            ],
        )?;
    }
    writer.finish_data_block(&cg2_id)?;

    writer.finalize()?;

    // -------- Verify using the crate parser --------
    let mdf = MDF::from_file("example.mf4")?;
    println!("Channel groups: {}", mdf.channel_groups().len());
    for (idx, group) in mdf.channel_groups().iter().enumerate() {
        let chans = group.channels();
        print!("  Group {} has {} channels", idx + 1, chans.len());
        if let Some(ch) = chans.first() {
            let values = ch.values()?;
            println!(" and {} records", values.len());
        } else {
            println!();
        }
    }

    Ok(())
}
