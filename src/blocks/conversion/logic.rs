use super::{bitfield, linear, table_lookup, text};
use crate::Result;
use crate::blocks::conversion::base::ConversionBlock;
use crate::blocks::conversion::types::ConversionType;
use crate::types::DecodedValue;

impl ConversionBlock {
    /// Applies the conversion formula to a decoded channel value.
    ///
    /// Depending on the conversion type, this method either returns a numeric value
    /// (wrapped as DecodedValue::Float) or a character string (wrapped as DecodedValue::String).
    /// For non-numeric conversions such as Algebraic or Table look-ups, placeholder implementations
    /// are provided and can be extended later.
    ///
    /// # Parameters
    /// * `value`: The already-decoded channel value (as DecodedValue).
    ///
    /// # Returns
    /// A DecodedValue where numeric conversion types yield a Float and string conversion types yield a String.
    pub fn apply_decoded(&self, value: DecodedValue, file_data: &[u8]) -> Result<DecodedValue> {
        match self.conversion_type {
            ConversionType::Identity => Ok(value),
            ConversionType::Linear => linear::apply_linear(self, value),
            ConversionType::Rational => linear::apply_rational(self, value),
            ConversionType::Algebraic => linear::apply_algebraic(self, value),
            ConversionType::TableLookupInterp => {
                table_lookup::apply_table_lookup(self, value, true)
            }
            ConversionType::TableLookupNoInterp => {
                table_lookup::apply_table_lookup(self, value, false)
            }
            ConversionType::RangeLookup => table_lookup::apply_range_lookup(self, value),
            ConversionType::ValueToText => text::apply_value_to_text(self, value, file_data),
            ConversionType::RangeToText => text::apply_range_to_text(self, value, file_data),
            ConversionType::TextToValue => text::apply_text_to_value(self, value, file_data),
            ConversionType::TextToText => text::apply_text_to_text(self, value, file_data),
            ConversionType::BitfieldText => bitfield::apply_bitfield_text(self, value, file_data),
            ConversionType::Unknown(_) => Ok(value),
        }
    }
}
