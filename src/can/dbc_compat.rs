//! DBC to MDF4 type conversion utilities.
//!
//! This module provides functions to convert between dbc-rs types and mdf4-rs types,
//! enabling seamless integration between CAN database definitions and MDF4 files.
//!
//! # Overview
//!
//! When logging CAN data to MDF4 files, signal metadata from DBC files can be
//! preserved in the MDF4 structure. This module handles the conversion of:
//!
//! - Signal data types (byte order, signedness, bit width)
//! - Conversion formulas (factor/offset to linear conversion blocks)
//! - Value descriptions (VAL_ entries to value-to-text conversions)
//! - Signal limits (min/max values)
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::can::dbc_compat::{signal_to_data_type, signal_to_conversion};
//!
//! let signal: &dbc_rs::Signal = /* ... */;
//!
//! // Get the appropriate MDF4 data type for this signal
//! let data_type = signal_to_data_type(signal);
//!
//! // Get the conversion block (if needed)
//! if let Some(conv) = signal_to_conversion(signal) {
//!     // Apply to MDF4 channel
//! }
//! ```

use crate::DataType;
use crate::blocks::ConversionBlock;

/// Determines the appropriate MDF4 DataType for a DBC signal.
///
/// This considers:
/// - Byte order (little-endian vs big-endian)
/// - Signedness (signed vs unsigned)
/// - Bit width (to choose appropriate integer size)
///
/// # Arguments
/// * `signal` - The DBC signal to analyze
///
/// # Returns
/// The most appropriate `DataType` for storing this signal's raw values.
///
/// # Note
/// For signals wider than 64 bits, this returns a byte array type.
/// For practical CAN signals (max 64 bits), it returns integer types.
pub fn signal_to_data_type(signal: &dbc_rs::Signal) -> DataType {
    let length = signal.length();
    let is_le = signal.byte_order() == dbc_rs::ByteOrder::LittleEndian;
    let is_unsigned = signal.is_unsigned();

    // Determine the appropriate integer width
    // MDF4 supports 8, 16, 32, 64 bit integers
    match (is_unsigned, is_le, length) {
        // Unsigned Little Endian
        (true, true, 1..=8) => DataType::UnsignedIntegerLE,
        (true, true, 9..=16) => DataType::UnsignedIntegerLE,
        (true, true, 17..=32) => DataType::UnsignedIntegerLE,
        (true, true, 33..=64) => DataType::UnsignedIntegerLE,

        // Unsigned Big Endian
        (true, false, 1..=8) => DataType::UnsignedIntegerBE,
        (true, false, 9..=16) => DataType::UnsignedIntegerBE,
        (true, false, 17..=32) => DataType::UnsignedIntegerBE,
        (true, false, 33..=64) => DataType::UnsignedIntegerBE,

        // Signed Little Endian
        (false, true, 1..=8) => DataType::SignedIntegerLE,
        (false, true, 9..=16) => DataType::SignedIntegerLE,
        (false, true, 17..=32) => DataType::SignedIntegerLE,
        (false, true, 33..=64) => DataType::SignedIntegerLE,

        // Signed Big Endian
        (false, false, 1..=8) => DataType::SignedIntegerBE,
        (false, false, 9..=16) => DataType::SignedIntegerBE,
        (false, false, 17..=32) => DataType::SignedIntegerBE,
        (false, false, 33..=64) => DataType::SignedIntegerBE,

        // Fallback for unusual cases (shouldn't happen with valid CAN signals)
        _ => DataType::ByteArray,
    }
}

/// Determines the bit count for storing a DBC signal's raw value.
///
/// Returns the minimum byte-aligned bit count needed to store the signal.
/// MDF4 channels typically use 8, 16, 32, or 64 bits.
///
/// # Arguments
/// * `signal` - The DBC signal to analyze
///
/// # Returns
/// The bit count for the MDF4 channel (8, 16, 32, or 64).
pub fn signal_to_bit_count(signal: &dbc_rs::Signal) -> u32 {
    let length = signal.length();
    match length {
        0..=8 => 8,
        9..=16 => 16,
        17..=32 => 32,
        _ => 64,
    }
}

/// Creates an MDF4 conversion block from a DBC signal's factor and offset.
///
/// DBC signals use the formula: `physical = offset + factor * raw`
/// This maps directly to MDF4's linear conversion type.
///
/// # Arguments
/// * `signal` - The DBC signal containing factor and offset
///
/// # Returns
/// - `Some(ConversionBlock)` if a conversion is needed (factor != 1 or offset != 0)
/// - `None` if the signal has identity conversion (factor = 1, offset = 0)
///
/// # Example
/// ```ignore
/// let signal = /* DBC signal with factor=0.25, offset=0 */;
/// if let Some(conv) = signal_to_conversion(&signal) {
///     writer.set_channel_conversion(&channel_id, &conv)?;
/// }
/// ```
pub fn signal_to_conversion(signal: &dbc_rs::Signal) -> Option<ConversionBlock> {
    let factor = signal.factor();
    let offset = signal.offset();

    // Skip identity conversions
    if factor == 1.0 && offset == 0.0 {
        return None;
    }

    Some(ConversionBlock::linear(offset, factor))
}

