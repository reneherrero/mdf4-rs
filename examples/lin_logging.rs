//! LIN Bus Logging Example
//!
//! This example demonstrates how to log LIN frames to MDF4 files
//! using the ASAM MDF4 Bus Logging specification.
//!
//! Run with: `cargo run --example lin_logging --features std`

use mdf4_rs::lin::{LinFlags, LinFrame, RawLinLogger};

fn main() -> mdf4_rs::Result<()> {
    println!("=== LIN Bus Logging Example ===\n");

    // Example 1: Basic LIN frame logging with enhanced checksum (LIN 2.x)
    basic_logging()?;

    // Example 2: Classic checksum (LIN 1.x)
    classic_checksum()?;

    // Example 3: Error frame handling
    error_frames()?;

    println!("\n=== All examples completed successfully! ===");
    Ok(())
}

/// Basic LIN frame logging with enhanced checksum
fn basic_logging() -> mdf4_rs::Result<()> {
    println!("--- Example 1: Basic LIN Logging ---");

    let mut logger = RawLinLogger::with_bus_name("Body_LIN")?;

    // Simulate logging frames from various LIN nodes
    let mut timestamp = 0u64;

    // Motor control frame (ID 0x20)
    logger.log(0x20, timestamp, &[0x00, 0x50, 0x00, 0x00]); // Speed = 80 RPM
    timestamp += 10_000; // 10ms interval

    // Sensor data frame (ID 0x21)
    logger.log(0x21, timestamp, &[0x1E, 0x28]); // Temp = 30°C, Humidity = 40%
    timestamp += 10_000;

    // Window position frame (ID 0x10)
    logger.log(0x10, timestamp, &[0x64]); // Window 100% open
    timestamp += 10_000;

    // Seat position frame (ID 0x11)
    logger.log(0x11, timestamp, &[0x50, 0x30, 0x00]); // Position, height, tilt
    timestamp += 10_000;

    // Mirror adjustment frame (ID 0x12)
    logger.log(0x12, timestamp, &[0x7F, 0x7F]); // Center position

    println!("  Logged {} frames", logger.total_frame_count());
    println!("  Unique IDs: {}", logger.unique_id_count());

    let mdf_bytes = logger.finalize()?;
    println!("  MDF size: {} bytes", mdf_bytes.len());

    Ok(())
}

/// LIN 1.x classic checksum example
fn classic_checksum() -> mdf4_rs::Result<()> {
    println!("\n--- Example 2: Classic Checksum (LIN 1.x) ---");

    let mut logger = RawLinLogger::with_bus_name("Legacy_LIN")?;

    let mut timestamp = 0u64;

    // Log frames using classic checksum (data bytes only)
    logger.log_classic(0x30, timestamp, &[0x01, 0x02, 0x03, 0x04]);
    timestamp += 5_000;

    logger.log_classic(0x31, timestamp, &[0xFF, 0x00]);
    timestamp += 5_000;

    logger.log_classic(
        0x32,
        timestamp,
        &[0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
    );

    // Verify checksum calculation
    let frame = LinFrame::with_classic_checksum(0x30, &[0x01, 0x02, 0x03, 0x04]);
    println!("  Frame ID 0x30 classic checksum: 0x{:02X}", frame.checksum);

    // Compare with enhanced checksum
    let frame_enhanced = LinFrame::with_enhanced_checksum(0x30, &[0x01, 0x02, 0x03, 0x04]);
    println!(
        "  Frame ID 0x30 enhanced checksum: 0x{:02X}",
        frame_enhanced.checksum
    );
    println!("  Protected ID for 0x30: 0x{:02X}", frame.protected_id());

    println!("  Logged {} frames", logger.total_frame_count());

    let mdf_bytes = logger.finalize()?;
    println!("  MDF size: {} bytes", mdf_bytes.len());

    Ok(())
}

/// Error frame handling example
fn error_frames() -> mdf4_rs::Result<()> {
    println!("\n--- Example 3: Error Frame Handling ---");

    let mut logger = RawLinLogger::with_bus_name("Debug_LIN")?;

    let mut timestamp = 0u64;

    // Normal frame
    logger.log_tx(0x20, timestamp, &[0x01, 0x02]);
    timestamp += 10_000;

    // Frame with checksum error
    let checksum_error_flags =
        LinFlags::from_byte(LinFlags::CHECKSUM_ERROR | LinFlags::ENHANCED_CHECKSUM);
    logger.log_with_flags(0x21, timestamp, &[0x03, 0x04], checksum_error_flags, 0xFF);
    timestamp += 10_000;

    // No response from slave
    let no_response_flags = LinFlags::from_byte(LinFlags::NO_RESPONSE);
    logger.log_with_flags(0x22, timestamp, &[], no_response_flags, 0x00);
    timestamp += 10_000;

    // Sync error
    let sync_error_flags = LinFlags::from_byte(LinFlags::SYNC_ERROR);
    logger.log_with_flags(0x23, timestamp, &[0x05], sync_error_flags, 0x00);
    timestamp += 10_000;

    // Short response (incomplete data)
    let short_response_flags =
        LinFlags::from_byte(LinFlags::SHORT_RESPONSE | LinFlags::ENHANCED_CHECKSUM);
    logger.log_with_flags(0x24, timestamp, &[0x06, 0x07], short_response_flags, 0x00);
    timestamp += 10_000;

    // Normal received frame
    logger.log_rx(0x25, timestamp, &[0x08, 0x09, 0x0A, 0x0B]);

    println!("  Total frames: {}", logger.total_frame_count());
    println!("  TX frames: {}", logger.tx_frame_count());
    println!("  RX frames: {}", logger.rx_frame_count());
    println!("  Error frames: {}", logger.error_frame_count());

    let mdf_bytes = logger.finalize()?;
    println!("  MDF size: {} bytes", mdf_bytes.len());

    Ok(())
}
