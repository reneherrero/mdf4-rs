//! End-to-end integration test: Fake embedded-can Driver -> MDF4 -> Read and Print
//!
//! This test demonstrates the complete workflow:
//! 1. A fake CAN driver generates frames based on a simulated vehicle
//! 2. Frames are decoded using DBC definitions and written to MDF4
//! 3. The MDF4 file is read back and all data is printed

use embedded_can::{Frame, Id, StandardId};
use mdf4_rs::{MDF, Result};
use mdf4_rs::can::DbcMdfLogger;

/// A simple CAN frame implementation for testing
#[derive(Debug, Clone)]
struct MockCanFrame {
    id: Id,
    data: [u8; 8],
    dlc: usize,
}

impl MockCanFrame {
    fn new_standard(id: u16, data: &[u8]) -> Self {
        let mut frame_data = [0u8; 8];
        let len = data.len().min(8);
        frame_data[..len].copy_from_slice(&data[..len]);
        Self {
            id: Id::Standard(StandardId::new(id).unwrap()),
            data: frame_data,
            dlc: len,
        }
    }
}

impl Frame for MockCanFrame {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        if data.len() > 8 {
            return None;
        }
        let mut frame_data = [0u8; 8];
        frame_data[..data.len()].copy_from_slice(data);
        Some(Self {
            id: id.into(),
            data: frame_data,
            dlc: data.len(),
        })
    }

    fn new_remote(id: impl Into<Id>, dlc: usize) -> Option<Self> {
        if dlc > 8 {
            return None;
        }
        Some(Self {
            id: id.into(),
            data: [0u8; 8],
            dlc,
        })
    }

    fn is_extended(&self) -> bool {
        matches!(self.id, Id::Extended(_))
    }

    fn is_remote_frame(&self) -> bool {
        false
    }

    fn id(&self) -> Id {
        self.id
    }

    fn dlc(&self) -> usize {
        self.dlc
    }

    fn data(&self) -> &[u8] {
        &self.data[..self.dlc]
    }
}

/// Simulates a vehicle ECU that generates CAN frames
struct FakeVehicleEcu {
    /// Current engine RPM (0-8000)
    rpm: f64,
    /// Current vehicle speed (0-250 km/h)
    speed: f64,
    /// Current coolant temperature (-40 to 215 °C)
    coolant_temp: f64,
    /// Current gear position (0=Park, 1=R, 2=N, 3=D, 4=Sport)
    gear: u8,
    /// Throttle position (0-100%)
    throttle: f64,
    /// Brake pressure (0-100%)
    brake: f64,
}

impl FakeVehicleEcu {
    fn new() -> Self {
        Self {
            rpm: 800.0,       // Idle RPM
            speed: 0.0,
            coolant_temp: 20.0,
            gear: 0,          // Park
            throttle: 0.0,
            brake: 0.0,
        }
    }

    /// Simulate vehicle state update
    fn update(&mut self, delta_time_s: f64) {
        // Simple physics simulation
        if self.gear >= 3 && self.throttle > 0.0 {
            // Accelerating in Drive
            self.rpm = (self.rpm + self.throttle * 50.0 * delta_time_s).min(7500.0);
            self.speed = (self.speed + self.throttle * 20.0 * delta_time_s).min(200.0);
        } else if self.brake > 0.0 {
            // Braking
            self.speed = (self.speed - self.brake * 30.0 * delta_time_s).max(0.0);
            self.rpm = (800.0 + self.speed * 30.0).min(7500.0);
        } else {
            // Coasting
            self.speed = (self.speed - 5.0 * delta_time_s).max(0.0);
            self.rpm = 800.0 + self.speed * 30.0;
        }

        // Coolant temperature slowly rises when engine is running
        if self.rpm > 0.0 {
            self.coolant_temp = (self.coolant_temp + 0.5 * delta_time_s).min(90.0);
        }
    }

    /// Set throttle position (0.0 to 1.0)
    fn set_throttle(&mut self, value: f64) {
        self.throttle = value.clamp(0.0, 1.0);
        self.brake = 0.0;
    }

    /// Set brake pressure (0.0 to 1.0)
    fn set_brake(&mut self, value: f64) {
        self.brake = value.clamp(0.0, 1.0);
        self.throttle = 0.0;
    }

