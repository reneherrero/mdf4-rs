use super::types::ConversionType;
use crate::blocks::common::{BlockHeader, BlockParse, read_u8, read_u16, validate_buffer_size};
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
    pub name_addr: Option<u64>,
    pub unit_addr: Option<u64>,
    pub comment_addr: Option<u64>,
    pub inverse_addr: Option<u64>,
    pub refs: Vec<u64>,

    // Data
    pub conversion_type: ConversionType,
    pub precision: u8,
    pub flags: u16,
    pub ref_count: u16,
    pub value_count: u16,
    pub phys_range_min: Option<f64>,
    pub phys_range_max: Option<f64>,
    pub values: Vec<f64>,

    pub formula: Option<String>,

    // Resolved data for self-contained conversions (populated during index creation)
    /// Pre-resolved text strings for text-based conversions (ValueToText, RangeToText, etc.)
    /// Maps refs indices to their resolved text content
    #[cfg(feature = "std")]
    pub resolved_texts: Option<BTreeMap<usize, String>>,
    #[cfg(not(feature = "std"))]
    pub resolved_texts: Option<()>,

    /// Pre-resolved nested conversion blocks for chained conversions
    /// Maps refs indices to their resolved ConversionBlock content
    #[cfg(feature = "std")]
    pub resolved_conversions: Option<BTreeMap<usize, Box<ConversionBlock>>>,
    #[cfg(not(feature = "std"))]
    pub resolved_conversions: Option<()>,

    /// Default conversion for fallback cases (similar to asammdf's "default_addr")
    /// This is typically the last reference in refs for some conversion types
    pub default_conversion: Option<Box<ConversionBlock>>,
}

impl BlockParse<'_> for ConversionBlock {
    const ID: &'static str = "##CC";
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;

        let mut offset = 24;

        // Fixed links
        let name_addr = read_link(bytes, &mut offset);
        let unit_addr = read_link(bytes, &mut offset);
        let comment_addr = read_link(bytes, &mut offset);
        let inverse_addr = read_link(bytes, &mut offset);

        let fixed_links = 4;
        let additional_links = header.link_count.saturating_sub(fixed_links);
        let mut refs = Vec::with_capacity(additional_links as usize);
        for _ in 0..additional_links {
            refs.push(read_u64_checked(bytes, &mut offset)?);
        }

        // Basic fields
        let conversion_type = ConversionType::from_u8(read_u8(bytes, offset));
        offset += 1;
        let precision = read_u8(bytes, offset);
        offset += 1;
        let flags = read_u16(bytes, offset);
        offset += 2;
        let ref_count = read_u16(bytes, offset);
        offset += 2;
        let value_count = read_u16(bytes, offset);
        offset += 2;

        // IMPORTANT: Some vendors (like dSPACE) always write the physical range fields
        // even when flags bit 1 is not set. We need to detect this by checking if
        // there's enough data in the block for the range fields.
        // Calculate expected sizes:
        let size_without_range =
            24 + (header.link_count as usize * 8) + 8 + (value_count as usize * 8);
        let size_with_range = size_without_range + 16;
        let has_range_data = header.length as usize >= size_with_range;

        let phys_range_min = if has_range_data {
            let val = f64::from_bits(read_u64_checked(bytes, &mut offset)?);
            Some(val)
        } else {
            None
        };

        let phys_range_max = if has_range_data {
            let val = f64::from_bits(read_u64_checked(bytes, &mut offset)?);
            Some(val)
        } else {
            None
        };

        let mut values = Vec::with_capacity(value_count as usize);
        for _ in 0..value_count {
            let val = f64::from_bits(read_u64_checked(bytes, &mut offset)?);
            values.push(val);
        }

        Ok(Self {
            header,
            name_addr,
            unit_addr,
            comment_addr,
            inverse_addr,
            refs,
            conversion_type,
            precision,
            flags,
            ref_count,
            value_count,
            phys_range_min,
            phys_range_max,
            values,
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        })
    }
}

