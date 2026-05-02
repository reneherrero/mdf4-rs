//! Channel hierarchy list block (`##HL`) in the DG measurement-data chain.
//!
//! See ASAM MDF: `##DG.data_block_addr` may point at `##HL`, which in turn links to
//! `##DL` / `##DT` / `##DZ`. Sample layouts store `hl_dl_first` after the link section.
//!
//! When `feature = "std"` is off this crate omits parsing modules that call these helpers,
//! yet the helpers stay compiled with `blocks` (`alloc`) so `--features alloc` / `dbc` configs
//! that share code paths remain consistent (`-D warnings` gated below).

#![cfg_attr(not(feature = "std"), allow(dead_code))]

use crate::{
    Error, Result,
    blocks::common::{BlockHeader, read_u64, u64_to_usize, validate_buffer_size},
};

#[derive(Debug, Clone)]
pub struct HlBlock {}

impl HlBlock {
    /// Parse an `##HL` block and return the address of the next block in the measurement chain
    /// (typically `##DL` or `##DT`).
    pub(crate) fn next_block_addr(block_bytes: &[u8]) -> Result<u64> {
        validate_buffer_size(block_bytes, 24)?;
        let header = BlockHeader::from_bytes(&block_bytes[..24])?;
        let total = u64_to_usize(header.length, "##HL length")?;
        validate_buffer_size(block_bytes, total)?;

        let link_count = usize::try_from(header.link_count).map_err(|_| {
            Error::BlockSerializationError("##HL: link_count does not fit usize".into())
        })?;
        let links_end =
            24usize.saturating_add(link_count.checked_mul(8).ok_or_else(|| {
                Error::BlockSerializationError("##HL: link section overflow".into())
            })?);
        if links_end > total {
            return Err(Error::BlockSerializationError(
                "##HL: links extend past block length".into(),
            ));
        }

        let mut off = 24usize;
        while off < links_end {
            let addr = read_u64(block_bytes, off);
            if addr != 0 {
                return Ok(addr);
            }
            off += 8;
        }

        if links_end + 8 <= total {
            let addr = read_u64(block_bytes, links_end);
            if addr != 0 {
                return Ok(addr);
            }
        }

        Err(Error::BlockSerializationError(
            "##HL: no pointer to next data block".into(),
        ))
    }

    /// Skip zero or more consecutive `##HL` blocks starting at `addr`.
    ///
    /// Returns the address and header of the first block that is not `##HL`.
    pub(crate) fn skip_hierarchy_blocks(mmap: &[u8], mut addr: u64) -> Result<(u64, BlockHeader)> {
        loop {
            if addr == 0 {
                return Err(Error::BlockSerializationError(
                    "##HL: reached null pointer in data chain".into(),
                ));
            }
            let off = u64_to_usize(addr, "data block link")?;
            validate_buffer_size(&mmap[off..], 24)?;
            let header = BlockHeader::from_bytes(&mmap[off..off + 24])?;
            if header.id.as_str() != "##HL" {
                return Ok((addr, header));
            }
            let len = u64_to_usize(header.length, "##HL")?;
            validate_buffer_size(&mmap[off..], len)?;
            addr = Self::next_block_addr(&mmap[off..off + len])?;
        }
    }
}
