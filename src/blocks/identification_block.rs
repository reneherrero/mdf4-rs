// identification_block.rs
use crate::{Error, Result};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::str::{self, from_utf8};

#[derive(Debug)]
pub struct IdentificationBlock {
    pub file_identifier: String,
    pub version_identifier: String,
    pub program_identifier: String,
    pub version_number: u16,
    pub standard_unfinalized_flags: u16,
    pub custom_unfinalized_flags: u16,
}

impl Default for IdentificationBlock {
    fn default() -> Self {
        IdentificationBlock {
            file_identifier: String::from("MDF     "),
            version_identifier: String::from("4.10    "), // padded to 8 bytes
            program_identifier: String::from("mdf4-rs "), // padded to 8 bytes
            version_number: 410,                          // 4.10
            standard_unfinalized_flags: 0,
            custom_unfinalized_flags: 0,
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

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Create a buffer with exact capacity
        let mut buffer = Vec::with_capacity(64);

        // 1. File identifier (8 bytes) - "MDF     " with space padding
        // According to spec: "MDF" followed by five spaces, no zero termination
        let mut file_id = [0u8; 8];
        Self::copy_string_with_padding(&self.file_identifier, &mut file_id, true); // Use space padding
        buffer.extend_from_slice(&file_id);

        // 2. Version identifier (8 bytes) - "4.10    " with space padding
        // According to spec: can be zero-terminated OR space-padded (we use space padding)
        let mut version_id = [0u8; 8];
        Self::copy_string_with_padding(&self.version_identifier, &mut version_id, true); // Use space padding
        buffer.extend_from_slice(&version_id);

        // 3. Program identifier (8 bytes) - e.g., "mdf4-rs " with space padding
        // According to spec: no zero-termination required
        let mut program_id = [0u8; 8];
        Self::copy_string_with_padding(&self.program_identifier, &mut program_id, true); // Use space padding
        buffer.extend_from_slice(&program_id);

        // 4. Reserved section (4 bytes of zeros)
        buffer.extend_from_slice(&[0u8; 4]);

        // 5. Version number as u16 (2 bytes) - e.g., 410 for version 4.10
        buffer.extend_from_slice(&self.version_number.to_le_bytes());

        // 6. Reserved section (30 bytes of zeros)
        buffer.extend_from_slice(&[0u8; 30]);

        // 7. Standard unfinalized flags (2 bytes)
        buffer.extend_from_slice(&self.standard_unfinalized_flags.to_le_bytes());

        // 8. Custom unfinalized flags (2 bytes)
        buffer.extend_from_slice(&self.custom_unfinalized_flags.to_le_bytes());

        // Verify the buffer is exactly 64 bytes
        if buffer.len() != 64 {
            return Err(Error::BlockSerializationError(format!(
                "IdentificationBlock must be exactly 64 bytes, got {}",
                buffer.len()
            )));
        }

        Ok(buffer)
    }

    /// Parse an identification block from a 64 byte slice.
    ///
    /// # Arguments
    /// * `bytes` - Slice containing the complete `##ID` block.
    ///
    /// # Returns
    /// The populated [`IdentificationBlock`] or an [`Error`] if the slice is
    /// invalid.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let expected_bytes = 64;
        if bytes.len() < expected_bytes {
            return Err(Error::TooShortBuffer {
                actual: bytes.len(),
                expected: expected_bytes,
                file: file!(),
                line: line!(),
            });
        }

        let file_identifier = str::from_utf8(&bytes[0..8]).unwrap().to_string();
        // Accept both finalized ("MDF     ") and unfinalized ("UnFinMF ") files
        if file_identifier != "MDF     " && file_identifier != "UnFinMF " {
            return Err(Error::FileIdentifierError(file_identifier));
        }

        let (major, minor) = Self::parse_block_version(&bytes[8..16])?;
        let version_u16 = major * 100 + minor;

        if version_u16 < 410 {
            return Err(Error::FileVersioningError(version_u16.to_string()));
        }

        Ok(Self {
            file_identifier,
            version_identifier: String::from(str::from_utf8(&bytes[8..16]).unwrap()),
            program_identifier: String::from(str::from_utf8(&bytes[16..24]).unwrap()),
            // Reserved bytes between 24 and 28 are skipped
            // The version number immediately follows at bytes 28..30
            version_number: u16::from_le_bytes(bytes[28..30].try_into().unwrap()),
            // Reserved bytes between 31 and 60 are skipped
            standard_unfinalized_flags: u16::from_le_bytes(bytes[60..62].try_into().unwrap()),
            custom_unfinalized_flags: u16::from_le_bytes(bytes[62..64].try_into().unwrap()),
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
