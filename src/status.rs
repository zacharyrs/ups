#[derive(Debug)]
pub enum UPSTestResults {
    NoTest,
    Passed,
    Warning,
    Error,
    Aborted,
    InProgress,
}

#[derive(Debug)]
pub enum UPSModes {
    Idle,
    Standby,
    Line,
    Inverting,
    SelfTest,
    Fault,
}

#[derive(Debug)]
pub struct UPSStatus {
    pub input_voltage: f32,
    pub input_frequency: f32,
    pub input_fault_voltage: f32,

    pub output_voltage: f32,
    pub output_current: f32,
    pub output_frequency: f32,
    pub output_load: u8,

    pub rated_output_voltage: f32,
    pub rated_output_current: i32,
    pub rated_output_frequency: f32,

    pub battery_voltage: f32,
    pub remaining_capacity: u8,
    pub seconds_to_empty: i32,

    pub rated_battery_voltage: f32,

    pub utility_failed: bool,
    pub charging: bool,

    pub shutdown_active: bool,

    pub fault: bool,
    pub overloaded: bool,
    pub replace_battery: bool,

    pub test_result: UPSTestResults,
    pub ups_mode: UPSModes,
}

impl UPSStatus {
    pub fn new() -> UPSStatus {
        return UPSStatus {
            input_voltage: 0.,
            input_frequency: 0.0,
            input_fault_voltage: 0.0,

            output_voltage: 0.0,
            output_current: 0.0,
            output_frequency: 0.0,
            output_load: 0,

            rated_output_voltage: 0.0,
            rated_output_current: 0,
            rated_output_frequency: 0.0,

            battery_voltage: 0.0,
            remaining_capacity: 0,
            seconds_to_empty: 0,

            rated_battery_voltage: 0.0,

            utility_failed: false,
            charging: false,

            shutdown_active: false,

            fault: false,
            overloaded: false,
            replace_battery: false,

            test_result: UPSTestResults::NoTest,
            ups_mode: UPSModes::Idle,
        };
    }
}
