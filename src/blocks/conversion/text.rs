use super::linear::extract_numeric;
use crate::Result;
use crate::blocks::common::{BlockHeader, BlockParse, read_string_block};
use crate::blocks::conversion::base::ConversionBlock;
use crate::types::DecodedValue;

/// Given `cc_val = [min0, max0, min1, max1, …]`, return the first index where
/// `raw` falls into `[min_i, max_i]`.
/// If no range matches, returns `n` (the default index).
pub fn find_range_to_text_index(cc_val: &[f64], raw: f64, inclusive_upper: bool) -> usize {
    let len = cc_val.len();
    if len < 2 || len % 2 != 0 {
        return 0;
    }
    let n = len / 2;
    for i in 0..n {
        let min = cc_val[2 * i];
        let max = cc_val[2 * i + 1];
        if inclusive_upper {
            if raw >= min && raw <= max {
                return i;
            }
        } else if raw >= min && raw < max {
            return i;
        }
    }
    n
}

pub fn apply_value_to_text(
    block: &ConversionBlock,
    value: DecodedValue,
    file_data: &[u8],
) -> Result<DecodedValue> {
    let raw = match extract_numeric(&value) {
        Some(x) => x,
        None => return Ok(value),
    };
    let idx = block
        .cc_val
        .iter()
        .position(|&k| k == raw)
        .unwrap_or(block.cc_val.len());

    // First try to use resolved data if available (std only)
    #[cfg(feature = "std")]
    {
        if let Some(resolved_text) = block.get_resolved_text(idx) {
            return Ok(DecodedValue::String(resolved_text.clone()));
        }

        if let Some(resolved_conversion) = block.get_resolved_conversion(idx) {
            return resolved_conversion.apply_decoded(value, &[]);
        }
    }

    // If no match found and we have a default conversion, use it
    if idx >= block.cc_val.len()
        && let Some(default_conversion) = block.get_default_conversion()
    {
        return default_conversion.apply_decoded(value, &[]);
    }

    // Fallback to legacy behavior if no resolved data (for backward compatibility)
    let link = *block.cc_ref.get(idx).unwrap_or(&0);
    if link == 0 {
        // Try default conversion as final fallback
        if let Some(default_conversion) = block.get_default_conversion() {
            return default_conversion.apply_decoded(value, &[]);
        }
        return Ok(DecodedValue::Unknown);
    }

    let off = link as usize;
    if off + 24 > file_data.len() {
        // Try default conversion if link is invalid
        if let Some(default_conversion) = block.get_default_conversion() {
            return default_conversion.apply_decoded(value, &[]);
        }
        return Ok(DecodedValue::Unknown);
    }

    let hdr = BlockHeader::from_bytes(&file_data[off..off + 24])?;
    if hdr.id == "##TX" {
        if let Some(txt) = read_string_block(file_data, link)? {
            return Ok(DecodedValue::String(txt));
        }
        // Try default conversion if text block read failed
        if let Some(default_conversion) = block.get_default_conversion() {
            return default_conversion.apply_decoded(value, &[]);
        }
        return Ok(DecodedValue::Unknown);
    }
    if hdr.id == "##CC" {
        let mut nested = ConversionBlock::from_bytes(&file_data[off..])?;
        let _ = nested.resolve_formula(file_data);
        return nested.apply_decoded(value, file_data);
    }

    // Try default conversion for unrecognized block types
    if let Some(default_conversion) = block.get_default_conversion() {
        return default_conversion.apply_decoded(value, &[]);
    }

    Ok(DecodedValue::Unknown)
}

pub fn apply_range_to_text(
    block: &ConversionBlock,
    value: DecodedValue,
    file_data: &[u8],
) -> Result<DecodedValue> {
    let raw = match extract_numeric(&value) {
        Some(x) => x,
        None => return Ok(value),
    };
    let inclusive_upper = matches!(
        value,
        DecodedValue::UnsignedInteger(_) | DecodedValue::SignedInteger(_)
    );
    let idx = find_range_to_text_index(&block.cc_val, raw, inclusive_upper);
    let n_ranges = block.cc_val.len() / 2;

    // First try to use resolved data if available (std only)
    #[cfg(feature = "std")]
    {
        if let Some(resolved_text) = block.get_resolved_text(idx) {
            return Ok(DecodedValue::String(resolved_text.clone()));
        }

        if let Some(resolved_conversion) = block.get_resolved_conversion(idx) {
            return resolved_conversion.apply_decoded(value, &[]);
        }
    }

    // If no range matched (idx == n_ranges) and we have a default conversion, use it
    if idx >= n_ranges
        && let Some(default_conversion) = block.get_default_conversion()
    {
        return default_conversion.apply_decoded(value, &[]);
    }

    // Fallback to legacy behavior if no resolved data (for backward compatibility)
    let link = *block.cc_ref.get(idx).unwrap_or(&0);
    if link == 0 {
        // Try default conversion as final fallback
        if let Some(default_conversion) = block.get_default_conversion() {
            return default_conversion.apply_decoded(value, &[]);
        }
        return Ok(DecodedValue::Unknown);
    }

    let off = link as usize;
    if off + 24 > file_data.len() {
        // Try default conversion if link is invalid
        if let Some(default_conversion) = block.get_default_conversion() {
            return default_conversion.apply_decoded(value, &[]);
        }
        return Ok(DecodedValue::Unknown);
    }

    let hdr = BlockHeader::from_bytes(&file_data[off..off + 24])?;
    if hdr.id == "##TX" {
        return match read_string_block(file_data, link)? {
            Some(txt) => Ok(DecodedValue::String(txt)),
            None => {
                // Try default conversion if text block read failed
                if let Some(default_conversion) = block.get_default_conversion() {
                    default_conversion.apply_decoded(value, &[])
                } else {
                    Ok(DecodedValue::Unknown)
                }
            }
        };
    }
    if hdr.id == "##CC" {
        let mut nested = ConversionBlock::from_bytes(&file_data[off..])?;
        let _ = nested.resolve_formula(file_data);
        return nested.apply_decoded(value, file_data);
    }

    // Try default conversion for unrecognized block types
    if let Some(default_conversion) = block.get_default_conversion() {
        return default_conversion.apply_decoded(value, &[]);
    }

    Ok(DecodedValue::Unknown)
}

