//! FlexRay Bus Logging Example
//!
//! This example demonstrates how to log FlexRay frames to MDF4 files
//! using the ASAM MDF4 Bus Logging specification.
//!
//! FlexRay is a high-speed, deterministic automotive bus used for
//! safety-critical applications like brake-by-wire and steer-by-wire.
//!
//! Run with: `cargo run --example flexray_logging --features std`

use mdf4_rs::flexray::{FlexRayChannel, FlexRayFrame, RawFlexRayLogger};

fn main() -> mdf4_rs::Result<()> {
    println!("=== FlexRay Bus Logging Example ===\n");

    // Example 1: Basic FlexRay frame logging
    basic_logging()?;

    // Example 2: Static and dynamic segment frames
    segment_types()?;

    // Example 3: Dual-channel communication
    dual_channel()?;

    println!("\n=== All examples completed successfully! ===");
    Ok(())
}

/// Basic FlexRay frame logging
fn basic_logging() -> mdf4_rs::Result<()> {
    println!("--- Example 1: Basic FlexRay Logging ---");

    let mut logger = RawFlexRayLogger::with_cluster_name("Chassis_FR")?;

    let mut timestamp = 0u64;

    // Wheel speed data (static segment, slot 10)
    let wheel_speeds = FlexRayFrame::new(
        10,
        0,
        FlexRayChannel::A,
        vec![
            0x00, 0x50, // Front Left: 80 km/h
            0x00, 0x50, // Front Right: 80 km/h
            0x00, 0x4F, // Rear Left: 79 km/h
            0x00, 0x51, // Rear Right: 81 km/h
        ],
    );
    logger.log_frame(timestamp, wheel_speeds);
    timestamp += 5_000; // 5ms cycle time

    // Brake pressure data (static segment, slot 11)
    let brake_pressure = FlexRayFrame::new(
        11,
        0,
        FlexRayChannel::A,
        vec![
            0x00, 0x00, // Front brake pressure
            0x00, 0x00, // Rear brake pressure
        ],
    );
    logger.log_frame(timestamp, brake_pressure);
    timestamp += 5_000;

    // Steering angle (static segment, slot 12)
    let steering = FlexRayFrame::new(
        12,
        0,
        FlexRayChannel::A,
        vec![
            0x00, 0x00, // Center position
            0x00, 0x10, // Angular velocity
        ],
    );
    logger.log_frame(timestamp, steering);

    println!("  Logged {} frames", logger.total_frame_count());
    println!("  Unique slots: {}", logger.unique_slot_count());

    let mdf_bytes = logger.finalize()?;
    println!("  MDF size: {} bytes", mdf_bytes.len());

    Ok(())
}

/// Static and dynamic segment frame types
fn segment_types() -> mdf4_rs::Result<()> {
    println!("\n--- Example 2: Static and Dynamic Segments ---");

    let mut logger = RawFlexRayLogger::with_cluster_name("Powertrain_FR")?;

    let mut timestamp = 0u64;
    let mut cycle = 0u8;

    // Simulate several FlexRay cycles
    for _ in 0..4 {
        // Static segment frames (guaranteed bandwidth, slots 1-100)
        // These are sent every cycle

        // Engine torque (slot 5, startup frame)
        let mut engine_torque = FlexRayFrame::new(
            5,
            cycle,
            FlexRayChannel::A,
            vec![
                0x00, 0x64, // Torque: 100 Nm
                0x00, 0x10, // Engine state
            ],
        );
        engine_torque.flags = engine_torque.flags.with_startup(true);
        logger.log_frame(timestamp, engine_torque);
        timestamp += 1_000;

        // Transmission state (slot 6, sync frame)
        let mut trans_state = FlexRayFrame::new(
            6,
            cycle,
            FlexRayChannel::A,
            vec![
                0x03, // Gear: D
                0x00, // Mode: Normal
            ],
        );
        trans_state.flags = trans_state.flags.with_sync(true);
        logger.log_frame(timestamp, trans_state);
        timestamp += 1_000;

        // Dynamic segment frames (variable bandwidth, slots 101+)
        // Only sent when data changes

        if cycle % 2 == 0 {
            // Diagnostic request (dynamic slot 150, only even cycles)
            let diag_request = FlexRayFrame::new(
                150,
                cycle,
                FlexRayChannel::A,
                vec![
                    0x22, 0x01, 0x00, // Read DID 0x0100
                ],
            );
            logger.log_frame(timestamp, diag_request);
            timestamp += 1_000;
        }

        cycle = (cycle + 1) % 64; // FlexRay has 64 cycles (0-63)
        timestamp += 2_000; // Rest of cycle time
    }

    println!("  Logged {} frames", logger.total_frame_count());
    println!(
        "  Channel A frames: {}",
        logger.channel_frame_count(FlexRayChannel::A)
    );

    let mdf_bytes = logger.finalize()?;
    println!("  MDF size: {} bytes", mdf_bytes.len());

    Ok(())
}

/// Dual-channel FlexRay communication
fn dual_channel() -> mdf4_rs::Result<()> {
    println!("\n--- Example 3: Dual-Channel Communication ---");

    let mut logger = RawFlexRayLogger::with_cluster_name("Safety_FR")?;

    let mut timestamp = 0u64;

    // Safety-critical data is sent on both channels for redundancy
    // Channel A: Primary
    // Channel B: Redundant

    // ABS control message (sent on both channels)
    let abs_channel_a = FlexRayFrame::new(
        20,
        0,
        FlexRayChannel::A,
        vec![
            0x01, // ABS active
            0x00, 0x50, // Target slip ratio
            0xFF, // Valve states
        ],
    );
    logger.log_frame(timestamp, abs_channel_a);

    let abs_channel_b = FlexRayFrame::new(
        20,
        0,
        FlexRayChannel::B,
        vec![
            0x01, // ABS active (redundant)
            0x00, 0x50, // Target slip ratio
            0xFF, // Valve states
        ],
    );
    logger.log_frame(timestamp, abs_channel_b);

    timestamp += 2_500; // 2.5ms

    // ESP control message (Channel AB - transmitted on both)
    let esp_both = FlexRayFrame::new(
        21,
        0,
        FlexRayChannel::AB,
        vec![
            0x00, // ESP intervention: none
            0x00, 0x00, // Yaw rate
            0x00, 0x00, // Lateral acceleration
        ],
    );
    logger.log_frame(timestamp, esp_both);

    timestamp += 2_500;

    // Airbag status (Channel A only - lower priority)
    let airbag = FlexRayFrame::new(
        30,
        0,
        FlexRayChannel::A,
        vec![
            0xFF, // All sensors OK
            0x00, // No deployment
        ],
    );
    logger.log_frame(timestamp, airbag);

    timestamp += 2_500;

    // Null frame (no data to send in this slot)
    let null_frame = FlexRayFrame::null_frame(40, 1, FlexRayChannel::A);
    logger.log_frame(timestamp, null_frame);

    println!("  Logged {} frames", logger.total_frame_count());
    println!(
        "  Channel A frames: {}",
        logger.channel_frame_count(FlexRayChannel::A)
    );
    println!(
        "  Channel B frames: {}",
        logger.channel_frame_count(FlexRayChannel::B)
    );
    println!(
        "  Channel AB frames: {}",
        logger.channel_frame_count(FlexRayChannel::AB)
    );
    println!("  TX frames: {}", logger.tx_frame_count());
    println!("  RX frames: {}", logger.rx_frame_count());

    let mdf_bytes = logger.finalize()?;
    println!("  MDF size: {} bytes", mdf_bytes.len());

    Ok(())
}
