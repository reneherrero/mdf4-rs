//! Timestamped CAN frame container.

/// A CAN frame with timestamp for logging.
///
/// This is a simple container that pairs a CAN frame with a timestamp.
/// Use this when you need to associate timing information with frames.
#[derive(Debug, Clone)]
pub struct TimestampedFrame<F> {
    /// Timestamp in microseconds since start of logging
    pub timestamp_us: u64,
    /// The CAN frame
    pub frame: F,
}

impl<F> TimestampedFrame<F> {
    /// Create a new timestamped frame.
    pub fn new(timestamp_us: u64, frame: F) -> Self {
        Self { timestamp_us, frame }
    }
}
