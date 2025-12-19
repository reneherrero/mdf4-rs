// Functions for creating and linking MDF structure blocks
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{MdfWrite, MdfWriter};
use crate::{
    Result,
    blocks::{
        BlockHeader, ChannelBlock, ChannelGroupBlock, DataGroupBlock, HeaderBlock,
        IdentificationBlock, TextBlock, SourceBlock, {ConversionBlock, ConversionType},
    },
};

impl<W: MdfWrite> MdfWriter<W> {
    /// Initializes a new MDF 4.1 file with identification and header blocks.
    pub fn init_mdf_file(&mut self) -> Result<(u64, u64)> {
        let id_block = IdentificationBlock::default();
        let id_bytes = id_block.to_bytes()?;
        let id_pos = self.write_block_with_id(&id_bytes, "id_block")?;

        let hd_block = HeaderBlock::default();
        let hd_bytes = hd_block.to_bytes()?;
        let hd_pos = self.write_block_with_id(&hd_bytes, "hd_block")?;
        Ok((id_pos, hd_pos))
    }

    /// Adds a data group block to the file and links it from the header block.
    pub fn add_data_group(&mut self, prev_dg_id: Option<&str>) -> Result<String> {
        let dg_count = self
            .block_positions
            .keys()
            .filter(|k| k.starts_with("dg_"))
            .count();
        let dg_id = format!("dg_{}", dg_count);
        let dg_block = DataGroupBlock::default();
        let dg_bytes = dg_block.to_bytes()?;
        let _pos = self.write_block_with_id(&dg_bytes, &dg_id)?;

        if let Some(prev) = prev_dg_id {
            let prev_off = 24;
            self.update_block_link(prev, prev_off, &dg_id)?;
        } else {
            let hd_dg_link_offset = 24;
            self.update_block_link("hd_block", hd_dg_link_offset, &dg_id)?;
        }
        Ok(dg_id)
    }

    /// Adds a channel group block to the specified data group and links it.
    pub fn add_channel_group_with_dg<F>(
        &mut self,
        dg_id: &str,
        prev_cg_id: Option<&str>,
        configure: F,
    ) -> Result<String>
    where
        F: FnOnce(&mut ChannelGroupBlock),
    {
        let cg_count = self
            .block_positions
            .keys()
            .filter(|k| k.starts_with("cg_"))
            .count();
        let cg_id = format!("cg_{}", cg_count);

        let mut cg_block = ChannelGroupBlock::default();
        configure(&mut cg_block);

        let cg_bytes = cg_block.to_bytes()?;
        let _pos = self.write_block_with_id(&cg_bytes, &cg_id)?;

        if let Some(prev) = prev_cg_id {
            let prev_cg_off = 24;
            self.update_block_link(prev, prev_cg_off, &cg_id)?;
        } else {
            let dg_cg_link_offset = 32;
            self.update_block_link(dg_id, dg_cg_link_offset, &cg_id)?;
        }
        Ok(cg_id)
    }

    /// Adds a channel group and automatically creates a new data group for it.
    pub fn add_channel_group<F>(&mut self, prev_cg_id: Option<&str>, configure: F) -> Result<String>
    where
        F: FnOnce(&mut ChannelGroupBlock),
    {
        let dg_id = match self.last_dg.clone() {
            Some(last) => self.add_data_group(Some(&last))?,
            None => self.add_data_group(None)?,
        };
        self.last_dg = Some(dg_id.clone());
        let cg_id = self.add_channel_group_with_dg(&dg_id, prev_cg_id, configure)?;
        self.cg_to_dg.insert(cg_id.clone(), dg_id);
        self.cg_offsets.insert(cg_id.clone(), 0);
        self.cg_channels.insert(cg_id.clone(), Vec::new());
        Ok(cg_id)
    }

