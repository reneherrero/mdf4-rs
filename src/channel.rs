use crate::{
    Result,
    blocks::{ChannelBlock, read_string_block},
    parsing::{
        RawChannel, RawChannelGroup, RawDataGroup, SourceInfo,
        decoder::{DecodedValue, decode_channel_value_with_validity},
    },
};

/// High level handle for a single channel within a group.
///
/// It holds references to the raw blocks and allows convenient access to
/// metadata and decoded values.
pub struct Channel<'a> {
    block: &'a ChannelBlock,
    raw_data_group: &'a RawDataGroup,
    raw_channel_group: &'a RawChannelGroup,
    raw_channel: &'a RawChannel,
    mmap: &'a [u8],
}

impl<'a> Channel<'a> {
    /// Construct a new [`Channel`] from raw block references.
    ///
    /// # Arguments
    /// * `block` - Channel block containing metadata
    /// * `raw_data_group` - Parent data group
    /// * `raw_channel_group` - Parent channel group
    /// * `raw_channel` - Raw channel helper used to iterate samples
    /// * `mmap` - Memory mapped file backing all data
    ///
    /// # Returns
    /// A [`Channel`] handle with no samples decoded yet.
    pub fn new(
        block: &'a ChannelBlock,
        raw_data_group: &'a RawDataGroup,
        raw_channel_group: &'a RawChannelGroup,
        raw_channel: &'a RawChannel,
        mmap: &'a [u8],
    ) -> Self {
        Channel {
            block,
            raw_data_group,
            raw_channel_group,
            raw_channel,
            mmap,
        }
    }
    /// Retrieve the channel name if present.
    pub fn name(&self) -> Result<Option<String>> {
        read_string_block(self.mmap, self.block.name_addr)
    }

    /// Retrieve the physical unit description.
    pub fn unit(&self) -> Result<Option<String>> {
        read_string_block(self.mmap, self.block.unit_addr)
    }

    /// Retrieve the channel comment if present.
    pub fn comment(&self) -> Result<Option<String>> {
        read_string_block(self.mmap, self.block.comment_addr)
    }

    /// Get the acquisition source for this channel if available.
    pub fn source(&self) -> Result<Option<SourceInfo>> {
        let addr = self.block.source_addr;
        SourceInfo::from_mmap(self.mmap, addr)
    }

    /// Decode and convert all samples of this channel.
    ///
    /// This method decodes all channel values and applies conversions.
    /// Invalid samples (as indicated by invalidation bits) are returned as `None`.
    ///
    /// # Returns
    /// A vector with one `Option<DecodedValue>` per record:
    /// - `Some(value)` for valid samples
    /// - `None` for invalid samples (invalidation bit set or decoding failed)
    pub fn values(&self) -> Result<Vec<Option<DecodedValue>>> {
        let record_id_size = self.raw_data_group.block.record_id_size as usize;
        let cg_data_bytes = self.raw_channel_group.block.record_size;
        let mut out = Vec::new();

        let records_iter =
            self.raw_channel
                .records(self.raw_data_group, self.raw_channel_group, self.mmap)?;

        for rec_res in records_iter {
            let rec = rec_res?;

            // Decode with validity checking
            if let Some(decoded) =
                decode_channel_value_with_validity(rec, record_id_size, cg_data_bytes, self.block)
            {
                if decoded.is_valid {
                    // Value is valid, apply conversion
                    let phys = self
                        .block
                        .apply_conversion_value(decoded.value, self.mmap)?;
                    out.push(Some(phys));
                } else {
                    // Value is invalid according to invalidation bit
                    out.push(None);
                }
            } else {
                // Decoding failed
                out.push(None);
            }
        }
        Ok(out)
    }

    /// Get the channel block (for internal use)
    pub fn block(&self) -> &ChannelBlock {
        self.block
    }

    /// Return a streaming iterator over decoded channel values.
    ///
    /// Unlike [`values()`](Self::values), this method does not load all values into memory.
    /// Instead, it decodes each value on-demand, making it suitable for large files.
    ///
    /// # Returns
    /// An iterator that yields `Result<Option<DecodedValue>>` for each record:
    /// - `Ok(Some(value))` for valid samples
    /// - `Ok(None)` for invalid samples (invalidation bit set or decoding failed)
    /// - `Err(...)` if there was an error reading or decoding the record
    ///
    /// # Example
    /// ```ignore
    /// for value_result in channel.iter_values()? {
    ///     match value_result {
    ///         Ok(Some(value)) => println!("Value: {:?}", value),
    ///         Ok(None) => println!("Invalid sample"),
    ///         Err(e) => eprintln!("Error: {:?}", e),
    ///     }
    /// }
    /// ```
    pub fn iter_values(&self) -> Result<ChannelValuesIter<'a>> {
        let record_id_size = self.raw_data_group.block.record_id_size as usize;
        let cg_data_bytes = self.raw_channel_group.block.record_size;

        let records_iter =
            self.raw_channel
                .records(self.raw_data_group, self.raw_channel_group, self.mmap)?;

        Ok(ChannelValuesIter {
            records_iter,
            block: self.block,
            mmap: self.mmap,
            record_id_size,
            cg_data_bytes,
        })
    }
}

/// Streaming iterator over decoded channel values.
///
/// Created by [`Channel::iter_values()`]. This iterator decodes values on-demand
/// without loading all data into memory, making it suitable for large MDF4 files.
pub struct ChannelValuesIter<'a> {
    records_iter: Box<dyn Iterator<Item = Result<&'a [u8]>> + 'a>,
    block: &'a ChannelBlock,
    mmap: &'a [u8],
    record_id_size: usize,
    cg_data_bytes: u32,
}

impl<'a> Iterator for ChannelValuesIter<'a> {
    type Item = Result<Option<DecodedValue>>;

    fn next(&mut self) -> Option<Self::Item> {
        let rec_result = self.records_iter.next()?;

        Some(match rec_result {
            Ok(rec) => {
                // Decode with validity checking
                if let Some(decoded) = decode_channel_value_with_validity(
                    rec,
                    self.record_id_size,
                    self.cg_data_bytes,
                    self.block,
                ) {
                    if decoded.is_valid {
                        // Value is valid, apply conversion
                        match self.block.apply_conversion_value(decoded.value, self.mmap) {
                            Ok(phys) => Ok(Some(phys)),
                            Err(e) => Err(e),
                        }
                    } else {
                        // Value is invalid according to invalidation bit
                        Ok(None)
                    }
                } else {
                    // Decoding failed
                    Ok(None)
                }
            }
            Err(e) => Err(e),
        })
    }
}
