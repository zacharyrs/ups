use crate::status;

use hidapi::{HidApi, HidDevice, HidError};
use std::{
    fmt,
    num::{ParseFloatError, ParseIntError},
    str::Utf8Error,
    thread, time,
};

// The UPS uses ASCII characters for communication.
const TERMINATOR: u8 = 13; // Carriage return
const SEPARATOR: u8 = 32; // Space
const PROTOCOL_ID: u8 = 72; // 'H'

// Messages received are at most 8 values.
// Longer messages are hence split with the above terminator.
const MAX_DATA_LENGTH: usize = 8;

// We use an arbitrary max number of messages to try receive.
const MAX_DATA_LOOP: usize = 20;

const TIMEOUT: i32 = 500;
const RETRIES: usize = 3;

#[derive(Debug)]
pub enum UPSError {
    Timeout,
    Unknown,
    Hid(HidError),
    ParseInt(ParseIntError),
    ParseFloat(ParseFloatError),
    Utf8(Utf8Error),
}
impl fmt::Display for UPSError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Issue with UPS communication")
    }
}
impl From<HidError> for UPSError {
    fn from(err: HidError) -> UPSError {
        UPSError::Hid(err)
    }
}
impl From<ParseIntError> for UPSError {
    fn from(err: ParseIntError) -> UPSError {
        UPSError::ParseInt(err)
    }
}
impl From<ParseFloatError> for UPSError {
    fn from(err: ParseFloatError) -> UPSError {
        UPSError::ParseFloat(err)
    }
}
impl From<Utf8Error> for UPSError {
    fn from(err: Utf8Error) -> UPSError {
        UPSError::Utf8(err)
    }
}

pub struct UPS {
    api: HidApi,
    device: Option<HidDevice>,
    pub status: status::UPSStatus,
}

impl UPS {
    pub fn new(api: hidapi::HidApi) -> UPS {
        // Create our UPS structure.
        let mut ups: UPS = UPS {
            api,
            device: None,
            status: status::UPSStatus::new(),
        };

        ups.connect();

        // Update with the rated values and current status.
        ups.get_ups_ratings().expect("Failed to read UPS ratings");
        ups.get_ups_status().expect("Failed to update UPS status");

        return ups;
    }

    pub fn connect(&mut self) {
        if self.device.is_some() {
            self.device = None;
        }

        // This vid:pid should narrow down to our UPS
        let device: HidDevice = self.api.open(0x0665, 0x5161).expect("Failed to find UPS");
        self.device = Some(device);

        // Check the protocol is right.
        self.send_command("M")
            .expect("Failed to query UPS protocol version");
        let mut res: Vec<u8> = Vec::new();
        self.get_response(&mut res, None)
            .expect("Failed to read UPS protocol version");

        assert!(
            res[0] == PROTOCOL_ID,
            "UPS returned incorrect protocol identifier."
        );
    }

    fn send_command(&self, cmd: &str) -> Result<(), UPSError> {
        if let Some(device) = &self.device {
            // We first read a few times to make sure there's no partial messages waiting.
            for i in 0..MAX_DATA_LOOP {
                if cfg!(debug_assertions) {
                    println!("CLEAR LOOP {}", i);
                }
                // Read one message.
                let bytes_read = device.read_timeout(&mut [0; MAX_DATA_LENGTH], TIMEOUT)?;
                if bytes_read == 0 {
                    break;
                }
                if i == (MAX_DATA_LOOP - 1) {
                    eprintln!("Appears messages may still be waiting on device - may crash.")
                }
            }

            if cfg!(debug_assertions) {
                println!("=====================");
            }
            for chunk in cmd.as_bytes().chunks(MAX_DATA_LENGTH) {
                // We need to prefix with a null byte to specify the USB interface to use.
                // Hence our message is `MAX_DATA_LENGTH` + 1.
                let mut message: [u8; MAX_DATA_LENGTH + 1] = [0; MAX_DATA_LENGTH + 1];

                // Now we convert our command to bytes
                for i in 0..chunk.len() {
                    message[i + 1] = chunk[i]
                }

                if cfg!(debug_assertions) {
                    println!(
                        "SEND {:?} {}",
                        message,
                        std::str::from_utf8(&message).unwrap()
                    );
                }

                // And send it off to the UPS.
                device.write(&message)?;
            }

            if cfg!(debug_assertions) {
                println!(
                    "SEND {:?} {}",
                    [0, TERMINATOR],
                    std::str::from_utf8(&[0, TERMINATOR]).unwrap()
                );
            }
            device.write(&[0, TERMINATOR])?;
            Ok(())
        } else {
            return Err(UPSError::Unknown);
        }
    }

    fn get_response(&self, res: &mut Vec<u8>, length: Option<usize>) -> Result<(), UPSError> {
        if let Some(device) = &self.device {
            // We at most `MAX_DATA_LOOP` times (till we read a terminator).
            for i in 0..MAX_DATA_LOOP {
                if cfg!(debug_assertions) {
                    println!("READ LOOP {}", i);
                }

                // Temporary array for data.
                let mut data: [u8; MAX_DATA_LENGTH] = [0; MAX_DATA_LENGTH];

                // Read one message.
                let bytes_read = device.read_timeout(&mut data, TIMEOUT)?;
                if bytes_read == 0 {
                    return Err(UPSError::Timeout);
                }

                if cfg!(debug_assertions) {
                    println!("READ {:?} {}", data, std::str::from_utf8(&data).unwrap());
                }

                // Add character by character to the output, and return on the terminator.
                // Alternately return when message is the right length.
                for c in data {
                    if c == TERMINATOR {
                        return Ok(());
                    }
                    res.push(c);
                    if let Some(l) = length {
                        if res.len() == l {
                            return Ok(());
                        }
                    }
                }
            }
        }

        Err(UPSError::Unknown)
    }

