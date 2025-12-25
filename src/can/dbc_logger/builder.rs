//! Builder pattern for CanDbcLogger configuration.

use crate::writer::FlushPolicy;

/// Configuration for CanDbcLogger.
#[derive(Debug, Clone)]
pub struct CanDbcLoggerConfig {
    /// Store raw values with conversion blocks instead of physical values.
    /// Default: false (store physical values as f64)
    pub store_raw_values: bool,

    /// Include unit information in MDF channels.
    /// Default: true
    pub include_units: bool,

    /// Include min/max limits in MDF channels.
    /// Default: true
    pub include_limits: bool,

    /// Include conversion blocks (for raw value mode).
    /// Default: true
    pub include_conversions: bool,

    /// Include value descriptions as ValueToText conversions.
    /// When enabled, DBC VAL_ entries are converted to MDF4 ValueToText blocks.
    /// Default: true
    pub include_value_descriptions: bool,
}

impl Default for CanDbcLoggerConfig {
    fn default() -> Self {
        Self {
            store_raw_values: false,
            include_units: true,
            include_limits: true,
            include_conversions: true,
            include_value_descriptions: true,
        }
    }
}

/// Builder for CanDbcLogger configuration.
pub struct CanDbcLoggerBuilder {
    pub(super) dbc: dbc_rs::Dbc,
    pub(super) config: CanDbcLoggerConfig,
    pub(super) capacity: Option<usize>,
    pub(super) flush_policy: Option<FlushPolicy>,
}

impl CanDbcLoggerBuilder {
    /// Create a new builder with default configuration.
    pub fn new(dbc: dbc_rs::Dbc) -> Self {
        Self {
            dbc,
            config: CanDbcLoggerConfig::default(),
            capacity: None,
            flush_policy: None,
        }
    }

    /// Set whether to store raw values with conversion blocks.
    ///
    /// When enabled, raw integer values are stored and conversion blocks
    /// are attached to channels. This preserves full precision and allows
    /// MDF4 viewers to display both raw and physical values.
    ///
    /// Default: false (stores physical f64 values)
    pub fn store_raw_values(mut self, enabled: bool) -> Self {
        self.config.store_raw_values = enabled;
        self
    }

    /// Set whether to include unit strings in MDF channels.
    ///
    /// Default: true
    pub fn include_units(mut self, enabled: bool) -> Self {
        self.config.include_units = enabled;
        self
    }

    /// Set whether to include min/max limits in MDF channels.
    ///
    /// Default: true
    pub fn include_limits(mut self, enabled: bool) -> Self {
        self.config.include_limits = enabled;
        self
    }

    /// Set whether to include conversion blocks (for raw value mode).
    ///
    /// Default: true
    pub fn include_conversions(mut self, enabled: bool) -> Self {
        self.config.include_conversions = enabled;
        self
    }

    /// Set whether to include value descriptions as ValueToText conversions.
    ///
    /// When enabled, DBC VAL_ entries are converted to MDF4 ValueToText blocks.
    /// This allows MDF4 viewers to display human-readable text for enum-like signals.
    ///
    /// Note: If a signal has both a linear conversion (factor/offset != 1/0) and
    /// value descriptions, the value descriptions take precedence.
    ///
    /// Default: true
    pub fn include_value_descriptions(mut self, enabled: bool) -> Self {
        self.config.include_value_descriptions = enabled;
        self
    }

    /// Set the initial buffer capacity.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = Some(capacity);
        self
    }

    /// Set the flush policy for streaming writes.
    ///
    /// When a flush policy is set, the underlying MDF writer will automatically
    /// flush buffered data to disk based on the policy criteria. This is essential
    /// for long-running captures where keeping all data in memory is not feasible.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mdf4_rs::can::CanDbcLogger;
    /// use mdf4_rs::FlushPolicy;
    ///
    /// let mut logger = CanDbcLogger::builder(dbc)
    ///     .with_flush_policy(FlushPolicy::EveryNRecords(1000))
    ///     .build_file("output.mf4")?;
    /// ```
    pub fn with_flush_policy(mut self, policy: FlushPolicy) -> Self {
        self.flush_policy = Some(policy);
        self
    }

    /// Build the logger with in-memory output.
    pub fn build(self) -> crate::Result<super::CanDbcLogger<crate::writer::VecWriter>> {
        let mut writer = match self.capacity {
            Some(cap) => {
                crate::MdfWriter::from_writer(crate::writer::VecWriter::with_capacity(cap))
            }
            None => crate::MdfWriter::from_writer(crate::writer::VecWriter::new()),
        };
        if let Some(policy) = self.flush_policy {
            writer.set_flush_policy(policy);
        }
        Ok(super::CanDbcLogger::with_config(
            self.dbc,
            writer,
            self.config,
        ))
    }

    /// Build the logger with file output.
    #[cfg(feature = "std")]
    pub fn build_file(
        self,
        path: &str,
    ) -> crate::Result<super::CanDbcLogger<crate::writer::FileWriter>> {
        let mut writer = match self.capacity {
            Some(cap) => crate::MdfWriter::new_with_capacity(path, cap)?,
            None => crate::MdfWriter::new(path)?,
        };
        if let Some(policy) = self.flush_policy {
            writer.set_flush_policy(policy);
        }
        Ok(super::CanDbcLogger::with_config(
            self.dbc,
            writer,
            self.config,
        ))
    }
}
