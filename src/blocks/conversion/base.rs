use super::types::ConversionType;
use crate::blocks::common::{BlockHeader, BlockParse};
use crate::{Error, Result};

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use alloc::collections::BTreeMap;
#[cfg(feature = "std")]
use alloc::collections::BTreeSet;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConversionBlock {
    pub header: BlockHeader,

    // Link section
    pub cc_tx_name: Option<u64>,
    pub cc_md_unit: Option<u64>,
    pub cc_md_comment: Option<u64>,
    pub cc_cc_inverse: Option<u64>,
    pub cc_ref: Vec<u64>,

    // Data
    pub cc_type: ConversionType,
    pub cc_precision: u8,
    pub cc_flags: u16,
    pub cc_ref_count: u16,
    pub cc_val_count: u16,
    pub cc_phy_range_min: Option<f64>,
    pub cc_phy_range_max: Option<f64>,
    pub cc_val: Vec<f64>,

    pub formula: Option<String>,

    // Resolved data for self-contained conversions (populated during index creation)
    /// Pre-resolved text strings for text-based conversions (ValueToText, RangeToText, etc.)
    /// Maps cc_ref indices to their resolved text content
    #[cfg(feature = "std")]
    pub resolved_texts: Option<BTreeMap<usize, String>>,
    #[cfg(not(feature = "std"))]
    pub resolved_texts: Option<()>,

    /// Pre-resolved nested conversion blocks for chained conversions
    /// Maps cc_ref indices to their resolved ConversionBlock content
    #[cfg(feature = "std")]
    pub resolved_conversions: Option<BTreeMap<usize, Box<ConversionBlock>>>,
    #[cfg(not(feature = "std"))]
    pub resolved_conversions: Option<()>,

    /// Default conversion for fallback cases (similar to asammdf's "default_addr")
    /// This is typically the last reference in cc_ref for some conversion types
    pub default_conversion: Option<Box<ConversionBlock>>,
}

impl BlockParse<'_> for ConversionBlock {
    const ID: &'static str = "##CC";
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let mut offset = 24;

        // Fixed links
        let cc_tx_name = read_link(bytes, &mut offset);
        let cc_md_unit = read_link(bytes, &mut offset);
        let cc_md_comment = read_link(bytes, &mut offset);
        let cc_cc_inverse = read_link(bytes, &mut offset);

        let fixed_links = 4;
        let additional_links = header.links_nr.saturating_sub(fixed_links);
        let mut cc_ref = Vec::with_capacity(additional_links as usize);
        for _ in 0..additional_links {
            cc_ref.push(read_u64(bytes, &mut offset)?);
        }

        // Basic fields
        let cc_type = ConversionType::from_u8(bytes[offset]);
        offset += 1;
        let cc_precision = bytes[offset];
        offset += 1;
        let cc_flags = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;
        let cc_ref_count = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;
        let cc_val_count = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;

        // IMPORTANT: Some vendors (like dSPACE) always write the physical range fields
        // even when cc_flags bit 1 is not set. We need to detect this by checking if
        // there's enough data in the block for the range fields.
        // Calculate expected sizes:
        let size_without_range =
            24 + (header.links_nr as usize * 8) + 8 + (cc_val_count as usize * 8);
        let size_with_range = size_without_range + 16;
        let has_range_data = header.block_len as usize >= size_with_range;

        let cc_phy_range_min = if has_range_data {
            let val = f64::from_bits(read_u64(bytes, &mut offset)?);
            Some(val)
        } else {
            None
        };

        let cc_phy_range_max = if has_range_data {
            let val = f64::from_bits(read_u64(bytes, &mut offset)?);
            Some(val)
        } else {
            None
        };

        let mut cc_val = Vec::with_capacity(cc_val_count as usize);
        for _ in 0..cc_val_count {
            let val = f64::from_bits(read_u64(bytes, &mut offset)?);
            cc_val.push(val);
        }

        Ok(Self {
            header,
            cc_tx_name,
            cc_md_unit,
            cc_md_comment,
            cc_cc_inverse,
            cc_ref,
            cc_type,
            cc_precision,
            cc_flags,
            cc_ref_count,
            cc_val_count,
            cc_phy_range_min,
            cc_phy_range_max,
            cc_val,
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        })
    }
}

