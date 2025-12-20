use super::{RawChannelGroup, RawDataGroup};
use crate::{
    Error, Result,
    blocks::{BlockParse, ChannelBlock, DataListBlock, SignalDataBlock},
};

/// A channel with lazy access to its raw record bytes (fixed-length or VLSD).
#[derive(Debug)]
pub struct RawChannel {
    pub block: ChannelBlock,
}

impl<'a> RawChannel {
    /// Return an iterator over raw record bytes for this channel.
    ///
    /// The iterator yields a `Result` for each record and transparently handles
    /// both fixed-size and VLSD storage schemes.
    ///
    /// # Arguments
    /// * `data_group` - Parent data group owning the records
    /// * `channel_group` - Channel group this channel belongs to
    /// * `mmap` - Memory mapped MDF data
    ///
    /// # Returns
    /// An iterator over byte slices containing each raw record, or an
    /// [`Error`] if the underlying blocks could not be parsed.
    pub fn records(
        &self,
        data_group: &'a RawDataGroup,
        channel_group: &'a RawChannelGroup,
        mmap: &'a [u8],
    ) -> Result<Box<dyn Iterator<Item = Result<&'a [u8]>> + 'a>> {
        // 1) VLSD path: channel has its own data pointer => SD/DL chain
        if self.block.channel_type == 1 && self.block.data != 0 {
            // Capture the file bytes and channel pointer
            let bytes = mmap;
            let mut next_addr = self.block.data;
            let mut data_links = Vec::new();
            let mut link_idx = 0;
            let mut current_sdb: Option<SignalDataBlock> = None;
            let mut sdb_pos = 0;

            // Build a from_fn iterator carrying that mutable state
            let vlsd_iter = std::iter::from_fn(move || -> Option<Result<&'a [u8]>> {
                loop {
                    // 1) Yield from an open SDBLOCK if any
                    if let Some(sdb) = &current_sdb {
                        let buf = sdb.data;
                        if sdb_pos + 4 <= buf.len() {
                            let len =
                                u32::from_le_bytes(buf[sdb_pos..sdb_pos + 4].try_into().unwrap())
                                    as usize;
                            let start = sdb_pos + 4;
                            let end = start + len;
                            if end > buf.len() {
                                return Some(Err(Error::TooShortBuffer {
                                    actual: buf.len(),
                                    expected: end,
                                    file: file!(),
                                    line: line!(),
                                }));
                            }
                            let slice = &buf[start..end];
                            sdb_pos = end;
                            return Some(Ok(slice));
                        }
                        // exhausted
                        current_sdb = None;
                    }

                    // 2) Next link in current DL batch?
                    if link_idx < data_links.len() {
                        let frag_addr = data_links[link_idx];
                        link_idx += 1;
                        let off = frag_addr as usize;
                        match SignalDataBlock::from_bytes(&bytes[off..]) {
                            Ok(sdb) => {
                                // Prepare to yield from it on the next loop
                                current_sdb = Some(sdb);
                                sdb_pos = 0;
                                continue;
                            }
                            Err(e) => return Some(Err(e)),
                        }
                    }

                    // 3) If we have a next_addr, peek its ID to decide what it is
                    if next_addr != 0 {
                        let off = next_addr as usize;
                        // read the 4-byte ID
                        let id = &bytes[off..off + 4];
                        match id {
                            b"##DL" => {
                                // Data List Block
                                match DataListBlock::from_bytes(&bytes[off..]) {
                                    Ok(dl) => {
                                        data_links = dl.data_links.clone();
                                        link_idx = 0;
                                        next_addr = dl.next;
                                        continue; // back to loop start
                                    }
                                    Err(e) => return Some(Err(e)),
                                }
                            }
                            b"##SD" => {
                                // Direct Signal Data Block
                                match SignalDataBlock::from_bytes(&bytes[off..]) {
                                    Ok(sdb) => {
                                        current_sdb = Some(sdb);
                                        sdb_pos = 0;
                                        next_addr = 0; // no list chain
                                        continue;
                                    }
                                    Err(e) => return Some(Err(e)),
                                }
                            }
                            other => {
                                // unexpected block type
                                return Some(Err(Error::BlockIDError {
                                    actual: String::from_utf8_lossy(other).into(),
                                    expected: "##DL or ##SD".to_string(),
                                }));
                            }
                        }
                    }

                    // 4) Done
                    return None;
                }
            });

            return Ok(Box::new(vlsd_iter));
        }

        // Compute the size of each record:
        // Record structure: record_id + data_bytes + invalidation_bytes
        let record_id_len = data_group.block.record_id_len as usize;
        let sample_byte_len = channel_group.block.samples_byte_nr as usize;
        let invalidation_bytes = channel_group.block.invalidation_bytes_nr as usize;
        let record_size = record_id_len + sample_byte_len + invalidation_bytes;

        // Gather all DataBlock fragments (DT, DV or DZ):
        let blocks = data_group.data_blocks(mmap)?;

        // When record_id_len > 0 and there are multiple channel groups,
        // records of different types are mixed in the data block.
        // We need to parse by record ID and filter for this channel group.
        if record_id_len > 0 && data_group.channel_groups.len() > 1 {
            // Build record size lookup from all channel groups
            let mut record_sizes: std::collections::HashMap<u64, usize> =
                std::collections::HashMap::new();
            for cg in &data_group.channel_groups {
                let cg_record_size = record_id_len
                    + cg.block.samples_byte_nr as usize
                    + cg.block.invalidation_bytes_nr as usize;
                record_sizes.insert(cg.block.record_id, cg_record_size);
            }

            let target_record_id = channel_group.block.record_id;
            let target_record_size = record_size;

            // Collect matching records from all data blocks
            let mut matching_records: Vec<&'a [u8]> = Vec::new();

            for data_block in blocks {
                let data = data_block.data;
                let mut pos = 0;

                while pos < data.len() {
                    // Read record ID
                    let rid = if record_id_len == 1 {
                        if pos >= data.len() {
                            break;
                        }
                        data[pos] as u64
                    } else if record_id_len == 2 {
                        if pos + 2 > data.len() {
                            break;
                        }
                        u16::from_le_bytes([data[pos], data[pos + 1]]) as u64
                    } else if record_id_len == 4 {
                        if pos + 4 > data.len() {
                            break;
                        }
                        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                            as u64
                    } else if record_id_len == 8 {
                        if pos + 8 > data.len() {
                            break;
                        }
                        u64::from_le_bytes([
                            data[pos],
                            data[pos + 1],
                            data[pos + 2],
                            data[pos + 3],
                            data[pos + 4],
                            data[pos + 5],
                            data[pos + 6],
                            data[pos + 7],
                        ])
                    } else {
                        break;
                    };

                    // Get record size for this ID
                    let rec_size = match record_sizes.get(&rid) {
                        Some(&size) => size,
                        None => {
                            // Unknown record ID - try to resync by scanning for next valid ID
                            pos += 1;
                            continue;
                        }
                    };

                    if pos + rec_size > data.len() {
                        break;
                    }

                    // If this matches our target channel group, collect the record
                    if rid == target_record_id {
                        matching_records.push(&data[pos..pos + target_record_size]);
                    }

                    pos += rec_size;
                }
            }

            return Ok(Box::new(matching_records.into_iter().map(Ok)));
        }

        // Simple case: no record IDs or single channel group - all records same size
        let iter = blocks.into_iter().flat_map(move |data_block| {
            // For DZBLOCK you already unzipped into DataBlock, so here data_block.data
            let raw = data_block.data;
            let valid_len = (raw.len() / record_size) * record_size;
            // `chunks_exact` returns an iterator of &[u8] each exactly record_size
            raw[..valid_len]
                .chunks_exact(record_size)
                // wrap each slice in Ok(...) so the overall Iterator<Item=Result<_,_>>
                .map(Ok)
            // If you wanted to handle an unexpected remainder, you could check raw.len() % record_size != 0 here.
        });

        Ok(Box::new(iter))
    }
}
