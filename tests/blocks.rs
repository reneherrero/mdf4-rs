use mdf4_rs::blocks::{
    BlockHeader, BlockParse, ChannelBlock, ChannelGroupBlock, DataBlock, DataGroupBlock,
    DataListBlock, HeaderBlock, IdentificationBlock, MetadataBlock, SignalDataBlock, SourceBlock,
    TextBlock,
};
use mdf4_rs::{DataType, Result};

fn header(id: &str, len: u64, links: u64) -> BlockHeader {
    BlockHeader {
        id: id.to_string(),
        reserved0: 0,
        block_len: len,
        links_nr: links,
    }
}

#[test]
fn block_header_roundtrip() -> Result<()> {
    let h = header("TEST", 64, 2);
    let bytes = h.to_bytes()?;
    let parsed = BlockHeader::from_bytes(&bytes)?;
    assert_eq!(parsed.id, "TEST");
    assert_eq!(parsed.block_len, 64);
    assert_eq!(parsed.links_nr, 2);
    Ok(())
}

#[test]
fn text_block_roundtrip() -> Result<()> {
    let tb = TextBlock::new("hello");
    let bytes = tb.to_bytes()?;
    let parsed = TextBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.text, "hello");
    Ok(())
}

#[test]
fn metadata_block_parse() -> Result<()> {
    let xml = "<x/>";
    let mut h = header("##MD", 0, 0);
    let needs_null = true;
    let base_len = 24 + xml.len() + if needs_null { 1 } else { 0 };
    let padding = (8 - (base_len % 8)) % 8;
    h.block_len = (base_len + padding) as u64;
    let mut bytes = h.to_bytes()?;
    bytes.extend_from_slice(xml.as_bytes());
    if needs_null {
        bytes.push(0);
    }
    bytes.extend_from_slice(&vec![0u8; padding]);
    let parsed = MetadataBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.xml, xml);
    Ok(())
}

#[test]
fn data_block_parse() -> Result<()> {
    let data = vec![1u8, 2, 3, 4];
    let h = header("##DT", 24 + data.len() as u64, 0);
    let mut bytes = h.to_bytes()?;
    bytes.extend_from_slice(&data);
    let block = DataBlock::from_bytes(&bytes)?;
    assert_eq!(block.data, &data[..]);
    Ok(())
}

#[test]
fn data_list_block_roundtrip() -> Result<()> {
    let dl = DataListBlock::new_equal(vec![0x10, 0x20], 8);
    let bytes = dl.to_bytes()?;
    let parsed = DataListBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.data_links, vec![0x10, 0x20]);
    assert_eq!(parsed.data_block_len, Some(8));
    Ok(())
}

#[test]
fn signal_data_block_parse() -> Result<()> {
    let h = header("##SD", 32, 0);
    let mut bytes = h.to_bytes()?;
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.push(42);
    bytes.extend_from_slice(&[0u8; 7]);
    let sd = SignalDataBlock::from_bytes(&bytes)?;
    assert_eq!(sd.data[0..4], 1u32.to_le_bytes());
    assert_eq!(sd.data[4], 42);
    Ok(())
}

#[test]
fn source_block_parse() -> Result<()> {
    let h = header("##SI", 56, 3);
    let mut bytes = h.to_bytes()?;
    bytes.extend_from_slice(&1u64.to_le_bytes());
    bytes.extend_from_slice(&2u64.to_le_bytes());
    bytes.extend_from_slice(&3u64.to_le_bytes());
    bytes.extend_from_slice(&[1, 2, 3, 0, 0, 0, 0, 0]);
    let sb = SourceBlock::from_bytes(&bytes)?;
    assert_eq!(sb.name_addr, 1);
    assert_eq!(sb.path_addr, 2);
    assert_eq!(sb.comment_addr, 3);
    assert_eq!(sb.source_type, 1);
    assert_eq!(sb.bus_type, 2);
    assert_eq!(sb.flags, 3);
    Ok(())
}

#[test]
fn identification_block_roundtrip() -> Result<()> {
    let ib = IdentificationBlock::default();
    let bytes = ib.to_bytes()?;
    let parsed = IdentificationBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.version_number, ib.version_number);
    Ok(())
}

#[test]
fn header_block_roundtrip() -> Result<()> {
    let hb = HeaderBlock::default();
    let bytes = hb.to_bytes()?;
    let parsed = HeaderBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.first_dg_addr, 0);
    assert_eq!(parsed.header.id, "##HD");
    Ok(())
}

#[test]
fn data_group_block_roundtrip() -> Result<()> {
    let dg = DataGroupBlock::default();
    let bytes = dg.to_bytes()?;
    let parsed = DataGroupBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.record_id_len, dg.record_id_len);
    Ok(())
}

#[test]
fn channel_group_block_roundtrip() -> Result<()> {
    let cg = ChannelGroupBlock::default();
    let bytes = cg.to_bytes()?;
    let parsed = ChannelGroupBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.samples_byte_nr, cg.samples_byte_nr);
    Ok(())
}

#[test]
fn channel_block_roundtrip() -> Result<()> {
    let ch = ChannelBlock::default();
    let bytes = ch.to_bytes()?;
    let parsed = ChannelBlock::from_bytes(&bytes)?;
    assert_eq!(parsed.bit_count, ch.bit_count);
    assert_eq!(
        parsed.data_type.to_u8(),
        DataType::UnsignedIntegerLE.to_u8()
    );
    Ok(())
}