/// Creates an MDF4 linear conversion with physical range limits from a DBC signal.
///
/// This is similar to `signal_to_conversion` but also includes the signal's
/// min/max range in the conversion block's physical range fields.
///
/// # Arguments
/// * `signal` - The DBC signal containing factor, offset, min, and max
///
/// # Returns
/// - `Some(ConversionBlock)` with physical range if conversion is needed
/// - `None` if the signal has identity conversion
pub fn signal_to_conversion_with_range(signal: &dbc_rs::Signal) -> Option<ConversionBlock> {
    let factor = signal.factor();
    let offset = signal.offset();
    let min = signal.min();
    let max = signal.max();

    // Skip identity conversions with no meaningful range
    if factor == 1.0 && offset == 0.0 && min == 0.0 && max == 0.0 {
        return None;
    }

    let mut conv = ConversionBlock::linear(offset, factor);

    // Add physical range if specified
    if min != 0.0 || max != 0.0 {
        conv = conv.with_physical_range(min, max);
    }

    Some(conv)
}

/// Information about a DBC signal for MDF4 channel creation.
///
/// This struct bundles all the information needed to create an MDF4 channel
/// from a DBC signal definition.
#[derive(Debug, Clone)]
pub struct SignalInfo {
    /// Signal name
    pub name: alloc::string::String,
    /// Physical unit (e.g., "rpm", "°C")
    pub unit: Option<alloc::string::String>,
    /// MDF4 data type for raw values
    pub data_type: DataType,
    /// Bit count for raw value storage
    pub bit_count: u32,
    /// Linear conversion (if needed)
    pub conversion: Option<ConversionBlock>,
    /// Physical minimum value
    pub min: f64,
    /// Physical maximum value
    pub max: f64,
    /// DBC factor for manual decoding
    pub factor: f64,
    /// DBC offset for manual decoding
    pub offset: f64,
    /// Whether the signal is unsigned
    pub unsigned: bool,
}

impl SignalInfo {
    /// Creates SignalInfo from a DBC signal.
    ///
    /// Extracts all relevant information from the signal and converts
    /// it to MDF4-compatible formats.
    pub fn from_signal(signal: &dbc_rs::Signal) -> Self {
        Self {
            name: alloc::string::String::from(signal.name()),
            unit: signal.unit().map(alloc::string::String::from),
            data_type: signal_to_data_type(signal),
            bit_count: signal_to_bit_count(signal),
            conversion: signal_to_conversion_with_range(signal),
            min: signal.min(),
            max: signal.max(),
            factor: signal.factor(),
            offset: signal.offset(),
            unsigned: signal.is_unsigned(),
        }
    }

    /// Check if this signal needs a conversion block.
    pub fn needs_conversion(&self) -> bool {
        self.conversion.is_some()
    }

    /// Check if this is an identity conversion (factor=1, offset=0).
    pub fn is_identity(&self) -> bool {
        self.factor == 1.0 && self.offset == 0.0
    }

    /// Convert a raw integer value to physical value.
    #[inline]
    pub fn raw_to_physical(&self, raw: i64) -> f64 {
        self.offset + self.factor * (raw as f64)
    }

    /// Convert a physical value to raw integer value.
    #[inline]
    pub fn physical_to_raw(&self, physical: f64) -> i64 {
        let raw = (physical - self.offset) / self.factor;
        // Round to nearest integer (no_std compatible)
        if raw >= 0.0 {
            (raw + 0.5) as i64
        } else {
            (raw - 0.5) as i64
        }
    }
}

/// Information about a DBC message for MDF4 channel group creation.
#[derive(Debug, Clone)]
pub struct MessageInfo {
    /// CAN message ID
    pub id: u32,
    /// Message name
    pub name: alloc::string::String,
    /// Data length code (bytes)
    pub dlc: u8,
    /// Transmitting ECU name
    pub sender: alloc::string::String,
    /// Signal information for each signal in the message
    pub signals: alloc::vec::Vec<SignalInfo>,
    /// Whether this is an extended (29-bit) CAN ID
    pub is_extended: bool,
}

impl MessageInfo {
    /// Creates MessageInfo from a DBC message.
    pub fn from_message(message: &dbc_rs::Message) -> Self {
        let id = message.id();
        let is_extended = (id & 0x8000_0000) != 0;
        let raw_id = if is_extended { id & 0x1FFF_FFFF } else { id };

        Self {
            id: raw_id,
            name: alloc::string::String::from(message.name()),
            dlc: message.dlc(),
            sender: alloc::string::String::from(message.sender()),
            signals: message
                .signals()
                .iter()
                .map(SignalInfo::from_signal)
                .collect(),
            is_extended,
        }
    }

