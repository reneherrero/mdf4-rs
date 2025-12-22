// identification_block.rs
use super::ID_BLOCK_SIZE;
use crate::{
    Error, Result,
    blocks::common::{debug_assert_aligned, read_u16, validate_buffer_size},
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::str::{self, from_utf8};

/// Identification Block - file format identifier at the start of every MDF file.
///
/// The identification block is always located at file offset 0 and identifies
/// the file as an MDF file, along with version information.
#[derive(Debug, Clone)]
pub struct IdentificationBlock {
    /// File identifier string ("MDF     " or "UnFinMF ").
    pub file_id: String,
    /// Format version string (e.g., "4.10    ").
    pub format_version: String,
    /// Program identifier string (tool that created the file).
    pub program_id: String,
    /// Numeric version (e.g., 410 for version 4.10).
    pub version_number: u16,
    /// Standard unfinalized flags (indicates incomplete sections).
    pub unfinalized_flags: u16,
    /// Custom unfinalized flags (vendor-specific).
    pub custom_flags: u16,
}

impl Default for IdentificationBlock {
    fn default() -> Self {
        Self {
            file_id: String::from("MDF     "),
            format_version: String::from("4.10    "),
            program_id: String::from("mdf4-rs "),
            version_number: 410,
            unfinalized_flags: 0,
            custom_flags: 0,
        }
    }
}

impl IdentificationBlock {
    /// Serializes the IdentificationBlock to bytes according to MDF 4.1 specification.
    ///
    /// # Structure (64 bytes total):
    /// - File identifier: 8 bytes (typically "MDF     " with spaces)
    /// - Version identifier: 8 bytes (typically "4.10    " with spaces)
    /// - Program identifier: 8 bytes (typically program name with spaces)
    /// - Reserved: 4 bytes (zeros)
    /// - Version number: 2 bytes (e.g., 410 for version 4.10)
    /// - Reserved: 30 bytes (zeros)
    /// - Standard flags: 2 bytes (unfinalized flags)
    /// - Custom flags: 2 bytes (unfinalized custom flags)
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` containing the serialized identification block
    /// - `Err(MdfError)` if serialization fails
    ///
    /// # Note
    /// String fields are padded with nulls (0x00) if shorter than required length,
    /// and truncated if longer.
    /// Helper function to copy a string to a fixed-size byte array with specified padding
    ///
    /// According to MDF 4.1 specification:
    /// - File identifier (id_file): "MDF     " (5 spaces, no zero termination)
    /// - Version identifier (id_vers): "4.10    " (4 spaces, no zero termination) OR "4.10\0..." (zero terminated)
    /// - Program identifier (id_prog): No zero-termination required (we'll use space padding)
    fn copy_string_with_padding(source: &str, target: &mut [u8], use_space_padding: bool) {
        // Copy string bytes up to target length
        let src_bytes = source.as_bytes();
        let copy_len = core::cmp::min(src_bytes.len(), target.len());
        target[..copy_len].copy_from_slice(&src_bytes[..copy_len]);

        // Apply padding if needed
        if copy_len < target.len() {
            let padding_byte = if use_space_padding { b' ' } else { 0u8 };
            for byte in target.iter_mut().skip(copy_len) {
                *byte = padding_byte;
            }
        }
    }

    /// Serializes the IdentificationBlock to bytes according to MDF 4.1 specification.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(ID_BLOCK_SIZE);

        // File identifier (8 bytes)
        let mut file_id = [0u8; 8];
        Self::copy_string_with_padding(&self.file_id, &mut file_id, true);
        buffer.extend_from_slice(&file_id);

        // Format version (8 bytes)
        let mut version_id = [0u8; 8];
        Self::copy_string_with_padding(&self.format_version, &mut version_id, true);
        buffer.extend_from_slice(&version_id);

        // Program identifier (8 bytes)
        let mut program_id = [0u8; 8];
        Self::copy_string_with_padding(&self.program_id, &mut program_id, true);
        buffer.extend_from_slice(&program_id);

        // Reserved (4 bytes)
        buffer.extend_from_slice(&[0u8; 4]);

        // Version number (2 bytes)
        buffer.extend_from_slice(&self.version_number.to_le_bytes());

        // Reserved (30 bytes)
        buffer.extend_from_slice(&[0u8; 30]);

        // Unfinalized flags (2 bytes)
        buffer.extend_from_slice(&self.unfinalized_flags.to_le_bytes());

        // Custom flags (2 bytes)
        buffer.extend_from_slice(&self.custom_flags.to_le_bytes());

        debug_assert_aligned(buffer.len());
        Ok(buffer)
    }

    /// Parses an identification block from a 64 byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        validate_buffer_size(bytes, ID_BLOCK_SIZE)?;

        let file_id = str::from_utf8(&bytes[0..8])
            .map(String::from)
            .unwrap_or_else(|_| String::from_utf8_lossy(&bytes[0..8]).into_owned());

        // Accept both finalized ("MDF     ") and unfinalized ("UnFinMF ") files
        if file_id != "MDF     " && file_id != "UnFinMF " {
            return Err(Error::FileIdentifierError(file_id));
        }

        let (major, minor) = Self::parse_block_version(&bytes[8..16])?;
        let version_u16 = major * 100 + minor;

        if version_u16 < 410 {
            return Err(Error::FileVersioningError(version_u16.to_string()));
        }

        Ok(Self {
            file_id,
            format_version: str::from_utf8(&bytes[8..16])
                .map(String::from)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes[8..16]).into_owned()),
            program_id: str::from_utf8(&bytes[16..24])
                .map(String::from)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes[16..24]).into_owned()),
            version_number: read_u16(bytes, 28),
            unfinalized_flags: read_u16(bytes, 60),
            custom_flags: read_u16(bytes, 62),
        })
    }
    /// Parse the textual version stored in the identification block.
    ///
    /// # Arguments
    /// * `bytes` - Eight bytes containing the version string, e.g. `"4.10\0"`.
    ///
    /// # Returns
    /// `(major, minor)` on success or an [`Error`] when the format is
    /// unexpected.
    pub fn parse_block_version(bytes: &[u8]) -> Result<(u16, u16)> {
        // 1) Decode to &str, ignoring invalid UTF-8 (there shouldn’t be any).
        let raw = from_utf8(bytes)
            .map_err(|_| Error::InvalidVersionString("Invalid UTF-8".to_string()))?;

        // 2) Trim trailing nulls and spaces
        let s = raw.trim_end_matches(char::from(0)).trim();
        // 3) Split on the dot
        let mut parts = s.split('.');
        let maj = parts
            .next()
            .ok_or_else(|| Error::InvalidVersionString("Missing major version".to_string()))?
            .parse::<u16>()
            .map_err(|_| Error::InvalidVersionString("Invalid major version string".to_string()))?;
        let min =
            parts.next().unwrap_or("0").parse::<u16>().map_err(|_| {
                Error::InvalidVersionString("Invalid minor version string".to_string())
            })?;
        Ok((maj, min))
    }
}