    /// Creates and writes a simple value-to-text conversion block.
    pub fn add_value_to_text_conversion(
        &mut self,
        mapping: &[(i64, &str)],
        default_text: &str,
        channel_id: Option<&str>,
    ) -> Result<(String, u64)> {
        let cc_count = self
            .block_positions
            .keys()
            .filter(|k| k.starts_with("cc_"))
            .count();
        let cc_id = format!("cc_{}", cc_count);

        let mut refs = Vec::new();
        for (idx, (_, txt)) in mapping.iter().enumerate() {
            let tx_id = format!("tx_{}_{}", cc_id, idx);
            let tx_block = TextBlock::new(txt);
            let tx_bytes = tx_block.to_bytes()?;
            let pos = self.write_block_with_id(&tx_bytes, &tx_id)?;
            refs.push(pos);
        }
        let tx_default_id = format!("tx_{}_default", cc_id);
        let tx_default = TextBlock::new(default_text);
        let tx_bytes = tx_default.to_bytes()?;
        let default_pos = self.write_block_with_id(&tx_bytes, &tx_default_id)?;
        refs.push(default_pos);

        let vals: Vec<f64> = mapping.iter().map(|(v, _)| *v as f64).collect();

        let block = ConversionBlock {
            header: BlockHeader {
                id: "##CC".into(),
                reserved0: 0,
                block_len: 0,
                links_nr: 0,
            },
            cc_tx_name: None,
            cc_md_unit: None,
            cc_md_comment: None,
            cc_cc_inverse: None,
            cc_ref: refs,
            cc_type: ConversionType::ValueToText,
            cc_precision: 0,
            cc_flags: 0b10,
            cc_ref_count: (mapping.len() + 1) as u16,
            cc_val_count: mapping.len() as u16,
            cc_phy_range_min: Some(0.0),
            cc_phy_range_max: Some(0.0),
            cc_val: vals,
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        };
        let cc_bytes = block.to_bytes()?;
        let pos = self.write_block_with_id(&cc_bytes, &cc_id)?;

        if let Some(cn) = channel_id {
            let conv_offset = 56u64;
            self.update_block_link(cn, conv_offset, &cc_id)?;
        }
        Ok((cc_id, pos))
    }

    /// Adds a channel block to the specified channel group and links it.
    pub fn add_channel<F>(
        &mut self,
        cg_id: &str,
        prev_cn_id: Option<&str>,
        configure: F,
    ) -> Result<String>
    where
        F: FnOnce(&mut ChannelBlock),
    {
        let cn_count = self
            .block_positions
            .keys()
            .filter(|k| k.starts_with("cn_"))
            .count();
        let cn_id = format!("cn_{}", cn_count);

        let mut ch = ChannelBlock::default();
        configure(&mut ch);
        if ch.bit_count == 0 {
            ch.bit_count = ch.data_type.default_bits();
        }
        if let Some(off) = self.cg_offsets.get_mut(cg_id) {
            if ch.byte_offset == 0 {
                ch.byte_offset = *off as u32;
            }
            let used = (ch.bit_offset as usize + ch.bit_count as usize).div_ceil(8);
            *off = ch.byte_offset as usize + used;
        }

        let cn_bytes = ch.to_bytes()?;
        let cn_pos = self.write_block_with_id(&cn_bytes, &cn_id)?;
        if let Some(channel_name) = &ch.name {
            let tx_id = format!("tx_name_{}", cn_id);
            let tx_block = TextBlock::new(channel_name);
            let tx_bytes = tx_block.to_bytes()?;
            let tx_pos = self.write_block_with_id(&tx_bytes, &tx_id)?;
            let name_link_offset = 40;
            self.update_link(cn_pos + name_link_offset, tx_pos)?;
        }

        let entry = self.cg_channels.entry(cg_id.to_string()).or_default();
        entry.push(ch.clone());
        let idx = entry.len() - 1;
        self.channel_map
            .insert(cn_id.clone(), (cg_id.to_string(), idx));

        if let Some(prev_cn) = prev_cn_id {
            let prev_cn_next_link_offset = 24;
            self.update_block_link(prev_cn, prev_cn_next_link_offset, &cn_id)?;
        } else {
            let cg_cn_link_offset = 32;
            self.update_block_link(cg_id, cg_cn_link_offset, &cn_id)?;
        }
        Ok(cn_id)
    }

