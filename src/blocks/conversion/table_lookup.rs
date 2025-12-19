use super::linear::extract_numeric;
use crate::Result;
use crate::blocks::conversion::base::ConversionBlock;
use crate::parsing::decoder::DecodedValue;

/// General table lookup: either interpolated or nearest neighbour.
/// `cc_val` must be `[key0, val0, key1, val1, …]`.
pub fn lookup_table(cc_val: &[f64], raw: f64, interp: bool) -> Option<f64> {
    let len = cc_val.len();
    if len < 4 || !len.is_multiple_of(2) {
        return None;
    }
    let n = len / 2;
    let mut table = Vec::with_capacity(n);
    for i in 0..n {
        table.push((cc_val[2 * i], cc_val[2 * i + 1]));
    }
    if raw <= table[0].0 {
        return Some(table[0].1);
    }
    if raw >= table[n - 1].0 {
        return Some(table[n - 1].1);
    }
    for i in 0..(n - 1) {
        let (k0, v0) = table[i];
        let (k1, v1) = table[i + 1];
        if raw >= k0 && raw <= k1 {
            if interp {
                let t = (raw - k0) / (k1 - k0);
                return Some(v0 + t * (v1 - v0));
            } else {
                let d0 = raw - k0;
                let d1 = k1 - raw;
                return Some(if d1 < d0 { v1 } else { v0 });
            }
        }
    }
    None
}

pub fn apply_table_lookup(
    block: &ConversionBlock,
    value: DecodedValue,
    interp: bool,
) -> Result<DecodedValue> {
    if let Some(raw) = extract_numeric(&value) {
        let phys = lookup_table(&block.cc_val, raw, interp).unwrap_or(raw);
        Ok(DecodedValue::Float(phys))
    } else {
        Ok(value)
    }
}

pub fn apply_range_lookup(block: &ConversionBlock, value: DecodedValue) -> Result<DecodedValue> {
    if let Some(raw) = extract_numeric(&value) {
        let inclusive_upper = matches!(
            value,
            DecodedValue::UnsignedInteger(_) | DecodedValue::SignedInteger(_)
        );
        let v = &block.cc_val;
        if v.len() < 4 || !(v.len() - 1).is_multiple_of(3) {
            return Ok(DecodedValue::Float(raw));
        }
        let n = (v.len() - 1) / 3;
        let default = v[3 * n];
        for i in 0..n {
            let key_min = v[3 * i];
            let key_max = v[3 * i + 1];
            let phys = v[3 * i + 2];
            if inclusive_upper {
                if raw >= key_min && raw <= key_max {
                    return Ok(DecodedValue::Float(phys));
                }
            } else if raw >= key_min && raw < key_max {
                return Ok(DecodedValue::Float(phys));
            }
        }
        Ok(DecodedValue::Float(default))
    } else {
        Ok(value)
    }
}