    /// Set gear position
    fn set_gear(&mut self, gear: u8) {
        self.gear = gear.min(4);
    }

    /// Generate Engine message (CAN ID 0x100)
    /// RPM: 16-bit @ offset 0, factor 0.25
    /// CoolantTemp: 8-bit signed @ offset 16, factor 1, offset -40
    /// ThrottlePos: 8-bit @ offset 24, factor 0.5
    fn generate_engine_frame(&self) -> MockCanFrame {
        let rpm_raw = (self.rpm / 0.25) as u16;
        let temp_raw = ((self.coolant_temp + 40.0) as i16).clamp(0, 255) as u8;
        let throttle_raw = (self.throttle * 100.0 / 0.5) as u8;

        MockCanFrame::new_standard(0x100, &[
            (rpm_raw & 0xFF) as u8,
            ((rpm_raw >> 8) & 0xFF) as u8,
            temp_raw,
            throttle_raw,
            0, 0, 0, 0,
        ])
    }

    /// Generate Vehicle message (CAN ID 0x200)
    /// Speed: 16-bit @ offset 0, factor 0.01
    /// BrakePress: 8-bit @ offset 16, factor 0.5
    fn generate_vehicle_frame(&self) -> MockCanFrame {
        let speed_raw = (self.speed / 0.01) as u16;
        let brake_raw = (self.brake * 100.0 / 0.5) as u8;

        MockCanFrame::new_standard(0x200, &[
            (speed_raw & 0xFF) as u8,
            ((speed_raw >> 8) & 0xFF) as u8,
            brake_raw,
            0, 0, 0, 0, 0,
        ])
    }

    /// Generate Transmission message (CAN ID 0x300)
    /// GearPosition: 8-bit @ offset 0, factor 1
    fn generate_transmission_frame(&self) -> MockCanFrame {
        MockCanFrame::new_standard(0x300, &[
            self.gear,
            0, 0, 0, 0, 0, 0, 0,
        ])
    }
}

/// A fake CAN driver that receives frames from the vehicle ECU
struct FakeCanDriver {
    ecu: FakeVehicleEcu,
    frame_buffer: Vec<(u64, MockCanFrame)>,
    current_time_us: u64,
}

impl FakeCanDriver {
    fn new() -> Self {
        Self {
            ecu: FakeVehicleEcu::new(),
            frame_buffer: Vec::new(),
            current_time_us: 0,
        }
    }

    /// Simulate time passing and generate CAN frames
    fn step(&mut self, delta_time_ms: u64) {
        self.current_time_us += delta_time_ms * 1000;
        self.ecu.update(delta_time_ms as f64 / 1000.0);

        // Generate frames at different rates
        // Engine: every 10ms
        // Vehicle: every 20ms
        // Transmission: every 100ms
        if self.current_time_us % 10_000 == 0 {
            self.frame_buffer.push((self.current_time_us, self.ecu.generate_engine_frame()));
        }
        if self.current_time_us % 20_000 == 0 {
            self.frame_buffer.push((self.current_time_us, self.ecu.generate_vehicle_frame()));
        }
        if self.current_time_us % 100_000 == 0 {
            self.frame_buffer.push((self.current_time_us, self.ecu.generate_transmission_frame()));
        }
    }

    /// Drain all pending frames
    fn drain_frames(&mut self) -> Vec<(u64, MockCanFrame)> {
        std::mem::take(&mut self.frame_buffer)
    }

    /// Access the ECU for control
    fn ecu_mut(&mut self) -> &mut FakeVehicleEcu {
        &mut self.ecu
    }
}

/// DBC definition for our fake vehicle
const VEHICLE_DBC: &str = r#"VERSION "1.0"

NS_ :

BS_:

BU_: ECM TCM VCU

BO_ 256 Engine: 8 ECM
 SG_ RPM : 0|16@1+ (0.25,0) [0|8000] "rpm" Vector__XXX
 SG_ CoolantTemp : 16|8@1+ (1,-40) [-40|215] "degC" Vector__XXX
 SG_ ThrottlePos : 24|8@1+ (0.5,0) [0|100] "%" Vector__XXX