    /// Get the number of signals in this message.
    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }
}

/// Extract all message information from a DBC database.
///
/// This is useful for pre-processing DBC data before logging.
pub fn extract_message_info(dbc: &dbc_rs::Dbc) -> alloc::vec::Vec<MessageInfo> {
    dbc.messages()
        .iter()
        .filter(|m| m.signals().iter().next().is_some()) // Only messages with signals
        .map(MessageInfo::from_message)
        .collect()
}

use alloc::string::String;
use alloc::vec::Vec;

/// Creates an MDF4 value-to-text conversion from DBC value descriptions.
///
/// DBC files can contain VAL_ entries that map integer values to text strings.
/// This converts them to MDF4's ValueToText conversion type.
///
/// # Arguments
/// * `descriptions` - Iterator over (value, text) pairs
/// * `default_text` - Text to use for values not in the mapping
///
/// # Returns
/// A tuple of (mapping as Vec, default text) suitable for `add_value_to_text_conversion`
pub fn value_descriptions_to_mapping<'a>(
    descriptions: impl Iterator<Item = (i64, &'a str)>,
    default_text: &str,
) -> (Vec<(i64, String)>, String) {
    let mapping: Vec<(i64, String)> = descriptions.map(|(v, t)| (v, String::from(t))).collect();
    (mapping, String::from(default_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_to_data_type_unsigned_le() {
        // Create a minimal DBC and parse a signal
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"
BU_:
BO_ 100 TestMsg: 8 Vector__XXX
 SG_ TestSig : 0|16@1+ (1,0) [0|65535] "" Vector__XXX
"#,
        )
        .unwrap();

        let msg = dbc.messages().find_by_id(100).unwrap();
        let sig = msg.signals().find("TestSig").unwrap();

        let dt = signal_to_data_type(sig);
        assert!(matches!(dt, DataType::UnsignedIntegerLE));
    }

    #[test]
    fn test_signal_to_conversion_identity() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"
BU_:
BO_ 100 TestMsg: 8 Vector__XXX
 SG_ TestSig : 0|16@1+ (1,0) [0|65535] "" Vector__XXX
"#,
        )
        .unwrap();

        let msg = dbc.messages().find_by_id(100).unwrap();
        let sig = msg.signals().find("TestSig").unwrap();

        // Factor=1, offset=0 should return None (identity)
        assert!(signal_to_conversion(sig).is_none());
    }

    #[test]
    fn test_signal_to_conversion_linear() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"
BU_:
BO_ 100 TestMsg: 8 Vector__XXX
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
"#,
        )
        .unwrap();

        let msg = dbc.messages().find_by_id(100).unwrap();
        let sig = msg.signals().find("RPM").unwrap();

        let conv = signal_to_conversion(sig);
        assert!(conv.is_some());

        let conv = conv.unwrap();
        assert_eq!(conv.cc_val.len(), 2);
        assert_eq!(conv.cc_val[0], 0.0); // offset
        assert_eq!(conv.cc_val[1], 0.25); // factor
    }

    #[test]
    fn test_signal_info_conversion() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"
BU_:
BO_ 100 TestMsg: 8 Vector__XXX
 SG_ Temp : 0|8@1- (0.5,-40) [-40|85] "C" Vector__XXX
"#,
        )
        .unwrap();

        let msg = dbc.messages().find_by_id(100).unwrap();
        let sig = msg.signals().find("Temp").unwrap();

        let info = SignalInfo::from_signal(sig);

        assert_eq!(info.name, "Temp");
        assert_eq!(info.unit, Some(String::from("C")));
        assert_eq!(info.factor, 0.5);
        assert_eq!(info.offset, -40.0);
        assert_eq!(info.min, -40.0);
        assert_eq!(info.max, 85.0);
        assert!(!info.unsigned);

        // Test conversion
        assert_eq!(info.raw_to_physical(0), -40.0);
        assert_eq!(info.raw_to_physical(160), 40.0); // 0.5 * 160 - 40 = 40
    }

    #[test]
    fn test_message_info() {
        let dbc = dbc_rs::Dbc::parse(
            r#"VERSION "1.0"
BU_: ECM
BO_ 256 Engine: 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
 SG_ Temp : 16|8@1- (1,-40) [-40|215] "C" Vector__XXX
"#,
        )
        .unwrap();

        let msg = dbc.messages().find_by_id(256).unwrap();
        let info = MessageInfo::from_message(msg);

        assert_eq!(info.id, 256);
        assert_eq!(info.name, "Engine");
        assert_eq!(info.dlc, 8);
        assert_eq!(info.sender, "ECM");
        assert_eq!(info.signal_count(), 2);
        assert!(!info.is_extended);
    }
}
