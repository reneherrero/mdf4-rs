// Functions for creating and linking MDF structure blocks
use super::MdfWriter;
use crate::{
    Result,
    blocks::{
        BlockHeader, ChannelBlock, ChannelGroupBlock, DataGroupBlock, HeaderBlock,
        IdentificationBlock, TextBlock, {ConversionBlock, ConversionType},
    },
};

impl MdfWriter {
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
}
