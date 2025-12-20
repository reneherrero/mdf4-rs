use crate::{Result, channel_group::ChannelGroup, parsing::MdfFile};

#[derive(Debug)]
/// High level representation of an MDF file.
///
/// The struct stores the memory mapped file internally and lazily exposes
/// [`ChannelGroup`] wrappers for easy inspection.
pub struct MDF {
    raw: MdfFile,
}

impl MDF {
    /// Parse an MDF4 file from disk.
    ///
    /// # Arguments
    /// * `path` - Path to the `.mf4` file.
    ///
    /// # Returns
    /// A new [`MDF`] on success or [`crate::Error`] on failure.
    pub fn from_file(path: &str) -> Result<Self> {
        let raw = MdfFile::parse_from_file(path)?;
        Ok(MDF { raw })
    }

    /// Access the raw parsed MDF file structure.
    ///
    /// Useful for debugging or advanced use cases.
    pub fn raw(&self) -> &MdfFile {
        &self.raw
    }

    /// Retrieve channel groups contained in the file.
    ///
    /// Each [`ChannelGroup`] is created lazily and does not decode any samples.
    pub fn channel_groups(&self) -> Vec<ChannelGroup<'_>> {
        let mut groups = Vec::new();

        for raw_data_group in &self.raw.data_groups {
            for raw_channel_group in &raw_data_group.channel_groups {
                groups.push(ChannelGroup::new(
                    raw_data_group,
                    raw_channel_group,
                    &self.raw.mmap,
                ));
            }
        }

        groups
    }
}