    /// Mark an existing channel as the time (master) channel.
    pub fn set_time_channel(&mut self, cn_id: &str) -> Result<()> {
        const CHANNEL_TYPE_OFFSET: u64 = 88;
        const SYNC_TYPE_OFFSET: u64 = 89;
        self.update_block_u8(cn_id, CHANNEL_TYPE_OFFSET, 2)?;
        self.update_block_u8(cn_id, SYNC_TYPE_OFFSET, 1)?;

        if let Some((cg, idx)) = self.channel_map.get(cn_id).cloned()
            && let Some(chs) = self.cg_channels.get_mut(&cg)
            && let Some(ch) = chs.get_mut(idx)
        {
            ch.channel_type = 2;
            ch.sync_type = 1;
        }
        Ok(())
    }

    /// Sets the unit string for an existing channel.
    ///
    /// This creates a text block containing the unit string and links it
    /// to the channel's unit_addr field.
    ///
    /// # Arguments
    /// * `cn_id` - The channel ID returned from `add_channel()`
    /// * `unit` - The unit string (e.g., "rpm", "°C", "km/h")
    ///
    /// # Example
    /// ```ignore
    /// let ch = writer.add_channel(&cg, None, |ch| {
    ///     ch.name = Some("Temperature".into());
    ///     ch.data_type = DataType::FloatLE;
    ///     ch.bit_count = 64;
    /// })?;
    /// writer.set_channel_unit(&ch, "°C")?;
    /// ```
    pub fn set_channel_unit(&mut self, cn_id: &str, unit: &str) -> Result<()> {
        if unit.is_empty() {
            return Ok(());
        }

        let cn_pos = self
            .get_block_position(cn_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel '{}' not found", cn_id)))?;

        let tx_id = format!("tx_unit_{}", cn_id);
        let tx_block = TextBlock::new(unit);
        let tx_bytes = tx_block.to_bytes()?;
        let tx_pos = self.write_block_with_id(&tx_bytes, &tx_id)?;

        // unit_addr is at offset 72 in ChannelBlock (after header + 6 links)
        const UNIT_ADDR_OFFSET: u64 = 72;
        self.update_link(cn_pos + UNIT_ADDR_OFFSET, tx_pos)?;

        Ok(())
    }

    /// Sets the comment/description for an existing channel.
    ///
    /// This creates a text block containing the comment and links it
    /// to the channel's comment_addr field.
    ///
    /// # Arguments
    /// * `cn_id` - The channel ID returned from `add_channel()`
    /// * `comment` - The comment/description string
    pub fn set_channel_comment(&mut self, cn_id: &str, comment: &str) -> Result<()> {
        if comment.is_empty() {
            return Ok(());
        }

        let cn_pos = self
            .get_block_position(cn_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel '{}' not found", cn_id)))?;

        let tx_id = format!("tx_comment_{}", cn_id);
        let tx_block = TextBlock::new(comment);
        let tx_bytes = tx_block.to_bytes()?;
        let tx_pos = self.write_block_with_id(&tx_bytes, &tx_id)?;

        // comment_addr is at offset 80 in ChannelBlock
        const COMMENT_ADDR_OFFSET: u64 = 80;
        self.update_link(cn_pos + COMMENT_ADDR_OFFSET, tx_pos)?;

        Ok(())
    }

    /// Sets the conversion block for an existing channel.
    ///
    /// This writes the conversion block and links it to the channel's
    /// conversion_addr field. Use `ConversionBlock::linear()` or other
    /// constructors to create the conversion.
    ///
    /// # Arguments
    /// * `cn_id` - The channel ID returned from `add_channel()`
    /// * `conversion` - The conversion block to attach
    ///
    /// # Example
    /// ```ignore
    /// use mdf4_rs::blocks::ConversionBlock;
    ///
    /// let ch = writer.add_channel(&cg, None, |ch| {
    ///     ch.name = Some("Temperature".into());
    ///     ch.data_type = DataType::SignedIntegerLE;
    ///     ch.bit_count = 16;
    /// })?;
    ///
    /// // Raw value to Celsius: physical = -40 + 0.1 * raw
    /// let conv = ConversionBlock::linear(-40.0, 0.1);
    /// writer.set_channel_conversion(&ch, &conv)?;
    /// ```
    pub fn set_channel_conversion(&mut self, cn_id: &str, conversion: &ConversionBlock) -> Result<()> {
        // Skip identity conversions (they're redundant)
        if conversion.is_identity() {
            return Ok(());
        }

        let cn_pos = self
            .get_block_position(cn_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel '{}' not found", cn_id)))?;

        let cc_count = self
            .block_positions
            .keys()
            .filter(|k| k.starts_with("cc_"))
            .count();
        let cc_id = format!("cc_{}", cc_count);

        let cc_bytes = conversion.to_bytes()?;
        let cc_pos = self.write_block_with_id(&cc_bytes, &cc_id)?;

        // conversion_addr is at offset 56 in ChannelBlock
        const CONVERSION_ADDR_OFFSET: u64 = 56;
        self.update_link(cn_pos + CONVERSION_ADDR_OFFSET, cc_pos)?;

        Ok(())
    }

    /// Sets channel limits (min/max physical values).
    ///
    /// Updates the lower_limit and upper_limit fields in the channel block.
    ///
    /// # Arguments
    /// * `cn_id` - The channel ID returned from `add_channel()`
    /// * `min` - Minimum physical value
    /// * `max` - Maximum physical value
    pub fn set_channel_limits(&mut self, cn_id: &str, min: f64, max: f64) -> Result<()> {
        let cn_pos = self
            .get_block_position(cn_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel '{}' not found", cn_id)))?;

        // lower_limit is at offset 128, upper_limit at 136
        const LOWER_LIMIT_OFFSET: u64 = 128;
        const UPPER_LIMIT_OFFSET: u64 = 136;

        self.update_link(cn_pos + LOWER_LIMIT_OFFSET, min.to_bits())?;
        self.update_link(cn_pos + UPPER_LIMIT_OFFSET, max.to_bits())?;

        // Update in-memory copy
        if let Some((cg, idx)) = self.channel_map.get(cn_id).cloned()
            && let Some(chs) = self.cg_channels.get_mut(&cg)
            && let Some(ch) = chs.get_mut(idx)
        {
            ch.lower_limit = min;
            ch.upper_limit = max;
        }

        Ok(())
    }

    /// Adds a linear conversion to a channel.
    ///
    /// Convenience method that combines `set_channel_conversion` with
    /// `ConversionBlock::linear()`.
    ///
    /// # Arguments
    /// * `cn_id` - The channel ID returned from `add_channel()`
    /// * `offset` - The offset value (P1): physical = offset + factor * raw
    /// * `factor` - The scaling factor (P2)
    ///
    /// # Example
    /// ```ignore
    /// // Raw to RPM: physical = 0 + 0.25 * raw
    /// writer.add_linear_conversion(&ch, 0.0, 0.25)?;
    /// ```
    pub fn add_linear_conversion(&mut self, cn_id: &str, offset: f64, factor: f64) -> Result<()> {
        // Skip identity conversions
        if offset == 0.0 && factor == 1.0 {
            return Ok(());
        }

        let conversion = ConversionBlock::linear(offset, factor);
        self.set_channel_conversion(cn_id, &conversion)
    }

    /// Sets the acquisition name for an existing channel group.
    ///
    /// This creates a text block containing the name and links it
    /// to the channel group's acq_name_addr field.
    ///
    /// # Arguments
    /// * `cg_id` - The channel group ID returned from `add_channel_group()`
    /// * `name` - The acquisition/group name (e.g., "Engine", "Transmission")
    ///
    /// # Example
    /// ```ignore
    /// let cg = writer.add_channel_group(None, |_| {})?;
    /// writer.set_channel_group_name(&cg, "Engine_0x100")?;
    /// ```
    pub fn set_channel_group_name(&mut self, cg_id: &str, name: &str) -> Result<()> {
        if name.is_empty() {
            return Ok(());
        }

        let cg_pos = self
            .get_block_position(cg_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel group '{}' not found", cg_id)))?;

        let tx_id = format!("tx_cgname_{}", cg_id);
        let tx_block = TextBlock::new(name);
        let tx_bytes = tx_block.to_bytes()?;
        let tx_pos = self.write_block_with_id(&tx_bytes, &tx_id)?;

        // acq_name_addr is at offset 40 in ChannelGroupBlock (after header + 2 links)
        const ACQ_NAME_ADDR_OFFSET: u64 = 40;
        self.update_link(cg_pos + ACQ_NAME_ADDR_OFFSET, tx_pos)?;

        Ok(())
    }

    /// Sets the comment for an existing channel group.
    ///
    /// This creates a text block containing the comment and links it
    /// to the channel group's comment_addr field.
    ///
    /// # Arguments
    /// * `cg_id` - The channel group ID returned from `add_channel_group()`
    /// * `comment` - The comment/description string
    pub fn set_channel_group_comment(&mut self, cg_id: &str, comment: &str) -> Result<()> {
        if comment.is_empty() {
            return Ok(());
        }

        let cg_pos = self
            .get_block_position(cg_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel group '{}' not found", cg_id)))?;

        let tx_id = format!("tx_cgcomment_{}", cg_id);
        let tx_block = TextBlock::new(comment);
        let tx_bytes = tx_block.to_bytes()?;
        let tx_pos = self.write_block_with_id(&tx_bytes, &tx_id)?;

        // comment_addr is at offset 64 in ChannelGroupBlock
        const COMMENT_ADDR_OFFSET: u64 = 64;
        self.update_link(cg_pos + COMMENT_ADDR_OFFSET, tx_pos)?;

        Ok(())
    }

    /// Sets the acquisition source for an existing channel group.
    ///
    /// This writes a source block with the given name and links it
    /// to the channel group's acq_source_addr field.
    ///
    /// # Arguments
    /// * `cg_id` - The channel group ID returned from `add_channel_group()`
    /// * `source` - The source block to attach
    /// * `source_name` - Optional name for the source (e.g., ECU name)
    ///
    /// # Example
    /// ```ignore
    /// use mdf4_rs::blocks::{SourceBlock, SourceType, BusType};
    ///
    /// let cg = writer.add_channel_group(None, |_| {})?;
    /// let source = SourceBlock::can_ecu();
    /// writer.set_channel_group_source(&cg, &source, Some("ECM"))?;
    /// ```
    pub fn set_channel_group_source(
        &mut self,
        cg_id: &str,
        source: &SourceBlock,
        source_name: Option<&str>,
    ) -> Result<()> {
        let cg_pos = self
            .get_block_position(cg_id)
            .ok_or_else(|| crate::Error::BlockLinkError(format!("Channel group '{}' not found", cg_id)))?;

        let si_count = self
            .block_positions
            .keys()
            .filter(|k| k.starts_with("si_"))
            .count();
        let si_id = format!("si_{}", si_count);

        // Clone source and optionally set name
        let mut source = source.clone();

        // Write source name text block if provided
        if let Some(name) = source_name {
            if !name.is_empty() {
                let tx_id = format!("tx_siname_{}", si_id);
                let tx_block = TextBlock::new(name);
                let tx_bytes = tx_block.to_bytes()?;
                let tx_pos = self.write_block_with_id(&tx_bytes, &tx_id)?;
                source.name_addr = tx_pos;
            }
        }

        let si_bytes = source.to_bytes()?;
        let si_pos = self.write_block_with_id(&si_bytes, &si_id)?;

        // Update name link in the source block if we wrote one
        if source.name_addr != 0 {
            // name_addr is at offset 24 in SourceBlock
            self.update_link(si_pos + 24, source.name_addr)?;
        }

        // acq_source_addr is at offset 48 in ChannelGroupBlock
        const ACQ_SOURCE_ADDR_OFFSET: u64 = 48;
        self.update_link(cg_pos + ACQ_SOURCE_ADDR_OFFSET, si_pos)?;

        Ok(())
    }

    /// Convenience method to set channel group source with just a name.
    ///
    /// Creates a CAN ECU source block with the given name.
    ///
    /// # Arguments
    /// * `cg_id` - The channel group ID
    /// * `ecu_name` - The ECU/sender name
    pub fn set_channel_group_source_name(&mut self, cg_id: &str, ecu_name: &str) -> Result<()> {
        if ecu_name.is_empty() {
            return Ok(());
        }
        let source = SourceBlock::can_ecu();
        self.set_channel_group_source(cg_id, &source, Some(ecu_name))
    }
}
