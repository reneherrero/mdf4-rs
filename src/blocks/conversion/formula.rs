use crate::Result;
use crate::blocks::common::read_string_block;
use crate::blocks::conversion::base::ConversionBlock;
use crate::blocks::conversion::types::ConversionType;

impl ConversionBlock {
    /// Resolve and store the algebraic formula referenced by this block.
    ///
    /// # Arguments
    /// * `file_data` - Memory mapped MDF bytes used to read the formula text.
    ///
    /// # Returns
    /// `Ok(())` on success or an [`crate::Error`] if the formula block cannot be
    /// read.
    pub fn resolve_formula(&mut self, file_data: &[u8]) -> Result<()> {
        if self.conversion_type != ConversionType::Algebraic || self.refs.is_empty() {
            return Ok(());
        }

        let addr = self.refs[0];
        if let Some(formula) = read_string_block(file_data, addr)? {
            self.formula = Some(formula);
        }

        Ok(())
    }
}
