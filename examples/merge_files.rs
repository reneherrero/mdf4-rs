use mdf4_rs::{DataType, DecodedValue, MDF, MdfWriter, Result, merge::merge_files};

fn main() -> Result<()> {
    let input1 = "merge_input1.mf4";
    let input2 = "merge_input2.mf4";
    let output = "merge_result.mf4";

    for path in [input1, input2, output] {
        let _ = std::fs::remove_file(path);
    }

    // Create first file with a time channel and a value channel
    let mut w1 = MdfWriter::new(input1)?;
    w1.init_mdf_file()?;
    let cg1 = w1.add_channel_group(None, |_| {})?;
    let t1 = w1.add_channel(&cg1, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".into());
        ch.bit_count = 64;
    })?;
    w1.set_time_channel(&t1)?;
    w1.add_channel(&cg1, Some(&t1), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Value".into());
    })?;
    w1.start_data_block_for_cg(&cg1, 0)?;
    for i in 0u64..5 {
        w1.write_record(
            &cg1,
            &[
                DecodedValue::Float(i as f64 * 0.1),
                DecodedValue::UnsignedInteger(i),
            ],
        )?;
    }
    w1.finish_data_block(&cg1)?;
    w1.finalize()?;

    // Create second file with the same channels continuing in time
    let mut w2 = MdfWriter::new(input2)?;
    w2.init_mdf_file()?;
    let cg2 = w2.add_channel_group(None, |_| {})?;
    let t2 = w2.add_channel(&cg2, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Time".into());
        ch.bit_count = 64;
    })?;
    w2.set_time_channel(&t2)?;
    w2.add_channel(&cg2, Some(&t2), |ch| {
        ch.data_type = DataType::UnsignedIntegerLE;
        ch.name = Some("Value".into());
    })?;
    w2.start_data_block_for_cg(&cg2, 0)?;
    for i in 5u64..10 {
        w2.write_record(
            &cg2,
            &[
                DecodedValue::Float(i as f64 * 0.1),
                DecodedValue::UnsignedInteger(i),
            ],
        )?;
    }
    w2.finish_data_block(&cg2)?;
    w2.finalize()?;

    // Merge the two files
    merge_files(output, input1, input2)?;

    // Inspect using the parser API
    let mdf = MDF::from_file(output)?;
    println!(
        "Merged file has {} channel group(s)",
        mdf.channel_groups().len()
    );
    for (g_idx, group) in mdf.channel_groups().iter().enumerate() {
        println!(
            " Group {}: {} channel(s)",
            g_idx + 1,
            group.channels().len()
        );
        for ch in group.channels() {
            println!("  {:?} -> {:?}", ch.name()?, ch.values()?);
        }
    }

    Ok(())
}
