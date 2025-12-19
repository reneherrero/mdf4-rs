// Low level file and block handling utilities for MdfWriter
use alloc::format;
use alloc::string::ToString;
use alloc::vec;

use super::{MdfWrite, MdfWriter};
use crate::{Error, Result};

#[cfg(feature = "std")]
use super::FileWriter;

#[cfg(feature = "std")]
impl MdfWriter<FileWriter> {
    /// Creates a new MdfWriter for the given file path using a 1 MB internal
    /// buffer. Use [`Self::new_with_capacity`] to customize the buffer size.
    pub fn new(path: &str) -> Result<Self> {
        Self::new_with_capacity(path, 1_048_576)
    }

    /// Creates a new MdfWriter with the specified `BufWriter` capacity.
    ///
    /// # Arguments
    /// * `path` - Path to the output file
    /// * `capacity` - Buffer size in bytes (default is 1 MB)
    ///
    /// # Example
    /// ```no_run
    /// use mdf4_rs::MdfWriter;
    ///
    /// // Use a 4 MB buffer for better performance with large files
    /// let writer = MdfWriter::new_with_capacity("output.mf4", 4 * 1024 * 1024)?;
    /// # Ok::<(), mdf4_rs::Error>(())
    /// ```
    pub fn new_with_capacity(path: &str, capacity: usize) -> Result<Self> {
        let file_writer = FileWriter::with_capacity(path, capacity)?;
        Ok(Self::from_writer(file_writer))
    }
}

impl<W: MdfWrite> MdfWriter<W> {
    /// Writes a block to the file, aligning to 8 bytes and zero-padding as needed.
    /// Returns the starting offset of the block in the file.
    pub fn write_block(&mut self, block_bytes: &[u8]) -> Result<u64> {
        let align = (8 - (self.offset % 8)) % 8;
        if align != 0 {
            let padding = vec![0u8; align as usize];
            self.writer.write_all(&padding)?;
            self.offset += align;
        }

        self.writer.write_all(block_bytes)?;
        let block_start = self.offset;
        self.offset += block_bytes.len() as u64;
        Ok(block_start)
    }

    /// Writes a block to the file and tracks its position with the given ID.
    pub fn write_block_with_id(&mut self, block_bytes: &[u8], block_id: &str) -> Result<u64> {
        let block_start = self.write_block(block_bytes)?;
        self.block_positions
            .insert(block_id.to_string(), block_start);
        Ok(block_start)
    }

    /// Retrieves the file position of a previously written block.
    pub fn get_block_position(&self, block_id: &str) -> Option<u64> {
        self.block_positions.get(block_id).copied()
    }

    /// Updates a link (u64 address) at a specific offset in the file.
    pub fn update_link(&mut self, offset: u64, address: u64) -> Result<()> {
        let current_pos = self.offset;
        self.writer.seek(offset)?;
        self.writer.write_all(&address.to_le_bytes())?;
        self.writer.seek(current_pos)?;
        Ok(())
    }

    /// Updates a link using block IDs instead of raw offsets.
    pub fn update_block_link(
        &mut self,
        source_id: &str,
        link_offset: u64,
        target_id: &str,
    ) -> Result<()> {
        let source_pos = self.get_block_position(source_id).ok_or_else(|| {
            Error::BlockLinkError(format!("Source block '{}' not found", source_id))
        })?;
        let target_pos = self.get_block_position(target_id).ok_or_else(|| {
            Error::BlockLinkError(format!("Target block '{}' not found", target_id))
        })?;
        let link_pos = source_pos + link_offset;
        self.update_link(link_pos, target_pos)
    }

    fn update_u32(&mut self, offset: u64, value: u32) -> Result<()> {
        let current_pos = self.offset;
        self.writer.seek(offset)?;
        self.writer.write_all(&value.to_le_bytes())?;
        self.writer.seek(current_pos)?;
        Ok(())
    }

    fn update_u64(&mut self, offset: u64, value: u64) -> Result<()> {
        let current_pos = self.offset;
        self.writer.seek(offset)?;
        self.writer.write_all(&value.to_le_bytes())?;
        self.writer.seek(current_pos)?;
        Ok(())
    }

    fn update_u8(&mut self, offset: u64, value: u8) -> Result<()> {
        let current_pos = self.offset;
        self.writer.seek(offset)?;
        self.writer.write_all(&[value])?;
        self.writer.seek(current_pos)?;
        Ok(())
    }

    pub(super) fn update_block_u32(
        &mut self,
        block_id: &str,
        field_offset: u64,
        value: u32,
    ) -> Result<()> {
        let block_pos = self
            .get_block_position(block_id)
            .ok_or_else(|| Error::BlockLinkError(format!("Block '{}' not found", block_id)))?;
        self.update_u32(block_pos + field_offset, value)
    }

    pub(super) fn update_block_u8(
        &mut self,
        block_id: &str,
        field_offset: u64,
        value: u8,
    ) -> Result<()> {
        let block_pos = self
            .get_block_position(block_id)
            .ok_or_else(|| Error::BlockLinkError(format!("Block '{}' not found", block_id)))?;
        self.update_u8(block_pos + field_offset, value)
    }

    pub(super) fn update_block_u64(
        &mut self,
        block_id: &str,
        field_offset: u64,
        value: u64,
    ) -> Result<()> {
        let block_pos = self
            .get_block_position(block_id)
            .ok_or_else(|| Error::BlockLinkError(format!("Block '{}' not found", block_id)))?;
        self.update_u64(block_pos + field_offset, value)
    }

    /// Returns the current file offset (for block address calculation).
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Finalizes the file (flushes all data to disk).
    pub fn finalize(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}