/// Read an optional link from bytes, advancing the offset.
fn read_link(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let link = u64::from_le_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
        bytes[*offset + 4],
        bytes[*offset + 5],
        bytes[*offset + 6],
        bytes[*offset + 7],
    ]);
    *offset += 8;
    if link == 0 { None } else { Some(link) }
}

/// Read a u64 from bytes, advancing the offset and validating bounds.
fn read_u64_checked(bytes: &[u8], offset: &mut usize) -> Result<u64> {
    validate_buffer_size(bytes, *offset + 8)?;
    let val = u64::from_le_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
        bytes[*offset + 4],
        bytes[*offset + 5],
        bytes[*offset + 6],
        bytes[*offset + 7],
    ]);
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
            self.conversion_type,
            crate::blocks::conversion::types::ConversionType::RangeToText // Add other types here as needed based on MDF specification
        );

        // For some conversion types, the last reference might be the default conversion
        let default_ref_index = if has_default_conversion && self.refs.len() > 2 {
            // Only treat as default if there are more than 2 references
            // This avoids incorrectly treating simple cases as having defaults
            Some(self.refs.len() - 1)
        } else {
            None
        };

        // Resolve each reference in refs
        for (i, &link_addr) in self.refs.iter().enumerate() {
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

    /// Get a resolved text string for a given refs index.
    /// Returns the text if it was resolved during dependency resolution.
    #[cfg(feature = "std")]
    pub fn get_resolved_text(&self, ref_index: usize) -> Option<&String> {
        self.resolved_texts.as_ref()?.get(&ref_index)
    }

    /// Get a resolved nested conversion for a given refs index.
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
        let links = 4 + self.refs.len();

        let mut header = self.header.clone();
        header.link_count = links as u64;

        let mut size = 24 + links * 8 + 1 + 1 + 2 + 2 + 2;
        // Include range fields if they exist (regardless of flag)
        if self.phys_range_min.is_some() || self.phys_range_max.is_some() {
            size += 16;
        }
        size += self.values.len() * 8;
        header.length = size as u64;

        let mut buf = Vec::with_capacity(size);
        buf.extend_from_slice(&header.to_bytes()?);
        for link in [
            self.name_addr,
            self.unit_addr,
            self.comment_addr,
            self.inverse_addr,
        ] {
            buf.extend_from_slice(&link.unwrap_or(0).to_le_bytes());
        }
        for l in &self.refs {
            buf.extend_from_slice(&l.to_le_bytes());
        }
        buf.push(self.conversion_type.to_u8());
        buf.push(self.precision);
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&(self.ref_count).to_le_bytes());
        buf.extend_from_slice(&(self.value_count).to_le_bytes());
        // Write range fields if they exist (regardless of flag, for vendor compatibility)
        if self.phys_range_min.is_some() || self.phys_range_max.is_some() {
            buf.extend_from_slice(&self.phys_range_min.unwrap_or(0.0).to_le_bytes());
            buf.extend_from_slice(&self.phys_range_max.unwrap_or(0.0).to_le_bytes());
        }
        for v in &self.values {
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

    /// Creates an identity conversion (1:1, no change).
    ///
    /// This is useful when you want to explicitly indicate that no conversion
    /// is applied, while still having a conversion block for consistency.
    ///
    /// # Example
    /// ```
    /// use mdf4_rs::blocks::ConversionBlock;
    ///
    /// let conv = ConversionBlock::identity();
    /// ```
    pub fn identity() -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##CC"),
                reserved: 0,
                length: 0, // Will be calculated during to_bytes()
                link_count: 4,
            },
            name_addr: None,
            unit_addr: None,
            comment_addr: None,
            inverse_addr: None,
            refs: Vec::new(),
            conversion_type: ConversionType::Identity,
            precision: 0,
            flags: 0,
            ref_count: 0,
            value_count: 0,
            phys_range_min: None,
            phys_range_max: None,
            values: Vec::new(),
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        }
    }

    /// Creates a linear conversion: `physical = offset + factor * raw`.
    ///
    /// This is the most common conversion type, used for scaling and offset
    /// adjustments. The MDF 4.1 specification defines linear conversion as:
    /// `y = P1 + P2 * x` where P1 is the offset and P2 is the factor.
    ///
    /// # Arguments
    /// * `offset` - The offset value (P1 in the MDF spec)
    /// * `factor` - The scaling factor (P2 in the MDF spec)
    ///
    /// # Example
    /// ```
    /// use mdf4_rs::blocks::ConversionBlock;
    ///
    /// // Convert raw temperature: physical = -40.0 + 0.1 * raw
    /// let temp_conv = ConversionBlock::linear(-40.0, 0.1);
    ///
    /// // Convert RPM: physical = 0.0 + 0.25 * raw
    /// let rpm_conv = ConversionBlock::linear(0.0, 0.25);
    /// ```
    pub fn linear(offset: f64, factor: f64) -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##CC"),
                reserved: 0,
                length: 0, // Will be calculated during to_bytes()
                link_count: 4,
            },
            name_addr: None,
            unit_addr: None,
            comment_addr: None,
            inverse_addr: None,
            refs: Vec::new(),
            conversion_type: ConversionType::Linear,
            precision: 0,
            flags: 0,
            ref_count: 0,
            value_count: 2,
            phys_range_min: None,
            phys_range_max: None,
            values: alloc::vec![offset, factor],
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        }
    }

    /// Creates a rational conversion: `physical = (P1 + P2*x + P3*x²) / (P4 + P5*x + P6*x²)`.
    ///
    /// Rational conversions are used for more complex non-linear transformations.
    ///
    /// # Arguments
    /// * `p1` - Numerator constant term
    /// * `p2` - Numerator linear coefficient
    /// * `p3` - Numerator quadratic coefficient
    /// * `p4` - Denominator constant term
    /// * `p5` - Denominator linear coefficient
    /// * `p6` - Denominator quadratic coefficient
    ///
    /// # Example
    /// ```
    /// use mdf4_rs::blocks::ConversionBlock;
    ///
    /// // Simple linear via rational: physical = (0 + 2*x + 0*x²) / (1 + 0*x + 0*x²) = 2*x
    /// let conv = ConversionBlock::rational(0.0, 2.0, 0.0, 1.0, 0.0, 0.0);
    /// ```
    pub fn rational(p1: f64, p2: f64, p3: f64, p4: f64, p5: f64, p6: f64) -> Self {
        Self {
            header: BlockHeader {
                id: String::from("##CC"),
                reserved: 0,
                length: 0,
                link_count: 4,
            },
            name_addr: None,
            unit_addr: None,
            comment_addr: None,
            inverse_addr: None,
            refs: Vec::new(),
            conversion_type: ConversionType::Rational,
            precision: 0,
            flags: 0,
            ref_count: 0,
            value_count: 6,
            phys_range_min: None,
            phys_range_max: None,
            values: alloc::vec![p1, p2, p3, p4, p5, p6],
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        }
    }

    /// Check if this is a trivial identity conversion that can be omitted.
    ///
    /// Returns `true` if:
    /// - The conversion type is Identity, OR
    /// - The conversion type is Linear with offset=0 and factor=1
    pub fn is_identity(&self) -> bool {
        match self.conversion_type {
            ConversionType::Identity => true,
            ConversionType::Linear => {
                self.values.len() >= 2 && self.values[0] == 0.0 && self.values[1] == 1.0
            }
            _ => false,
        }
    }

    /// Set the physical range limits for this conversion.
    ///
    /// # Arguments
    /// * `min` - Minimum physical value
    /// * `max` - Maximum physical value
    pub fn with_physical_range(mut self, min: f64, max: f64) -> Self {
        self.phys_range_min = Some(min);
        self.phys_range_max = Some(max);
        self.flags |= 0b10; // Set physical range valid flag
        self
    }
}
