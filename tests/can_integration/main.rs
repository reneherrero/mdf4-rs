//! CAN integration test module
//!
//! This module contains end-to-end integration tests for CAN bus logging:
//! - `dbc_integration`: Tests with DBC signal decoding
//! - `raw_integration`: Tests without DBC (raw frame capture and later decoding)
//! - `fd_integration`: Tests for CAN FD (Flexible Data-rate) support

mod dbc_integration;
mod fd_integration;
mod raw_integration;

// Shared test utilities
use embedded_can::{Frame, Id, StandardId};

/// A simple CAN frame implementation for testing
#[derive(Debug, Clone)]
pub struct MockCanFrame {
    id: Id,
    data: [u8; 8],
    dlc: usize,
}

impl MockCanFrame {
    pub fn new_standard(id: u16, data: &[u8]) -> Self {
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
pub struct FakeVehicleEcu {
    /// Current engine RPM (0-8000)
    pub rpm: f64,
    /// Current vehicle speed (0-250 km/h)
    pub speed: f64,
    /// Current coolant temperature (-40 to 215 °C)
    pub coolant_temp: f64,
    /// Current gear position (0=Park, 1=R, 2=N, 3=D, 4=Sport)
    pub gear: u8,
    /// Throttle position (0-100%)
    pub throttle: f64,
    /// Brake pressure (0-100%)
    pub brake: f64,
}

impl FakeVehicleEcu {
    pub fn new() -> Self {
        Self {
            rpm: 800.0,
            speed: 0.0,
            coolant_temp: 20.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
        }
    }

    pub fn update(&mut self, delta_time_s: f64) {
        if self.gear >= 3 && self.throttle > 0.0 {
            self.rpm = (self.rpm + self.throttle * 50.0 * delta_time_s).min(7500.0);
            self.speed = (self.speed + self.throttle * 20.0 * delta_time_s).min(200.0);
        } else if self.brake > 0.0 {
            self.speed = (self.speed - self.brake * 30.0 * delta_time_s).max(0.0);
            self.rpm = (800.0 + self.speed * 30.0).min(7500.0);
        } else {
            self.speed = (self.speed - 5.0 * delta_time_s).max(0.0);
            self.rpm = 800.0 + self.speed * 30.0;
        }

        if self.rpm > 0.0 {
            self.coolant_temp = (self.coolant_temp + 0.5 * delta_time_s).min(90.0);
        }
    }

    pub fn set_throttle(&mut self, value: f64) {
        self.throttle = value.clamp(0.0, 1.0);
        self.brake = 0.0;
    }

    pub fn set_brake(&mut self, value: f64) {
        self.brake = value.clamp(0.0, 1.0);
        self.throttle = 0.0;
    }

    pub fn set_gear(&mut self, gear: u8) {
        self.gear = gear.min(4);
    }

    pub fn generate_engine_frame(&self) -> MockCanFrame {
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

    pub fn generate_vehicle_frame(&self) -> MockCanFrame {
        let speed_raw = (self.speed / 0.01) as u16;
        let brake_raw = (self.brake * 100.0 / 0.5) as u8;

        MockCanFrame::new_standard(0x200, &[
            (speed_raw & 0xFF) as u8,
            ((speed_raw >> 8) & 0xFF) as u8,
            brake_raw,
            0, 0, 0, 0, 0,
        ])
    }

    pub fn generate_transmission_frame(&self) -> MockCanFrame {
        MockCanFrame::new_standard(0x300, &[self.gear, 0, 0, 0, 0, 0, 0, 0])
    }
}

/// A fake CAN driver that receives frames from the vehicle ECU
pub struct FakeCanDriver {
    pub ecu: FakeVehicleEcu,
    frame_buffer: Vec<(u64, MockCanFrame)>,
    current_time_us: u64,
}

impl FakeCanDriver {
    pub fn new() -> Self {
        Self {
            ecu: FakeVehicleEcu::new(),
            frame_buffer: Vec::new(),
            current_time_us: 0,
        }
    }

    pub fn step(&mut self, delta_time_ms: u64) {
        self.current_time_us += delta_time_ms * 1000;
        self.ecu.update(delta_time_ms as f64 / 1000.0);

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

    pub fn drain_frames(&mut self) -> Vec<(u64, MockCanFrame)> {
        std::mem::take(&mut self.frame_buffer)
    }
}

/// DBC definition for the fake vehicle
pub const VEHICLE_DBC: &str = r#"VERSION "1.0"

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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_fake_vehicle_ecu() {
        let mut ecu = FakeVehicleEcu::new();

        assert_eq!(ecu.gear, 0);
        assert!((ecu.rpm - 800.0).abs() < 1.0);

        ecu.set_gear(3);
        ecu.set_throttle(0.5);

        for _ in 0..100 {
            ecu.update(0.01);
        }

        assert!(ecu.rpm > 800.0, "RPM should increase when accelerating");
        assert!(ecu.speed > 0.0, "Speed should increase when accelerating");

        ecu.set_brake(1.0);
        let speed_before_brake = ecu.speed;

        for _ in 0..100 {
            ecu.update(0.01);
        }

        assert!(ecu.speed < speed_before_brake, "Speed should decrease when braking");
    }
}
