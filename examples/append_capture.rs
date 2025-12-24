//! Example: Appending to existing MDF4 captures
//!
//! This example demonstrates how to:
//! 1. Create an initial capture with RawCanLogger
//! 2. Load the capture and append more frames
//! 3. Also shows CanDbcLogger loading raw captures for DBC decoding
//!
//! Run with: cargo run --example append_capture --features std

use mdf4_rs::can::RawCanLogger;
use std::io::Write;

fn main() -> mdf4_rs::Result<()> {
    let temp_path = std::env::temp_dir().join("append_example.mf4");
    let temp_path_str = temp_path.to_str().unwrap();

    println!("=== MDF4 Append Example ===\n");

    // Step 1: Create initial capture
    println!("1. Creating initial capture...");
    {
        let mut logger = RawCanLogger::new()?;

        // Simulate some CAN traffic
        logger.log(0x100, 1_000_000, &[0x01, 0x02, 0x03, 0x04]);
        logger.log(0x100, 2_000_000, &[0x05, 0x06, 0x07, 0x08]);
        logger.log(0x200, 1_500_000, &[0xAA, 0xBB, 0xCC, 0xDD]);
        logger.log(0x200, 2_500_000, &[0x11, 0x22, 0x33, 0x44]);

        println!("   Logged {} frames", logger.total_frame_count());

        let bytes = logger.finalize()?;
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(&bytes)?;
        println!("   Saved to: {}", temp_path_str);
    }

    // Step 2: Load and append more frames
    println!("\n2. Loading capture and appending...");
    {
        let mut logger = RawCanLogger::from_file(temp_path_str)?;

        println!("   Loaded {} frames", logger.loaded_frame_count());
        println!("   Last timestamp: {} µs", logger.last_timestamp_us());

        // Continue from where we left off
        let next_ts = logger.last_timestamp_us();

        // Append new frames with continuing timestamps
        logger.log(0x100, next_ts + 1_000_000, &[0x10, 0x20, 0x30, 0x40]);
        logger.log(0x300, next_ts + 1_500_000, &[0xFF, 0xEE, 0xDD]); // New CAN ID
        logger.log(0x200, next_ts + 2_000_000, &[0x99, 0x88, 0x77, 0x66]);

        println!(
            "   Total frames after append: {}",
            logger.total_frame_count()
        );

        // Save the appended capture
        let bytes = logger.finalize()?;
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(&bytes)?;
        println!("   Saved appended capture");
    }

    // Step 3: Verify the result
    println!("\n3. Verifying appended capture...");
    {
        let logger = RawCanLogger::from_file(temp_path_str)?;
        println!("   Final frame count: {}", logger.loaded_frame_count());
        println!("   Final timestamp: {} µs", logger.last_timestamp_us());
        println!("   Unique CAN IDs: {}", logger.unique_id_count());
    }

    // Cleanup
    let _ = std::fs::remove_file(&temp_path);

    println!("\n=== Example complete ===");
    Ok(())
}
