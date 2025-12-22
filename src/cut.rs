use crate::{
    Result,
    blocks::ChannelBlock,
    parsing::{
        MdfFile,
        decoder::{DecodedValue, decode_channel_value},
    },
    writer::MdfWriter,
};

// Helper to fetch the next set of raw records from parallel iterators.
// Returns `Ok(Some(records))` when a full set was read,
// `Ok(None)` if any iterator was exhausted, or an error propagated from
// the iterators.
fn next_record_set<'a, I>(iters: &mut [I]) -> Result<Option<Vec<&'a [u8]>>>
where
    I: Iterator<Item = Result<&'a [u8]>>,
{
    let mut rec = Vec::with_capacity(iters.len());
    for it in iters.iter_mut() {
        match it.next() {
            Some(Ok(r)) => rec.push(r),
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        }
    }
    Ok(Some(rec))
}

/// Cut a segment of an MDF file based on time stamps.
///
/// The input file is scanned for a master time channel (channel type `2` and
/// sync type `1`). Only records whose time value lies in the inclusive range
/// `[start_time, end_time]` are copied to the new file.
///
/// # Arguments
/// * `input_path` - Path to the source MF4 file
/// * `output_path` - Destination path for the trimmed file
/// * `start_time` - Start time of the segment in seconds
/// * `end_time` - End time of the segment in seconds
///
/// # Returns
/// `Ok(())` on success or an [`crate::Error`] if reading or writing fails.
pub fn cut_mdf_by_time(
    input_path: &str,
    output_path: &str,
    start_time: f64,
    end_time: f64,
) -> Result<()> {
    let mdf = MdfFile::parse_from_file(input_path)?;
    let mut writer = MdfWriter::new(output_path)?;
    writer.init_mdf_file()?;

    for dg in &mdf.data_groups {
        let mut prev_cg: Option<String> = None;
        for cg in &dg.channel_groups {
            let cg_id = writer.add_channel_group(prev_cg.as_deref(), |_| {})?;
            prev_cg = Some(cg_id.clone());

            let mut prev_cn: Option<String> = None;
            let mut channel_blocks: Vec<ChannelBlock> = Vec::new();
            for ch in &cg.raw_channels {
                let mut block = ch.block.clone();
                block.resolve_name(&mdf.mmap)?;
                let id = writer.add_channel(&cg_id, prev_cn.as_deref(), |c| {
                    *c = block.clone();
                })?;
                prev_cn = Some(id);
                channel_blocks.push(block);
            }

            // Prepare iterators over raw records for each channel
            let mut iters = Vec::new();
            for ch in &cg.raw_channels {
                iters.push(ch.records(dg, cg, &mdf.mmap)?);
            }

            // Identify the time (master) channel index
            let mut time_idx: Option<usize> = None;
            for (idx, ch) in cg.raw_channels.iter().enumerate() {
                if ch.block.channel_type == 2 && ch.block.sync_type == 1 {
                    time_idx = Some(idx);
                    break;
                }
            }
            let time_idx = match time_idx {
                Some(i) => i,
                None => {
                    // No time channel found; copy all records
                    writer.start_data_block_for_cg(&cg_id, dg.block.record_id_size)?;
                    while let Some(rec) = next_record_set(&mut iters)? {
                        let mut vals = Vec::new();
                        for (slice, ch) in rec.into_iter().zip(channel_blocks.iter()) {
                            let dv =
                                decode_channel_value(slice, dg.block.record_id_size as usize, ch)
                                    .unwrap_or(DecodedValue::Unknown);
                            vals.push(ch.apply_conversion_value(dv, &mdf.mmap)?);
                        }
                        writer.write_record(&cg_id, &vals)?;
                    }
                    writer.finish_data_block(&cg_id)?;
                    continue;
                }
            };

            writer.start_data_block_for_cg(&cg_id, dg.block.record_id_size)?;

            while let Some(rec) = next_record_set(&mut iters)? {
                // Decode time value
                let time_val = {
                    let ch = &channel_blocks[time_idx];
                    let dv =
                        decode_channel_value(rec[time_idx], dg.block.record_id_size as usize, ch)
                            .unwrap_or(DecodedValue::Unknown);
                    match ch.apply_conversion_value(dv, &mdf.mmap)? {
                        DecodedValue::Float(f) => f,
                        DecodedValue::UnsignedInteger(u) => u as f64,
                        DecodedValue::SignedInteger(i) => i as f64,
                        _ => continue,
                    }
                };

                if time_val < start_time {
                    continue;
                }
                if time_val - end_time > f64::EPSILON {
                    break;
                }

                let mut vals = Vec::new();
                for (slice, ch) in rec.into_iter().zip(channel_blocks.iter()) {
                    let dv = decode_channel_value(slice, dg.block.record_id_size as usize, ch)
                        .unwrap_or(DecodedValue::Unknown);
                    vals.push(ch.apply_conversion_value(dv, &mdf.mmap)?);
                }
                writer.write_record(&cg_id, &vals)?;
            }
            writer.finish_data_block(&cg_id)?;
        }
    }

    writer.finalize()
}
