//! Streaming write configuration for long-running MDF4 captures.
//!
//! This module provides [`FlushPolicy`] for configuring automatic flushing
//! of MDF4 data during capture, enabling memory-efficient logging of long
//! recordings.
//!
//! # Use Cases
//!
//! - Hours-long vehicle data logging without running out of memory
//! - Crash-safe logging where partial files remain valid MDF4
//! - Embedded systems with limited RAM
//!
//! # Example
//!
//! ```ignore
//! use mdf4_rs::{MdfWriter, FlushPolicy};
//!
//! let mut writer = MdfWriter::new("output.mf4")?
//!     .with_flush_policy(FlushPolicy::EveryNRecords(1000));
//!
//! // Records are automatically flushed to disk every 1000 records
//! for i in 0..10000 {
//!     writer.write_record(&cg_id, &values)?;
//! }
//! ```

/// Policy for automatic flushing of MDF4 data during streaming writes.
///
/// When a flush policy is set, the writer will automatically flush buffered
/// data to disk based on the policy criteria. This is essential for long-running
/// captures where keeping all data in memory is not feasible.
///
/// # Flush Behavior
///
/// When a flush is triggered:
/// 1. All buffered record data is written to the underlying I/O
/// 2. DT block size links are updated
/// 3. The I/O buffer is flushed to disk
///
/// The file remains in a valid state after each flush, with proper DT block
/// sizes recorded. Final DL (Data List) blocks are created during finalization.
#[derive(Debug, Clone, PartialEq)]
pub enum FlushPolicy {
    /// Never auto-flush. Data is only flushed on explicit `flush()` or `finalize()` calls.
    /// This is the default behavior.
    Manual,

    /// Flush after every N records written across all channel groups.
    ///
    /// This is useful when you want predictable flush intervals based on
    /// the number of data points captured.
    ///
    /// # Example
    /// ```ignore
    /// // Flush every 1000 records
    /// FlushPolicy::EveryNRecords(1000)
    /// ```
    EveryNRecords(u64),

    /// Flush after N bytes of record data have been written.
    ///
    /// This is useful when you want to limit memory usage to a specific
    /// amount regardless of record size.
    ///
    /// # Example
    /// ```ignore
    /// // Flush every 1 MB of data
    /// FlushPolicy::EveryNBytes(1024 * 1024)
    /// ```
    EveryNBytes(u64),
}

impl Default for FlushPolicy {
    fn default() -> Self {
        FlushPolicy::Manual
    }
}

impl FlushPolicy {
    /// Check if this policy requires automatic flushing.
    pub fn is_auto(&self) -> bool {
        !matches!(self, FlushPolicy::Manual)
    }
}

/// Configuration for streaming MDF4 writes.
#[derive(Debug, Clone, Default)]
pub struct StreamingConfig {
    /// The flush policy to use.
    pub policy: FlushPolicy,
}

impl StreamingConfig {
    /// Create a new streaming configuration with manual flush policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a streaming configuration that flushes every N records.
    pub fn every_n_records(n: u64) -> Self {
        Self {
            policy: FlushPolicy::EveryNRecords(n),
        }
    }

    /// Create a streaming configuration that flushes every N bytes.
    pub fn every_n_bytes(n: u64) -> Self {
        Self {
            policy: FlushPolicy::EveryNBytes(n),
        }
    }
}

/// Tracks flush state for streaming writes.
#[derive(Debug, Default)]
pub(super) struct FlushState {
    /// Records written since last flush.
    pub records_since_flush: u64,
    /// Bytes written since last flush.
    pub bytes_since_flush: u64,
    /// Total records written.
    pub total_records: u64,
    /// Total bytes written.
    pub total_bytes: u64,
    /// Number of flushes performed.
    pub flush_count: u64,
}

impl FlushState {
    /// Record that data was written.
    pub fn record_write(&mut self, records: u64, bytes: u64) {
        self.records_since_flush += records;
        self.bytes_since_flush += bytes;
        self.total_records += records;
        self.total_bytes += bytes;
    }

    /// Check if a flush should be triggered based on the policy.
    pub fn should_flush(&self, policy: &FlushPolicy) -> bool {
        match policy {
            FlushPolicy::Manual => false,
            FlushPolicy::EveryNRecords(n) => self.records_since_flush >= *n,
            FlushPolicy::EveryNBytes(n) => self.bytes_since_flush >= *n,
        }
    }

    /// Reset counters after a flush.
    pub fn on_flush(&mut self) {
        self.records_since_flush = 0;
        self.bytes_since_flush = 0;
        self.flush_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flush_policy_default() {
        assert_eq!(FlushPolicy::default(), FlushPolicy::Manual);
    }

    #[test]
    fn test_flush_policy_is_auto() {
        assert!(!FlushPolicy::Manual.is_auto());
        assert!(FlushPolicy::EveryNRecords(100).is_auto());
        assert!(FlushPolicy::EveryNBytes(1024).is_auto());
    }

    #[test]
    fn test_flush_state_should_flush() {
        let mut state = FlushState::default();

        // Manual policy never triggers
        state.record_write(1000, 10000);
        assert!(!state.should_flush(&FlushPolicy::Manual));

        // EveryNRecords triggers at threshold
        assert!(!state.should_flush(&FlushPolicy::EveryNRecords(1001)));
        assert!(state.should_flush(&FlushPolicy::EveryNRecords(1000)));
        assert!(state.should_flush(&FlushPolicy::EveryNRecords(500)));

        // EveryNBytes triggers at threshold
        assert!(!state.should_flush(&FlushPolicy::EveryNBytes(10001)));
        assert!(state.should_flush(&FlushPolicy::EveryNBytes(10000)));
        assert!(state.should_flush(&FlushPolicy::EveryNBytes(5000)));
    }

    #[test]
    fn test_flush_state_reset() {
        let mut state = FlushState::default();
        state.record_write(100, 1000);
        assert_eq!(state.records_since_flush, 100);
        assert_eq!(state.bytes_since_flush, 1000);

        state.on_flush();
        assert_eq!(state.records_since_flush, 0);
        assert_eq!(state.bytes_since_flush, 0);
        assert_eq!(state.total_records, 100); // Total preserved
        assert_eq!(state.total_bytes, 1000);
        assert_eq!(state.flush_count, 1);
    }

    #[test]
    fn test_streaming_config_constructors() {
        let config = StreamingConfig::every_n_records(500);
        assert_eq!(config.policy, FlushPolicy::EveryNRecords(500));

        let config = StreamingConfig::every_n_bytes(1024);
        assert_eq!(config.policy, FlushPolicy::EveryNBytes(1024));
    }
}
