//! Example: Ethernet bus logging to MDF4 files.
//!
//! This example demonstrates logging Ethernet traffic to MDF4 files
//! using the ASAM MDF4 Bus Logging specification.
//!
//! Run with: `cargo run --example ethernet_logging`

use mdf4_rs::MDF;
use mdf4_rs::ethernet::{
    ETH_HEADER_SIZE, EthernetFlags, EthernetFrame, MacAddress, RawEthernetLogger, ethertype,
};

fn main() -> Result<(), mdf4_rs::Error> {
    println!("=== Workflow 1: Raw Ethernet Logging ===\n");
    raw_logging()?;

    println!("\n=== Workflow 2: Structured Frame Logging ===\n");
    structured_logging()?;

    println!("\n=== Workflow 3: Tx/Rx Direction Tracking ===\n");
    direction_tracking()?;

    Ok(())
}

/// Workflow 1: Log raw Ethernet frames from bytes
fn raw_logging() -> Result<(), mdf4_rs::Error> {
    let mut logger = RawEthernetLogger::with_interface_name("eth0")?;

    // Simulate some Ethernet frames (ARP request/reply)
    let frames = [
        // ARP Request: Who has 192.168.1.1?
        create_arp_request(
            [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            [192, 168, 1, 100],
            [192, 168, 1, 1],
        ),
        // ARP Reply
        create_arp_reply([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF], [192, 168, 1, 1]),
        // IPv4 packet
        create_ipv4_packet(
            [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            64,
        ),
    ];

    // Log frames with timestamps
    for (i, frame_bytes) in frames.iter().enumerate() {
        let timestamp_us = (i as u64 + 1) * 1000;
        logger.log(timestamp_us, frame_bytes);
    }

    println!("Logged {} frames", logger.total_frame_count());
    println!("Standard frames: {}", logger.standard_frame_count());
    println!("Jumbo frames: {}", logger.jumbo_frame_count());

    // Finalize and save
    let mdf_bytes = logger.finalize()?;
    let path = std::env::temp_dir().join("ethernet_raw.mf4");
    std::fs::write(&path, &mdf_bytes)?;
    println!("Saved to: {}", path.display());

    // Read back and verify
    let mdf = MDF::from_file(path.to_str().unwrap())?;
    for group in mdf.channel_groups() {
        let name = group.name()?.unwrap_or_default();
        println!("  Group '{}': {} channels", name, group.channels().len());
    }

    Ok(())
}

/// Workflow 2: Log structured Ethernet frames
fn structured_logging() -> Result<(), mdf4_rs::Error> {
    let mut logger = RawEthernetLogger::with_interface_name("Vehicle_ETH")?;

    // Create frames using the EthernetFrame struct
    let src_mac = MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    let dst_mac = MacAddress::broadcast();

    // Create an ARP frame
    let arp_payload = vec![
        0x00, 0x01, // Hardware type: Ethernet
        0x08, 0x00, // Protocol type: IPv4
        0x06, // Hardware size: 6
        0x04, // Protocol size: 4
        0x00, 0x01, // Opcode: Request
        // Rest of ARP payload...
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, // Sender MAC
        192, 168, 1, 100, // Sender IP
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Target MAC (unknown)
        192, 168, 1, 1, // Target IP
    ];

    let frame = EthernetFrame::new(dst_mac, src_mac, ethertype::ARP, arp_payload);
    logger.log_frame(1000, frame);

    // Create an IPv4 frame
    let ipv4_payload = vec![0x45, 0x00, 0x00, 0x28]; // Minimal IP header start
    let ipv4_frame = EthernetFrame::new(
        MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
        src_mac,
        ethertype::IPV4,
        ipv4_payload,
    );
    logger.log_frame(2000, ipv4_frame);

    // Log using components directly
    logger.log_components(
        3000,
        MacAddress::new([0x33, 0x33, 0x00, 0x00, 0x00, 0x01]), // IPv6 multicast
        src_mac,
        ethertype::IPV6,
        &[0x60, 0x00, 0x00, 0x00], // IPv6 header start
    );

    println!("Logged {} frames", logger.total_frame_count());

    let mdf_bytes = logger.finalize()?;
    let path = std::env::temp_dir().join("ethernet_structured.mf4");
    std::fs::write(&path, &mdf_bytes)?;
    println!("Saved to: {}", path.display());

    Ok(())
}

/// Workflow 3: Track Tx/Rx direction
fn direction_tracking() -> Result<(), mdf4_rs::Error> {
    let mut logger = RawEthernetLogger::with_interface_name("eth1")?;

    let frame = create_ipv4_packet(
        [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
        [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        100,
    );

    // Log transmitted frames
    logger.log_tx(1000, &frame);
    logger.log_tx(2000, &frame);

    // Log received frames
    logger.log_rx(1500, &frame);
    logger.log_rx(2500, &frame);
    logger.log_rx(3000, &frame);

    println!("Logged {} total frames", logger.total_frame_count());
    println!("  Tx frames: {}", logger.tx_frame_count());
    println!("  Rx frames: {}", logger.rx_frame_count());

    // Can also use explicit flags
    let flags = EthernetFlags::tx().with_fcs_valid(true);
    logger.log_with_flags(4000, &frame, flags);

    let mdf_bytes = logger.finalize()?;
    let path = std::env::temp_dir().join("ethernet_direction.mf4");
    std::fs::write(&path, &mdf_bytes)?;
    println!("Saved to: {}", path.display());

    Ok(())
}

// Helper functions to create test frames

fn create_arp_request(src_mac: [u8; 6], src_ip: [u8; 4], target_ip: [u8; 4]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ETH_HEADER_SIZE + 28);

    // Ethernet header
    frame.extend_from_slice(&[0xFF; 6]); // Broadcast destination
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ethertype::ARP.to_be_bytes());

    // ARP payload
    frame.extend_from_slice(&[0x00, 0x01]); // Hardware type: Ethernet
    frame.extend_from_slice(&[0x08, 0x00]); // Protocol type: IPv4
    frame.push(6); // Hardware size
    frame.push(4); // Protocol size
    frame.extend_from_slice(&[0x00, 0x01]); // Opcode: Request
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&[0x00; 6]); // Target MAC (unknown)
    frame.extend_from_slice(&target_ip);

    frame
}

fn create_arp_reply(src_mac: [u8; 6], src_ip: [u8; 4]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ETH_HEADER_SIZE + 28);

    // Ethernet header (reply to requester)
    frame.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]); // Destination
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ethertype::ARP.to_be_bytes());

    // ARP payload
    frame.extend_from_slice(&[0x00, 0x01]); // Hardware type: Ethernet
    frame.extend_from_slice(&[0x08, 0x00]); // Protocol type: IPv4
    frame.push(6);
    frame.push(4);
    frame.extend_from_slice(&[0x00, 0x02]); // Opcode: Reply
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    frame.extend_from_slice(&[192, 168, 1, 100]);

    frame
}

fn create_ipv4_packet(src_mac: [u8; 6], dst_mac: [u8; 6], payload_size: usize) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ETH_HEADER_SIZE + 20 + payload_size);

    // Ethernet header
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ethertype::IPV4.to_be_bytes());

    // Minimal IPv4 header (20 bytes)
    let total_len = (20 + payload_size) as u16;
    frame.push(0x45); // Version + IHL
    frame.push(0x00); // DSCP + ECN
    frame.extend_from_slice(&total_len.to_be_bytes()); // Total length
    frame.extend_from_slice(&[0x00; 4]); // ID, flags, fragment
    frame.push(64); // TTL
    frame.push(17); // Protocol: UDP
    frame.extend_from_slice(&[0x00; 2]); // Checksum
    frame.extend_from_slice(&[192, 168, 1, 100]); // Source IP
    frame.extend_from_slice(&[192, 168, 1, 1]); // Dest IP

    // Payload
    frame.extend(std::iter::repeat(0xAA).take(payload_size));

    frame
}
