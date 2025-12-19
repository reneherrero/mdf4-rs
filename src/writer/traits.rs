//! Abstract I/O traits for the MDF writer.
//!
//! This module provides traits that abstract away the underlying I/O implementation,
//! allowing the writer to work with both file-based I/O (with `std`) and in-memory
//! buffers (with just `alloc`).

use crate::Result;

/// Trait for write operations used by MdfWriter.
///
/// This trait abstracts the write and seek operations needed for MDF file creation.
/// It is implemented for standard file-based I/O when the `std` feature is enabled,
/// and for in-memory buffers in `no_std` environments.
pub trait MdfWrite {
    /// Write all bytes to the destination.
    fn write_all(&mut self, bytes: &[u8]) -> Result<()>;

    /// Seek to an absolute position.
    fn seek(&mut self, pos: u64) -> Result<u64>;

    /// Get the current position.
    fn position(&self) -> u64;

    /// Flush any buffered data.
    fn flush(&mut self) -> Result<()>;
}

/// A writer that writes to an in-memory buffer.
///
/// This is available in both `std` and `no_std` environments, making it useful
/// for creating MDF data in memory before writing to external storage.
pub struct VecWriter {
    buffer: alloc::vec::Vec<u8>,
    position: u64,
}

impl VecWriter {
    /// Create a new VecWriter with an empty buffer.
    pub fn new() -> Self {
        Self {
            buffer: alloc::vec::Vec::new(),
            position: 0,
        }
    }

    /// Create a new VecWriter with the given initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: alloc::vec::Vec::with_capacity(capacity),
            position: 0,
        }
    }

    /// Consume the writer and return the underlying buffer.
    pub fn into_inner(self) -> alloc::vec::Vec<u8> {
        self.buffer
    }

    /// Get a reference to the underlying buffer.
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Get the total length of the written data.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for VecWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl MdfWrite for VecWriter {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        let pos = self.position as usize;
        let end = pos + bytes.len();

        // Extend buffer if needed
        if end > self.buffer.len() {
            self.buffer.resize(end, 0);
        }

        // Copy bytes to buffer
        self.buffer[pos..end].copy_from_slice(bytes);
        self.position = end as u64;
        Ok(())
    }

    fn seek(&mut self, pos: u64) -> Result<u64> {
        self.position = pos;
        Ok(self.position)
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn flush(&mut self) -> Result<()> {
        // No-op for in-memory buffer
        Ok(())
    }
}

#[cfg(feature = "std")]
mod std_impl {
    use super::MdfWrite;
    use crate::Result;
    use std::fs::File;
    use std::io::{BufWriter, Seek, SeekFrom, Write};

    /// A wrapper that implements MdfWrite for standard file I/O.
    pub struct FileWriter {
        inner: BufWriter<File>,
        position: u64,
    }

    impl FileWriter {
        /// Create a new FileWriter for the given file path.
        pub fn new(path: &str) -> Result<Self> {
            Self::with_capacity(path, 1_048_576) // 1 MB default buffer
        }

        /// Create a new FileWriter with the specified buffer capacity.
        pub fn with_capacity(path: &str, capacity: usize) -> Result<Self> {
            let file = File::create(path)?;
            let inner = BufWriter::with_capacity(capacity, file);
            Ok(Self { inner, position: 0 })
        }
    }

    impl MdfWrite for FileWriter {
        fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
            self.inner.write_all(bytes)?;
            self.position += bytes.len() as u64;
            Ok(())
        }

        fn seek(&mut self, pos: u64) -> Result<u64> {
            self.inner.seek(SeekFrom::Start(pos))?;
            self.position = pos;
            Ok(self.position)
        }

        fn position(&self) -> u64 {
            self.position
        }

        fn flush(&mut self) -> Result<()> {
            self.inner.flush()?;
            Ok(())
        }
    }
}

#[cfg(feature = "std")]
pub use std_impl::FileWriter;
