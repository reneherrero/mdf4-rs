use crate::{
    Error, Result,
    blocks::common::{BlockHeader, BlockParse},
};

#[derive(Debug, Clone)]
pub struct DataBlock<'a> {
    pub header: BlockHeader,
    pub data: &'a [u8],
}

impl<'a> BlockParse<'a> for DataBlock<'a> {
    const ID: &'static str = "##DT";
    /// Parse a DTBLOCK from the given byte slice.
    ///
    /// The slice must contain at least the number of bytes specified by the
    /// block length in the header. Only a reference to the data portion is
    /// stored to avoid unnecessary allocations.
    fn from_bytes(bytes: &'a [u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let data_len = (header.length as usize).saturating_sub(24);
        let expected_bytes = 24 + data_len;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }
        let data = &bytes[24..24 + data_len];
        Ok(Self { header, data })
    }
}

impl<'a> DataBlock<'a> {
    /// Parse a DTBLOCK from an unfinalized MDF file.
    ///
    /// In unfinalized files, the block_len in the header may be incorrect (set to 24,
    /// header only), but actual data continues until the end of the file.
    /// This method reads all remaining bytes after the header as data.
    pub fn from_bytes_unfinalized(bytes: &'a [u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        // Use all bytes after the header as data
        let data = &bytes[24..];
        Ok(Self { header, data })
    }
}
impl<'a> DataBlock<'a> {
    /// Iterate over raw records of fixed size.
    /// If the data block contains padding at the end, it’s your caller’s responsibility to trim that.
    ///
    /// # Arguments
    /// * `record_size` - Size in bytes of one record (including record ID)
    ///
    /// # Returns
    /// An iterator yielding each raw record slice.
    pub fn records(&self, record_size: usize) -> impl Iterator<Item = &'a [u8]> {
        self.data.chunks_exact(record_size)
    }
}