fn read_link(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let link = u64::from_le_bytes(bytes[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    if link == 0 { None } else { Some(link) }
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64> {
    if bytes.len() < *offset + 8 {
        return Err(Error::TooShortBuffer {
            actual: bytes.len(),
            expected: *offset + 8,
            file: file!(),
            line: line!(),
        });
    }
    let val = u64::from_le_bytes(bytes[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    Ok(val)
}

impl ConversionBlock {
    /// Resolve all dependencies for this conversion block to make it self-contained.
    /// This reads referenced text blocks and nested conversions from the file data
    /// and stores them in the resolved_texts and resolved_conversions fields.
    ///
    /// Supports arbitrary depth conversion chains with cycle detection.
    ///
    /// # Arguments
    /// * `file_data` - Memory mapped MDF bytes used to read referenced data
    ///
    /// # Returns
    /// `Ok(())` on success or an [`Error`] if resolution fails
    #[cfg(feature = "std")]
    pub fn resolve_all_dependencies(&mut self, file_data: &[u8]) -> Result<()> {
        self.resolve_all_dependencies_with_address(file_data, 0)
    }

    /// Resolve all dependencies with a known current block address (used internally)
    #[cfg(feature = "std")]
    pub fn resolve_all_dependencies_with_address(
        &mut self,
        file_data: &[u8],
        current_address: u64,
    ) -> Result<()> {
        // Start resolution with empty visited set to detect cycles
        let mut visited = BTreeSet::new();
        self.resolve_all_dependencies_recursive(file_data, 0, &mut visited, current_address)
    }

    /// Internal recursive method for resolving conversion dependencies.
    ///
    /// # Arguments
    /// * `file_data` - Memory mapped MDF bytes used to read referenced data
    /// * `depth` - Current recursion depth (for cycle detection)
    /// * `visited` - Set of visited block addresses (for cycle detection)
    /// * `current_address` - Address of the current conversion block being resolved
    ///
    /// # Returns
    /// `Ok(())` on success or an [`Error`] if resolution fails
    #[cfg(feature = "std")]
    fn resolve_all_dependencies_recursive(
        &mut self,
        file_data: &[u8],
        depth: usize,
        visited: &mut BTreeSet<u64>,
        current_address: u64,
    ) -> Result<()> {
        use crate::blocks::common::{BlockHeader, read_string_block};

        const MAX_DEPTH: usize = 20; // Reasonable depth limit

        // Prevent infinite recursion
        if depth > MAX_DEPTH {
            return Err(Error::ConversionChainTooDeep {
                max_depth: MAX_DEPTH,
            });
        }

        // Add current address to visited set
        visited.insert(current_address);

        // First resolve the formula if this is an algebraic conversion
        self.resolve_formula(file_data)?;

        // Initialize resolved data containers
        let mut resolved_texts = BTreeMap::new();
        let mut resolved_conversions = BTreeMap::new();
        let mut default_conversion = None;

        // Re-enable default conversion logic for specific types that need it
        let has_default_conversion = matches!(
            self.cc_type,
            crate::blocks::conversion::types::ConversionType::RangeToText // Add other types here as needed based on MDF specification
        );

        // For some conversion types, the last reference might be the default conversion
        let default_ref_index = if has_default_conversion && self.cc_ref.len() > 2 {
            // Only treat as default if there are more than 2 references
            // This avoids incorrectly treating simple cases as having defaults
            Some(self.cc_ref.len() - 1)
        } else {
            None
        };

        // Resolve each reference in cc_ref
        for (i, &link_addr) in self.cc_ref.iter().enumerate() {
            // Skip null links (address 0 typically means null in MDF format)
            if link_addr == 0 {
                continue; // Skip null links
            }

            // Check for cycles
            if visited.contains(&link_addr) {
                return Err(Error::ConversionChainCycle { address: link_addr });
            }

            let offset = link_addr as usize;
            if offset + 24 > file_data.len() {
                continue; // Skip invalid offsets
            }

            // Read the block header to determine the type
            let header = BlockHeader::from_bytes(&file_data[offset..offset + 24])?;

            match header.id.as_str() {
                "##TX" => {
                    // Text block - resolve the string content
                    if let Some(text) = read_string_block(file_data, link_addr)? {
                        resolved_texts.insert(i, text);
                    }
                }
                "##CC" => {
                    // Nested conversion block - resolve recursively
                    let mut nested_conversion = ConversionBlock::from_bytes(&file_data[offset..])?;
                    nested_conversion.resolve_all_dependencies_recursive(
                        file_data,
                        depth + 1,
                        visited,
                        link_addr,
                    )?;

                    // Check if this should be stored as default conversion
                    if Some(i) == default_ref_index {
                        default_conversion = Some(Box::new(nested_conversion));
                    } else {
                        resolved_conversions.insert(i, Box::new(nested_conversion));
                    }
                }
                _ => {
                    // Other block types - ignore for now but could be extended
                    // to support metadata blocks, source information, etc.
                }
            }
        }

        // Store resolved data if any was found
        if !resolved_texts.is_empty() {
            self.resolved_texts = Some(resolved_texts);
        }
        if !resolved_conversions.is_empty() {
            self.resolved_conversions = Some(resolved_conversions);
        }
        if default_conversion.is_some() {
            self.default_conversion = default_conversion;
        }

        // Remove current address from visited set before returning
        visited.remove(&current_address);

        Ok(())
    }

    /// Get a resolved text string for a given cc_ref index.
    /// Returns the text if it was resolved during dependency resolution.
    #[cfg(feature = "std")]
    pub fn get_resolved_text(&self, ref_index: usize) -> Option<&String> {
        self.resolved_texts.as_ref()?.get(&ref_index)
    }

    /// Get a resolved nested conversion for a given cc_ref index.
    /// Returns the conversion block if it was resolved during dependency resolution.
    #[cfg(feature = "std")]
    pub fn get_resolved_conversion(&self, ref_index: usize) -> Option<&ConversionBlock> {
        self.resolved_conversions
            .as_ref()?
            .get(&ref_index)
            .map(|boxed| boxed.as_ref())
    }

    /// Get the default conversion for fallback cases.
    /// Returns the default conversion if it was resolved during dependency resolution.
    pub fn get_default_conversion(&self) -> Option<&ConversionBlock> {
        self.default_conversion.as_ref().map(|boxed| boxed.as_ref())
    }

    /// Serialize this conversion block back to bytes.
    ///
    /// # Returns
    /// A byte vector containing the encoded block or an [`Error`] if
    /// serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let links = 4 + self.cc_ref.len();

        let mut header = self.header.clone();
        header.links_nr = links as u64;

        let mut size = 24 + links * 8 + 1 + 1 + 2 + 2 + 2;
        // Include range fields if they exist (regardless of flag)
        if self.cc_phy_range_min.is_some() || self.cc_phy_range_max.is_some() {
            size += 16;
        }
        size += self.cc_val.len() * 8;
        header.block_len = size as u64;

        let mut buf = Vec::with_capacity(size);
        buf.extend_from_slice(&header.to_bytes()?);
        for link in [
            self.cc_tx_name,
            self.cc_md_unit,
            self.cc_md_comment,
            self.cc_cc_inverse,
        ] {
            buf.extend_from_slice(&link.unwrap_or(0).to_le_bytes());
        }
        for l in &self.cc_ref {
            buf.extend_from_slice(&l.to_le_bytes());
        }
        buf.push(self.cc_type.to_u8());
        buf.push(self.cc_precision);
        buf.extend_from_slice(&self.cc_flags.to_le_bytes());
        buf.extend_from_slice(&(self.cc_ref_count).to_le_bytes());
        buf.extend_from_slice(&(self.cc_val_count).to_le_bytes());
        // Write range fields if they exist (regardless of flag, for vendor compatibility)
        if self.cc_phy_range_min.is_some() || self.cc_phy_range_max.is_some() {
            buf.extend_from_slice(&self.cc_phy_range_min.unwrap_or(0.0).to_le_bytes());
            buf.extend_from_slice(&self.cc_phy_range_max.unwrap_or(0.0).to_le_bytes());
        }
        for v in &self.cc_val {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        if buf.len() != size {
            return Err(Error::BlockSerializationError(format!(
                "ConversionBlock expected size {size} but wrote {}",
                buf.len()
            )));
        }
        Ok(buf)
    }
}
