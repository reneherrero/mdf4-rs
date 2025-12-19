use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse},
};

/// SDBLOCK: Signal Data Block (variable‐length signal values)
pub struct SignalDataBlock<'a> {
    pub header: BlockHeader,
    /// The concatenated sequence of VLSD values:
    /// [u32 length][value bytes] … repeated, back‐to‐back.
    pub data: &'a [u8],
}

impl<'a> BlockParse<'a> for SignalDataBlock<'a> {
    const ID: &'static str = "##SD";
    fn from_bytes(bytes: &'a [u8]) -> Result<Self> {
        // 1) Parse the common 24-byte block header
        let header = Self::parse_header(bytes)?;
        // 2) Ensure we have the full SDBLOCK on‐disk
        let expected_bytes = header.block_len as usize;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }

        // 4) The rest is the VLSD stream: [u32 length][value bytes]…
        let data = &bytes[24..expected_bytes];

        Ok(SignalDataBlock { header, data })
    }
}
