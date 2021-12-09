mod mailer;
mod status;
mod ups;

use std::{
    process::{exit, Command},
    thread, time,
};

use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};

// The following define polling behaviour and shutdown behaviour.
const POLL_DELAY: u64 = 10; // Seconds to wait between polls.
const UTILITY_FAILED_POLL_DELAY: u64 = 1; // Seconds to wait between polls while utility is failed.
const COMMUNICATION_FAILED_POLL_DELAY: u64 = 5; //Seconds to wait between polls if communication failed.
const SECONDS_TO_SHUTDOWN: i32 = 30; // Seconds to wait before shutting down.
const BATTERY_LOW_THRESHOLD: u8 = 50; // Threshold capacity for a low battery.
const MINUTES_TO_SHUTDOWN: f32 = 2.0; // Time to wait for PC to shutdown before UPS shuts down.
const MINUTES_TO_RESTART: i32 = 0; // Time after shutdown before restart. 0 means no restart.

#[derive(Deserialize, Serialize, Debug)]
struct Settings {
    poll_delay: u64,
    utility_failed_poll_delay: u64,
    communication_failed_poll_delay: u64,
    seconds_to_shutdown: i32,
    battery_low_threshold: u8,
    minutes_to_shutdown: f32,
    minutes_to_restart: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            poll_delay: POLL_DELAY,
            utility_failed_poll_delay: UTILITY_FAILED_POLL_DELAY,
            communication_failed_poll_delay: COMMUNICATION_FAILED_POLL_DELAY,
            seconds_to_shutdown: SECONDS_TO_SHUTDOWN,
            battery_low_threshold: BATTERY_LOW_THRESHOLD,
            minutes_to_shutdown: MINUTES_TO_SHUTDOWN,
            minutes_to_restart: MINUTES_TO_RESTART,
        }
    }
}

fn linux_shutdown() {
    println!("Shutting down.");
    Command::new("/usr/sbin/shutdown")
        .arg("-h")
        .arg("now")
        .output()
        .unwrap();
}

fn windows_shutdown() {
    println!("Shutting down.");
    Command::new("C:\\Windows\\System32\\shutdown.exe")
        .arg("/s")
        .arg("/f")
        .arg("/t")
        .arg("0")
        .output()
        .unwrap();
}

fn shutdown(ups: &ups::UPS, minutes_to_shutdown: f32, minutes_to_restart: i32) {
    if cfg!(debug_assertions) {
        println!("In debug build, not shutting down.")
    } else {
        if let Ok(_) = ups.shutdown(minutes_to_shutdown, minutes_to_restart) {
            eprintln!("Set UPS to shutdown in {}M.", minutes_to_shutdown)
        }

        if cfg!(unix) {
            linux_shutdown()
        } else if cfg!(windows) {
            windows_shutdown()
        }
    }

    exit(0)
}