    fn send_and_split(
        &mut self,
        cmd: &str,
        out: &mut Vec<Vec<u8>>,
        length: Option<usize>,
    ) -> Result<(), UPSError> {
        // Set up an array for our data, then send and receive from the UPS.
        let mut data: Vec<u8> = Vec::new();

        for attempt in 0..RETRIES {
            self.send_command(cmd)?;
            match self.get_response(&mut data, length) {
                Ok(_) => break,
                Err(e) => {
                    if matches!(e, UPSError::Timeout) {
                        self.connect();
                        thread::sleep(time::Duration::from_millis(200));
                        if attempt == (RETRIES - 1) {
                            return Err(e);
                        }
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // Strip the first character (a '#' or '(').
        data.remove(0);

        // Loop through the full message and split at `SEPARATOR`, pushing vectors to the output.
        out.push(Vec::new());
        for c in data {
            if c == SEPARATOR {
                out.push(Vec::new());
            } else {
                out.last_mut().unwrap().push(c);
            }
        }

        Ok(())
    }

    pub fn get_ups_ratings(&mut self) -> Result<(), UPSError> {
        let mut res: Vec<Vec<u8>> = Vec::new();
        self.send_and_split("F", &mut res, None)?;
        self.status.rated_output_voltage = std::str::from_utf8(&(res[0]))?.parse()?;
        self.status.rated_output_current = std::str::from_utf8(&res[1])?.parse()?;
        self.status.rated_battery_voltage = std::str::from_utf8(&res[2])?.parse()?;
        self.status.rated_output_frequency = std::str::from_utf8(&res[3])?.parse()?;

        Ok(())
    }

    pub fn get_ups_status(&mut self) -> Result<(), UPSError> {
        let mut res: Vec<Vec<u8>> = Vec::new();
        self.send_and_split("QS", &mut res, None)?;
        self.status.input_voltage = std::str::from_utf8(&res[0])?.parse()?;
        self.status.input_fault_voltage = std::str::from_utf8(&res[1])?.parse()?;
        self.status.output_voltage = std::str::from_utf8(&res[2])?.parse()?;
        self.status.output_load = std::str::from_utf8(&res[3])?.parse()?;
        self.status.output_frequency = std::str::from_utf8(&res[4])?.parse()?;
        self.status.battery_voltage = std::str::from_utf8(&res[5])?.parse()?;

        self.status.utility_failed = res[7][0] == b'1';
        self.status.shutdown_active = res[7][6] == b'1';

        let mut res: Vec<Vec<u8>> = Vec::new();
        self.send_and_split("QI", &mut res, Some(48))?;
        self.status.remaining_capacity = std::str::from_utf8(&res[0])?.parse()?;
        self.status.seconds_to_empty = std::str::from_utf8(&res[1])?.parse()?;
        self.status.input_frequency = std::str::from_utf8(&res[2])?.parse()?;
        self.status.output_current = std::str::from_utf8(&res[3])?.parse()?;

        self.status.test_result = match res[7][7] {
            b'1' => status::UPSTestResults::Passed,
            b'2' => status::UPSTestResults::Warning,
            b'3' => status::UPSTestResults::Error,
            b'4' => status::UPSTestResults::Aborted,
            b'5' => status::UPSTestResults::InProgress,
            _ => status::UPSTestResults::NoTest,
        };
        self.status.overloaded = res[7][8] == b'1';
        self.status.replace_battery = res[7][9] == b'1';
        self.status.charging = res[7][10] == b'1';
        self.status.ups_mode = match res[7][12] {
            b'1' => status::UPSModes::Standby,
            b'2' => status::UPSModes::Line,
            b'3' => status::UPSModes::Inverting,
            b'4' => status::UPSModes::SelfTest,
            b'5' => status::UPSModes::Fault,
            _ => status::UPSModes::Idle,
        };

        Ok(())
    }

    pub fn shutdown(&self, delay: f32, restart: i32) -> Result<(), UPSError> {
        let cmd: String;
        if delay < 1.0 {
            cmd = format!("S.{:01}R{:04}", delay * 10.0, restart);
        } else {
            cmd = format!("S{:02}R{:04}", delay, restart);
        }
        self.send_command(cmd.as_str())?;
        Ok(())
    }

    // fn run_test(&self) -> Result<(), UPSError> {
    //     self.send_command("T")?;
    //     thread::sleep(time::Duration::from_secs(10));
    //     Ok(())
    // }

    // fn cancel_shutdown(&self) -> Result<(), UPSError> {
    //     self.send_command("C")?;
    //     Ok(())
    // }

    // fn toggle_beep(&self) -> Result<(), UPSError> {
    //     self.send_command("Q")?;
    //     Ok(())
    // }
}