pub fn apply_text_to_value(
    block: &ConversionBlock,
    value: DecodedValue,
    file_data: &[u8],
) -> Result<DecodedValue> {
    let input = match value {
        DecodedValue::String(s) => s,
        other => return Ok(other),
    };
    let n = block.cc_ref.len();

    // First try to use resolved data if available (std only)
    #[cfg(feature = "std")]
    if let Some(resolved_texts) = &block.resolved_texts {
        for (i, resolved_text) in resolved_texts.iter() {
            if *i < n && input == *resolved_text {
                if *i < block.cc_val.len() {
                    return Ok(DecodedValue::Float(block.cc_val[*i]));
                } else {
                    return Ok(DecodedValue::Unknown);
                }
            }
        }
        // If we have resolved texts but no match found, return default or unknown
        if block.cc_val.len() > n {
            return Ok(DecodedValue::Float(block.cc_val[n]));
        } else {
            return Ok(DecodedValue::Unknown);
        }
    }

    // Fallback to legacy behavior if no resolved data (for backward compatibility)
    for i in 0..n {
        let link = block.cc_ref[i];
        if link == 0 {
            continue;
        }
        if let Some(key_str) = read_string_block(file_data, link)?
            && input == key_str
        {
            if i < block.cc_val.len() {
                return Ok(DecodedValue::Float(block.cc_val[i]));
            } else {
                return Ok(DecodedValue::Unknown);
            }
        }
    }
    if block.cc_val.len() > n {
        Ok(DecodedValue::Float(block.cc_val[n]))
    } else {
        Ok(DecodedValue::Unknown)
    }
}

pub fn apply_text_to_text(
    block: &ConversionBlock,
    value: DecodedValue,
    file_data: &[u8],
) -> Result<DecodedValue> {
    let input = match value {
        DecodedValue::String(s) => s,
        other => return Ok(other),
    };
    let pairs = block.cc_ref.len().saturating_sub(1) / 2;

    // First try to use resolved data if available (std only)
    #[cfg(feature = "std")]
    if let Some(resolved_texts) = &block.resolved_texts {
        for i in 0..pairs {
            let key_idx = 2 * i;
            let output_idx = 2 * i + 1;

            if let Some(key_str) = resolved_texts.get(&key_idx)
                && *key_str == input
            {
                return if let Some(output_str) = resolved_texts.get(&output_idx) {
                    Ok(DecodedValue::String(output_str.clone()))
                } else {
                    Ok(DecodedValue::String(input))
                };
            }
        }
        // Default case with resolved texts
        let default_idx = 2 * pairs;
        if let Some(default_str) = resolved_texts.get(&default_idx) {
            return Ok(DecodedValue::String(default_str.clone()));
        } else {
            return Ok(DecodedValue::String(input));
        }
    }

    // Fallback to legacy behavior if no resolved data (for backward compatibility)
    for i in 0..pairs {
        let key_link = block.cc_ref[2 * i];
        let output_link = block.cc_ref[2 * i + 1];
        if let Some(key_str) = read_string_block(file_data, key_link)?
            && key_str == input
        {
            return if output_link == 0 {
                Ok(DecodedValue::String(input))
            } else {
                Ok(read_string_block(file_data, output_link)?
                    .map(DecodedValue::String)
                    .unwrap_or(DecodedValue::String(input)))
            };
        }
    }
    let default_link = *block.cc_ref.get(2 * pairs).unwrap_or(&0);
    if default_link == 0 {
        Ok(DecodedValue::String(input))
    } else {
        Ok(read_string_block(file_data, default_link)?
            .map(DecodedValue::String)
            .unwrap_or(DecodedValue::String(input)))
    }
}
