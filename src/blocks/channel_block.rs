use super::CN_BLOCK_SIZE;
use crate::{
    Result,
    blocks::{
        common::{
            BlockHeader, BlockParse, DataType, debug_assert_aligned, read_f64, read_u8, read_u16,
            read_u32, read_u64, validate_block_id, validate_block_length, validate_buffer_size,
        },
        conversion::ConversionBlock,
        text_block::TextBlock,
    },
    types::DecodedValue,
};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct ChannelBlock {
    pub header: BlockHeader,
    pub next_ch_addr: u64,
    pub component_addr: u64,
    pub name_addr: u64,
    pub source_addr: u64,
    pub conversion_addr: u64,
    pub data_addr: u64,
    pub unit_addr: u64,
    pub comment_addr: u64,
    pub channel_type: u8,
    pub sync_type: u8,
    pub data_type: DataType,
    pub bit_offset: u8,
    pub byte_offset: u32,
    pub bit_count: u32,
    pub flags: u32,
    pub pos_invalidation_bit: u32,
    pub precision: u8,
    pub reserved1: u8,
    pub attachment_count: u16,
    pub min_raw_value: f64,
    pub max_raw_value: f64,
    pub lower_limit: f64,
    pub upper_limit: f64,
    pub lower_ext_limit: f64,
    pub upper_ext_limit: f64,
    pub name: Option<String>,
    pub conversion: Option<ConversionBlock>,
}

impl BlockParse<'_> for ChannelBlock {
    const ID: &'static str = "##CN";

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        validate_buffer_size(bytes, CN_BLOCK_SIZE)?;

        Ok(Self {
            header,
            // Links section (8 x u64 = 64 bytes at offset 24)
            next_ch_addr: read_u64(bytes, 24),
            component_addr: read_u64(bytes, 32),
            name_addr: read_u64(bytes, 40),
            source_addr: read_u64(bytes, 48),
            conversion_addr: read_u64(bytes, 56),
            data_addr: read_u64(bytes, 64),
            unit_addr: read_u64(bytes, 72),
            comment_addr: read_u64(bytes, 80),
            // Format section at offset 88
            channel_type: read_u8(bytes, 88),
            sync_type: read_u8(bytes, 89),
            data_type: DataType::from_u8(read_u8(bytes, 90)),
            bit_offset: read_u8(bytes, 91),
            byte_offset: read_u32(bytes, 92),
            bit_count: read_u32(bytes, 96),
            flags: read_u32(bytes, 100),
            pos_invalidation_bit: read_u32(bytes, 104),
            precision: read_u8(bytes, 108),
            reserved1: read_u8(bytes, 109),
            attachment_count: read_u16(bytes, 110),
            // Range section (6 x f64 = 48 bytes at offset 112)
            min_raw_value: read_f64(bytes, 112),
            max_raw_value: read_f64(bytes, 120),
            lower_limit: read_f64(bytes, 128),
            upper_limit: read_f64(bytes, 136),
            lower_ext_limit: read_f64(bytes, 144),
            upper_ext_limit: read_f64(bytes, 152),
            // Resolved fields
            name: None,
            conversion: None,
        })
    }
}

impl ChannelBlock {
    /// Serializes the ChannelBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        validate_block_id(&self.header, "##CN")?;
        validate_block_length(&self.header, CN_BLOCK_SIZE as u64)?;

        let mut buffer = Vec::with_capacity(CN_BLOCK_SIZE);

        // Header (24 bytes)
        buffer.extend_from_slice(&self.header.to_bytes()?);

        // Links (64 bytes)
        buffer.extend_from_slice(&self.next_ch_addr.to_le_bytes());
        buffer.extend_from_slice(&self.component_addr.to_le_bytes());
        buffer.extend_from_slice(&self.name_addr.to_le_bytes());
        buffer.extend_from_slice(&self.source_addr.to_le_bytes());
        buffer.extend_from_slice(&self.conversion_addr.to_le_bytes());
        buffer.extend_from_slice(&self.data_addr.to_le_bytes());
        buffer.extend_from_slice(&self.unit_addr.to_le_bytes());
        buffer.extend_from_slice(&self.comment_addr.to_le_bytes());

        // Format section (24 bytes)
        buffer.push(self.channel_type);
        buffer.push(self.sync_type);
        buffer.push(self.data_type.to_u8());
        buffer.push(self.bit_offset);
        buffer.extend_from_slice(&self.byte_offset.to_le_bytes());
        buffer.extend_from_slice(&self.bit_count.to_le_bytes());
        buffer.extend_from_slice(&self.flags.to_le_bytes());
        buffer.extend_from_slice(&self.pos_invalidation_bit.to_le_bytes());
        buffer.push(self.precision);
        buffer.push(self.reserved1);
        buffer.extend_from_slice(&self.attachment_count.to_le_bytes());

        // Range section (48 bytes)
        buffer.extend_from_slice(&self.min_raw_value.to_le_bytes());
        buffer.extend_from_slice(&self.max_raw_value.to_le_bytes());
        buffer.extend_from_slice(&self.lower_limit.to_le_bytes());
        buffer.extend_from_slice(&self.upper_limit.to_le_bytes());
        buffer.extend_from_slice(&self.lower_ext_limit.to_le_bytes());
        buffer.extend_from_slice(&self.upper_ext_limit.to_le_bytes());

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }

    /// Load the channel name from the file using the stored `name_addr`.
    pub fn resolve_name(&mut self, file_data: &[u8]) -> Result<()> {
        if self.name.is_none() && self.name_addr != 0 {
            let offset = self.name_addr as usize;
            if offset + 24 <= file_data.len() {
                let text_block = TextBlock::from_bytes(&file_data[offset..])?;
                self.name = Some(text_block.text);
            }
        }
        Ok(())
    }

    /// Resolve and store the conversion block pointed to by `conversion_addr`.
    pub fn resolve_conversion(&mut self, bytes: &[u8]) -> Result<()> {
        if self.conversion.is_none() && self.conversion_addr != 0 {
            let offset = self.conversion_addr as usize;
            validate_buffer_size(bytes, offset + 24)?;

            let mut conv_block = ConversionBlock::from_bytes(&bytes[offset..])?;
            let _ = conv_block.resolve_formula(bytes);
            self.conversion = Some(conv_block);
        }
        Ok(())
    }

    /// Apply the stored conversion to a decoded value.
    pub fn apply_conversion_value(
        &self,
        raw: DecodedValue,
        file_data: &[u8],
    ) -> Result<DecodedValue> {
        if let Some(conv) = &self.conversion {
            conv.apply_decoded(raw, file_data)
        } else {
            Ok(raw)
        }
    }
}

impl Default for ChannelBlock {
    fn default() -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##CN"),
                reserved: 0,
                length: CN_BLOCK_SIZE as u64,
                link_count: 8,
            },
            next_ch_addr: 0,
            component_addr: 0,
            name_addr: 0,
            source_addr: 0,
            conversion_addr: 0,
            data_addr: 0,
            unit_addr: 0,
            comment_addr: 0,
            channel_type: 0,
            sync_type: 0,
            data_type: DataType::UnsignedIntegerLE,
            bit_offset: 0,
            byte_offset: 0,
            bit_count: 0,
            flags: 0,
            pos_invalidation_bit: 0,
            precision: 0,
            reserved1: 0,
            attachment_count: 0,
            min_raw_value: 0.0,
            max_raw_value: 0.0,
            lower_limit: 0.0,
            upper_limit: 0.0,
            lower_ext_limit: 0.0,
            upper_ext_limit: 0.0,
            name: None,
            conversion: None,
        }
    }
}
