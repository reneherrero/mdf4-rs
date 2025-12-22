//! CAN integration tests for ASAM MDF4 Bus Logging format.

mod asam_format;
mod dbc_logger;

/// Complete DBC for testing (matches can-frame-generator).
pub const COMPLETE_DBC: &str = r#"VERSION "2.0"

NS_ :

BS_:

BU_: ECM TCM BCM ABS SENSOR

BO_ 256 EngineData : 8 ECM
 SG_ RPM : 7|16@0+ (0.25,0) [0|8000] "rpm" TCM BCM
 SG_ Temperature : 23|8@0- (1,-40) [-40|215] "C" TCM BCM ABS
 SG_ ThrottlePosition : 31|8@0+ (0.392157,0) [0|100] "%" *
 SG_ OilPressure : 32|16@1+ (0.01,0) [0|1000] "kPa" TCM

BO_ 512 TransmissionData : 8 TCM
 SG_ GearPosition : 7|8@0+ (1,0) [0|5] "" BCM
 SG_ ClutchEngaged : 8|1@0+ (1,0) [0|1] "" ECM
 SG_ Torque : 16|16@1- (0.1,0) [-3276.8|3276.7] "Nm" ECM BCM
 SG_ TransmissionTemp : 39|8@0- (1,-40) [-40|215] "C" ECM

BO_ 768 BrakeData : 6 ABS
 SG_ BrakePressure : 0|16@1+ (0.1,0) [0|1000] "bar" ECM BCM
 SG_ ABSActive : 16|1@0+ (1,0) [0|1] "" ECM
 SG_ WheelSpeedFL : 17|15@1+ (0.01,0) [0|327.67] "km/h" ECM
 SG_ WheelSpeedFR : 32|15@1+ (0.01,0) [0|327.67] "km/h" ECM

BO_ 1024 SensorData : 6 SENSOR
 SG_ Voltage : 7|16@0+ (0.01,0) [0|20] "V" ECM TCM
 SG_ Current : 23|16@0- (0.001,0) [-32.768|32.767] "A" ECM
 SG_ Humidity : 39|8@0+ (0.5,0) [0|127.5] "%" BCM
"#;