BO_ 512 Vehicle: 8 VCU
 SG_ Speed : 0|16@1+ (0.01,0) [0|300] "km/h" Vector__XXX
 SG_ BrakePress : 16|8@1+ (0.5,0) [0|100] "%" Vector__XXX

BO_ 768 Transmission: 8 TCM
 SG_ GearPosition : 0|8@1+ (1,0) [0|5] "" Vector__XXX

VAL_ 768 GearPosition 0 "Park" 1 "Reverse" 2 "Neutral" 3 "Drive" 4 "Sport" ;
"#;

#[test]
fn end_to_end_can_to_mdf4_integration() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("End-to-End Integration Test: CAN -> DBC Decode -> MDF4 -> Read");
    println!("{}\n", "=".repeat(80));

    // Step 1: Parse DBC and create logger
    println!("Step 1: Parsing DBC file...");
    let dbc = dbc_rs::Dbc::parse(VEHICLE_DBC).expect("Failed to parse DBC");
    println!("  - Found {} messages:", dbc.messages().len());
    for msg in dbc.messages().iter() {
        println!("    - 0x{:03X} {} ({} signals)", msg.id(), msg.name(), msg.signals().iter().count());
    }

    // Step 2: Create MDF logger with full metadata
    println!("\nStep 2: Creating MDF4 logger with DBC integration...");
    let mut logger = DbcMdfLogger::builder(&dbc)
        .store_raw_values(true)
        .include_units(true)
        .include_limits(true)
        .include_conversions(true)
        .include_value_descriptions(true)
        .build()?;
    println!("  - Logger configured for raw value storage with conversions");

    // Step 3: Create fake CAN driver and simulate vehicle
    println!("\nStep 3: Simulating vehicle CAN bus traffic...");
    let mut can_driver = FakeCanDriver::new();

    // Simulate a driving scenario
    let scenario = [
        // (duration_ms, action)
        (500, "Starting in Park, engine idle"),
        (200, "Shift to Drive"),
        (1000, "Accelerating (50% throttle)"),
        (500, "Accelerating (100% throttle)"),
        (500, "Coasting"),
        (500, "Braking (50%)"),
        (300, "Stopped"),
    ];

    let mut total_frames = 0;
    for (duration_ms, action) in &scenario {
        println!("  - {}", action);

        match *action {
            "Shift to Drive" => can_driver.ecu_mut().set_gear(3),
            "Accelerating (50% throttle)" => can_driver.ecu_mut().set_throttle(0.5),
            "Accelerating (100% throttle)" => can_driver.ecu_mut().set_throttle(1.0),
            "Coasting" => {
                can_driver.ecu_mut().set_throttle(0.0);
                can_driver.ecu_mut().set_brake(0.0);
            }
            "Braking (50%)" => can_driver.ecu_mut().set_brake(0.5),
            _ => {}
        }

        // Run simulation in 10ms steps
        for _ in 0..(*duration_ms / 10) {
            can_driver.step(10);

            // Log all generated frames using embedded-can interface
            for (timestamp, frame) in can_driver.drain_frames() {
                if logger.log_frame(timestamp, &frame) {
                    total_frames += 1;
                }
            }
        }
    }

    println!("  - Total frames logged: {}", total_frames);
    println!("  - Frames per message:");
    for msg in dbc.messages().iter() {
        let count = logger.frame_count(msg.id());
        if count > 0 {
            println!("    - 0x{:03X} {}: {} frames", msg.id(), msg.name(), count);
        }
    }

    // Step 4: Finalize and get MDF4 bytes
    println!("\nStep 4: Finalizing MDF4 file...");
    let mdf_bytes = logger.finalize()?;
    println!("  - MDF4 file size: {} bytes", mdf_bytes.len());

    // Step 5: Write to temporary file and read back
    let temp_path = std::env::temp_dir().join("can_integration_test.mf4");
    println!("\nStep 5: Writing to temporary file: {:?}", temp_path);
    std::fs::write(&temp_path, &mdf_bytes)?;

    // Step 6: Read MDF4 file and print contents
    println!("\nStep 6: Reading MDF4 file and printing contents...");
    let mdf = MDF::from_file(temp_path.to_str().unwrap())?;

    println!("\n{}", "=".repeat(80));
    println!("MDF4 FILE CONTENTS");
    println!("{}", "=".repeat(80));

    let groups = mdf.channel_groups();
    println!("\nTotal Channel Groups: {}", groups.len());

    for (gidx, group) in groups.iter().enumerate() {
        let group_name = group.name()?.unwrap_or_else(|| "(unnamed)".to_string());
        println!("\n{}", "-".repeat(60));
        println!("Channel Group [{}]: {}", gidx, group_name);
        println!("{}", "-".repeat(60));

        let channels = group.channels();
        println!("  Channels: {}", channels.len());

        for (cidx, channel) in channels.iter().enumerate() {
            let name = channel.name()?.unwrap_or_else(|| format!("Channel{}", cidx));
            let unit = channel.unit()?.unwrap_or_default();
            let data_type = format!("{:?}", channel.block().data_type);
            let bit_count = channel.block().bit_count;

            print!("\n  [{}] {} ", cidx, name);
            if !unit.is_empty() {
                print!("[{}] ", unit);
            }
            println!("({}, {} bits)", data_type, bit_count);

            match channel.values() {
                Ok(vals) => {
                    let valid_count = vals.iter().filter(|v| v.is_some()).count();
                    println!("      Samples: {}/{} valid", valid_count, vals.len());

                    // Print first few and last few values
                    if !vals.is_empty() {
                        let show_count = 3.min(vals.len());
                        print!("      First {}: ", show_count);
                        for i in 0..show_count {
                            if let Some(val) = &vals[i] {
                                print!("{:?} ", val);
                            } else {
                                print!("None ");
                            }
                        }
                        println!();

                        if vals.len() > show_count * 2 {
                            println!("      ...");
                            print!("      Last {}:  ", show_count);
                            for i in (vals.len() - show_count)..vals.len() {
                                if let Some(val) = &vals[i] {
                                    print!("{:?} ", val);
                                } else {
                                    print!("None ");
                                }
                            }
                            println!();
                        }
                    }
                }
                Err(e) => println!("      Error reading values: {}", e),
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("END OF MDF4 FILE CONTENTS");
    println!("{}\n", "=".repeat(80));

    // Cleanup
    std::fs::remove_file(&temp_path)?;
    println!("Temporary file removed.\n");

    // Verify we got meaningful data
    assert!(groups.len() >= 3, "Expected at least 3 channel groups");
    for group in groups.iter() {
        let channels = group.channels();
        assert!(!channels.is_empty(), "Each group should have channels");
        for channel in channels.iter() {
            let vals = channel.values()?;
            assert!(!vals.is_empty(), "Each channel should have values");
        }
    }

    println!("Integration test PASSED!\n");
    Ok(())
}

/// Test that verifies the mock CAN frame implementation works correctly
#[test]
fn test_mock_can_frame() {
    let frame = MockCanFrame::new_standard(0x100, &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(frame.is_standard());
    assert!(!frame.is_extended());
    assert_eq!(frame.dlc(), 8);
    assert_eq!(frame.data(), &[1, 2, 3, 4, 5, 6, 7, 8]);

    if let Id::Standard(id) = frame.id() {
        assert_eq!(id.as_raw(), 0x100);
    } else {
        panic!("Expected standard ID");
    }
}

/// Test the fake vehicle ECU simulation
#[test]
fn test_fake_vehicle_ecu() {
    let mut ecu = FakeVehicleEcu::new();

    // Initial state
    assert_eq!(ecu.gear, 0); // Park
    assert!((ecu.rpm - 800.0).abs() < 1.0); // Idle RPM

    // Shift to drive and accelerate
    ecu.set_gear(3);
    ecu.set_throttle(0.5);

    // Simulate for 1 second
    for _ in 0..100 {
        ecu.update(0.01);
    }

    // RPM and speed should have increased
    assert!(ecu.rpm > 800.0, "RPM should increase when accelerating");
    assert!(ecu.speed > 0.0, "Speed should increase when accelerating");

    // Apply brakes
    ecu.set_brake(1.0);
    let speed_before_brake = ecu.speed;

    for _ in 0..100 {
        ecu.update(0.01);
    }

    // Speed should decrease
    assert!(ecu.speed < speed_before_brake, "Speed should decrease when braking");
}