fn main() {
    let settings: Settings = Figment::from(Serialized::defaults(Settings::default()))
        .merge(Toml::file("ups.toml"))
        .extract()
        .expect("Failed to read ups config.");

    let mailer_settings: mailer::MailerSettings = Figment::new()
        .merge(Toml::file("mailer.toml"))
        .extract()
        .expect("Failed to read smtp config.");

    if cfg!(debug_assertions) {
        println!("{:#?}", settings);
        println!("{:#?}", mailer_settings);
    }

    let api: HidApi = HidApi::new().expect("Failed to initialise HIDAPI.");
    let mut ups = ups::UPS::new(api);
    let mailer = mailer::Mailer::new(mailer_settings);

    println!("UPS monitor running and connected!");

    let mut sent_utility_failed: bool = false;
    let mut seconds_until_shutdown: i32 = settings.seconds_to_shutdown;
    let mut poll_delay: u64;
    loop {
        if let Err(e) = ups.get_ups_status() {
            eprintln!(
                "UPS communication failed - retrying in {}.",
                settings.communication_failed_poll_delay
            );
            mailer.send(
                &format!(
                    "UPS communication failed - retrying in {}.",
                    settings.communication_failed_poll_delay
                ),
                &format!("{:#?}\n{:#?}", e, ups.status).to_string(),
            );

            thread::sleep(time::Duration::from_secs(
                settings.communication_failed_poll_delay,
            ));
            ups.connect();

            if let Err(e) = ups.get_ups_status() {
                eprintln!("UPS communication failed again - shutting down.");
                mailer.send(
                    "UPS communication failed - shutting down.",
                    &format!("{:#?}\n{:#?}", e, ups.status).to_string(),
                );

                shutdown(
                    &ups,
                    settings.minutes_to_shutdown,
                    settings.minutes_to_restart,
                );
            }
        }

        if cfg!(debug_assertions) {
            println!("{:#?}", ups.status);
        }

        if ups.status.utility_failed {
            poll_delay = settings.utility_failed_poll_delay;
            seconds_until_shutdown -= poll_delay as i32;

            if !sent_utility_failed {
                eprintln!("Utility failed.");
                mailer.send("Utility failed.", &format!("{:#?}", ups.status).to_string());
                sent_utility_failed = true;
            }
            thread::sleep(time::Duration::from_secs(poll_delay));
            if seconds_until_shutdown <= 0 {
                eprintln!(
                    "Shutting down, UPS has {}s remaining, will shutdown in {}min.",
                    ups.status.seconds_to_empty, settings.minutes_to_shutdown
                );
                mailer.send(
                    "Utility failed - shutting down.",
                    &format!(
                        "UPS has {}s remaining, will shutdown in {}min.\n{:#?}",
                        ups.status.seconds_to_empty, settings.minutes_to_shutdown, ups.status
                    )
                    .to_string(),
                );

                shutdown(
                    &ups,
                    settings.minutes_to_shutdown,
                    settings.minutes_to_restart,
                );
            } else {
                eprintln!("Utility failed - shutdown in {}s.", seconds_until_shutdown)
            }
        } else {
            poll_delay = settings.poll_delay;
            seconds_until_shutdown = settings.seconds_to_shutdown;

            if sent_utility_failed {
                eprintln!("Utility back.");
                mailer.send("Utility back.", &format!("{:#?}", ups.status).to_string());
                sent_utility_failed = false;
            }
        }

        if ups.status.fault {
            eprintln!("Fault detected - shutting down.");
            mailer.send(
                "Fault detected - shutting down.",
                &format!("{:#?}", ups.status).to_string(),
            );

            shutdown(
                &ups,
                settings.minutes_to_shutdown,
                settings.minutes_to_restart,
            );
        }

        if ups.status.overloaded {
            eprintln!("UPS overloaded - shutting down.");
            mailer.send(
                "UPS overloaded - shutting down.",
                &format!("{:#?}", ups.status).to_string(),
            );

            shutdown(
                &ups,
                settings.minutes_to_shutdown,
                settings.minutes_to_restart,
            );
        }

        if ups.status.replace_battery {
            eprintln!("Battery needs replacement - shutting down.");
            mailer.send(
                "Battery needs replacement - shutting down.",
                &format!("{:#?}", ups.status).to_string(),
            );

            shutdown(
                &ups,
                settings.minutes_to_shutdown,
                settings.minutes_to_restart,
            );
        }

        if ups.status.remaining_capacity < settings.battery_low_threshold {
            if ups.status.charging {
                eprintln!("Battery low capacity.");
                mailer.send(
                    "Battery low capacity.",
                    &format!("{:#?}", ups.status).to_string(),
                );
            } else {
                eprintln!("Battery low capacity and not charging - shutting down.");
                mailer.send(
                    "Battery low capacity and not charging - shutting down.",
                    &format!("{:#?}", ups.status).to_string(),
                );
            }
        }

        thread::sleep(time::Duration::from_secs(poll_delay));
    }
}
