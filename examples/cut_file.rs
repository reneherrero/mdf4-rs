use mdf4_rs::{DataType, DecodedValue, MDF, MdfWriter, Result, cut::cut_mdf_by_time};

fn main() -> Result<()> {
    let input = "cut_example_input.mf4";
    let output = "cut_example_output.mf4";

    // create a simple MF4 file with a time channel and a value channel
    let mut writer = MdfWriter::new(input)?;
    writer.init_mdf_file()?;
    let cg_id = writer.add_channel_group(None, |_| {})?;
    let time_id = writer.add_channel(&cg_id, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".into());
        ch.bit_count = 64;
    })?;
    writer.set_time_channel(&time_id)?;
    writer.add_channel(&cg_id, Some(&time_id), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Val".into());
        ch.bit_count = 32;
    })?;
    writer.start_data_block_for_cg(&cg_id, 0)?;
    for i in 0u64..10 {
        writer.write_record(
            &cg_id,
            &[
                DecodedValue::Float(i as f64 * 0.1),
                DecodedValue::UnsignedInteger(i),
            ],
        )?;
    }
    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    // cut between 0.3 and 0.6 seconds
    cut_mdf_by_time(input, output, 0.3, 0.6)?;

    // Inspect the cut file and print channel names
    let mdf = MDF::from_file(output)?;
    for (g_idx, group) in mdf.channel_groups().iter().enumerate() {
        let channels = group.channels();
        for (c_idx, ch) in channels.iter().enumerate() {
            if let Some(name) = ch.name()? {
                println!("Group {} Channel {}: {}", g_idx + 1, c_idx + 1, name);
            }
        }
    }

    println!("Created {} and {}", input, output);
    Ok(())
}
