//! Error types for MDF4 operations.
//!
//! This module defines the [`Error`] enum which represents all possible failures
//! that can occur when reading, writing, or processing MDF files.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "std")]
//! use mdf4_rs::{MDF, Error, Result};
//!
//! # #[cfg(feature = "std")]
//! fn process_file(path: &str) -> Result<()> {
//!     match MDF::from_file(path) {
//!         Ok(mdf) => {
//!             println!("Loaded {} channel groups", mdf.channel_groups().len());
//!             Ok(())
//!         }
//!         Err(Error::IOError(e)) => {
//!             eprintln!("File I/O error: {}", e);
//!             Err(Error::IOError(e))
//!         }
//!         Err(Error::FileIdentifierError(id)) => {
//!             eprintln!("Not a valid MDF file: {}", id);
//!             Err(Error::FileIdentifierError(id))
//!         }
//!         Err(e) => Err(e),
//!     }
//! }
//! ```

use core::fmt;

#[cfg(feature = "alloc")]
use alloc::string::String;

/// Errors that can occur during MDF file operations.
///
/// This enum covers all failure modes including I/O errors, parsing failures,
/// and structural issues in the MDF file.
#[derive(Debug)]
pub enum Error {
    /// Buffer provided for parsing was too small.
    ///
    /// This typically indicates file corruption or an incomplete read.
    TooShortBuffer {
        /// Actual number of bytes available
        actual: usize,
        /// Minimum number of bytes required
        expected: usize,
        /// Source file where the error was detected
        file: &'static str,
        /// Line number where the error was detected
        line: u32,
    },

    /// The file identifier is not "MDF     " as required by the specification.
    ///
    /// This can occur when trying to open a non-MDF file or a file using an
    /// unsupported variant like "UnFinMF" (unfinalized MDF).
    FileIdentifierError(String),

    /// The MDF version is not supported (requires 4.1 or later).
    FileVersioningError(String),

    /// A block identifier did not match the expected value.
    ///
    /// Each MDF block starts with a 4-character identifier (e.g., "##HD" for
    /// the header block). This error indicates structural corruption.
    BlockIDError {
        /// The identifier that was found
        actual: String,
        /// The identifier that was expected
        expected: String,
    },

    /// An I/O error occurred while reading or writing the file.
    ///
    /// Only available with the `std` feature.
    #[cfg(feature = "std")]
    IOError(std::io::Error),

    /// A write operation failed (no_std version).
    ///
    /// Only available without the `std` feature.
    #[cfg(not(feature = "std"))]
    WriteError,

    /// The version string in the identification block could not be parsed.
    InvalidVersionString(String),

    /// Failed to link blocks together during file writing.
    ///
    /// This typically indicates a programming error where blocks are
    /// referenced before being written.
    BlockLinkError(String),

    /// Failed to serialize a block to bytes.
    BlockSerializationError(String),

    /// A conversion chain exceeded the maximum allowed depth.
    ///
    /// MDF supports chained conversions where one conversion references another.
    /// This error prevents infinite loops from malformed files.
    ConversionChainTooDeep {
        /// The maximum depth that was exceeded
        max_depth: usize,
    },

    /// A cycle was detected in a conversion chain.
    ///
    /// This indicates file corruption where conversion blocks form a loop.
    ConversionChainCycle {
        /// The address where the cycle was detected
        address: u64,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShortBuffer {
                actual,
                expected,
                file,
                line,
            } => write!(
                f,
                "Buffer too small at {file}:{line}: need at least {expected} bytes, got {actual}"
            ),
            Error::FileIdentifierError(id) => {
                write!(
                    f,
                    r#"Invalid file identifier: Expected "MDF     ", found {id}"#
                )
            }
            Error::FileVersioningError(ver) => {
                write!(f, r#"File version too low: Expected "> 4.1", found {ver}"#)
            }
            Error::BlockIDError { actual, expected } => {
                write!(
                    f,
                    "Invalid block identifier: Expected {expected:?}, got {actual:?}"
                )
            }
            #[cfg(feature = "std")]
            Error::IOError(e) => write!(f, "I/O error: {e}"),
            #[cfg(not(feature = "std"))]
            Error::WriteError => write!(f, "Write error"),
            Error::InvalidVersionString(s) => write!(f, "Invalid version string: {s}"),
            Error::BlockLinkError(s) => write!(f, "Block linking error: {s}"),
            Error::BlockSerializationError(s) => write!(f, "Block serialization error: {s}"),
            Error::ConversionChainTooDeep { max_depth } => {
                write!(
                    f,
                    "Conversion chain too deep: maximum depth of {max_depth} exceeded"
                )
            }
            Error::ConversionChainCycle { address } => {
                write!(
                    f,
                    "Conversion chain cycle detected at block address {address:#x}"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::IOError(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IOError(err)
    }
}

/// A specialized Result type for MDF operations.
///
/// This is defined as `core::result::Result<T, Error>` for convenience.
pub type Result<T> = core::result::Result<T, Error>;
