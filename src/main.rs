mod mailer;
mod status;
mod ups;

use std::{
    path::PathBuf,
    process::{exit, Command},
    thread, time,
};

use clap::Parser;
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};

// The following define polling behaviour and shutdown behaviour.
const POLL_DELAY: u64 = 10; // Seconds to wait between polls.
const UTILITY_FAILED_POLL_DELAY: u64 = 1; // Seconds to wait between polls while utility is failed.
const COMMUNICATION_FAILED_POLL_DELAY: u64 = 2; //Seconds to wait between polls if communication failed.
const SECONDS_TO_SHUTDOWN: i32 = 30; // Seconds to wait before shutting down.
const BATTERY_LOW_THRESHOLD: u8 = 50; // Threshold capacity for a low battery.
const MINUTES_TO_SHUTDOWN: f32 = 2.0; // Time to wait for PC to shutdown before UPS shuts down.
const MINUTES_TO_RESTART: i32 = 0; // Time after shutdown before restart. 0 means no restart.

#[derive(Deserialize, Serialize, Debug)]
struct UpsSettings {
    // Configuration for the actual UPS communication, with the above definitions.
    poll_delay: u64,
    utility_failed_poll_delay: u64,
    communication_failed_poll_delay: u64,
    seconds_to_shutdown: i32,
    battery_low_threshold: u8,
    minutes_to_shutdown: f32,
    minutes_to_restart: i32,
}

impl Default for UpsSettings {
    fn default() -> Self {
        UpsSettings {
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

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Path to mailer settings toml file
    #[clap(
        short,
        long,
        value_parser,
        default_value = "/etc/ups/mailer.toml",
        value_name = "FILE"
    )]
    mailer_settings_path: PathBuf,

    /// Path to optional UPS settings toml file
    #[clap(
        short,
        long,
        value_parser,
        default_value = "/etc/ups/ups.toml",
        value_name = "FILE"
    )]
    ups_settings_path: PathBuf,
}

// Helpers to shut down specific OS candidates
fn linux_shutdown() {
    Command::new("/bin/sudo")
        .arg("/sbin/halt")
        .output()
        .unwrap();
}

fn windows_shutdown() {
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
        // Don't actually shut down in debug builds.
        println!("In debug build, not shutting down.")
    } else {
        if let Ok(_) = ups.shutdown(minutes_to_shutdown, minutes_to_restart) {
            // Inform the UPS to shut down after we have
            println!("Set UPS to shutdown in {}M.", minutes_to_shutdown)
        } else {
            eprintln!("Failed to set UPS to shutdown in {}M.", minutes_to_shutdown)
        }

        // Now shut down the system
        println!("Shutting down.");
        if cfg!(unix) {
            linux_shutdown()
        } else if cfg!(windows) {
            windows_shutdown()
        }
    }

    // Friendly exit for Rust's sake, but we'd never actually get here in production...
    exit(0)
}

fn main() {
    // Use the cli to make config paths configurable
    let cli = Cli::parse();

    // Load in the optional ups config, merging with defaults.
    let ups_settings: UpsSettings = Figment::from(Serialized::defaults(UpsSettings::default()))
        .merge(Toml::file(cli.ups_settings_path))
        .extract()
        .expect("Failed to read ups config");

    // Load in the mailer config - this one is mandatory.
    let mailer_settings: mailer::MailerSettings = Figment::new()
        .merge(Toml::file(cli.mailer_settings_path))
        .extract()
        .expect("Failed to read smtp config");

    if cfg!(debug_assertions) {
        // Print our config in debug mode.
        println!("{:#?}", ups_settings);
        println!("{:#?}", mailer_settings);
    }

    // Initialise the UPS connection and mailer.
    let api: HidApi = HidApi::new().expect("Failed to initialise HIDAPI");
    let mut ups = ups::UPS::new(api);
    let mailer = mailer::Mailer::new(mailer_settings);

    println!("UPS monitor running and connected!");

    // And now enter the endless checking loop...
    let mut sent_utility_failed: bool = false;
    let mut seconds_until_shutdown: i32 = ups_settings.seconds_to_shutdown;
    let mut poll_delay: u64;
    loop {
        if let Err(e) = ups.get_ups_status() {
            mailer.send(
                &format!(
                    "UPS communication failed - retrying in {}.",
                    ups_settings.communication_failed_poll_delay
                ),
                &format!("{:#?}\n{:#?}", e, ups.status).to_string(),
            );

            thread::sleep(time::Duration::from_secs(
                ups_settings.communication_failed_poll_delay,
            ));
            ups.connect();

            if let Err(e) = ups.get_ups_status() {
                mailer.send(
                    "UPS communication failed - shutting down.",
                    &format!("{:#?}\n{:#?}", e, ups.status).to_string(),
                );

                shutdown(
                    &ups,
                    ups_settings.minutes_to_shutdown,
                    ups_settings.minutes_to_restart,
                );
            } else {
                mailer.send(
                    &format!("UPS communication restored.",),
                    &format!("{:#?}\n{:#?}", e, ups.status).to_string(),
                );
            }
        }

        if cfg!(debug_assertions) {
            println!("{:#?}", ups.status);
        }

        if ups.status.utility_failed {
            poll_delay = ups_settings.utility_failed_poll_delay;
            seconds_until_shutdown -= poll_delay as i32;

            if !sent_utility_failed {
                mailer.send("Utility failed.", &format!("{:#?}", ups.status).to_string());
                sent_utility_failed = true;
            }
            if seconds_until_shutdown <= 0 {
                mailer.send(
                    "Utility failed - shutting down.",
                    &format!(
                        "UPS has {}s remaining, will shutdown in {}min.\n{:#?}",
                        ups.status.seconds_to_empty, ups_settings.minutes_to_shutdown, ups.status
                    )
                    .to_string(),
                );

                shutdown(
                    &ups,
                    ups_settings.minutes_to_shutdown,
                    ups_settings.minutes_to_restart,
                );
            } else {
                eprintln!("Utility failed - shutdown in {}s.", seconds_until_shutdown)
            }
        } else {
            poll_delay = ups_settings.poll_delay;
            seconds_until_shutdown = ups_settings.seconds_to_shutdown;

            if sent_utility_failed {
                mailer.send(
                    "Utility restored.",
                    &format!("{:#?}", ups.status).to_string(),
                );
                sent_utility_failed = false;
            }
        }

        if ups.status.fault {
            mailer.send(
                "Fault detected - shutting down.",
                &format!("{:#?}", ups.status).to_string(),
            );

            shutdown(
                &ups,
                ups_settings.minutes_to_shutdown,
                ups_settings.minutes_to_restart,
            );
        }

        if ups.status.overloaded {
            mailer.send(
                "UPS overloaded - shutting down.",
                &format!("{:#?}", ups.status).to_string(),
            );

            shutdown(
                &ups,
                ups_settings.minutes_to_shutdown,
                ups_settings.minutes_to_restart,
            );
        }

        if ups.status.replace_battery {
            mailer.send(
                "Battery needs replacement - shutting down.",
                &format!("{:#?}", ups.status).to_string(),
            );

            shutdown(
                &ups,
                ups_settings.minutes_to_shutdown,
                ups_settings.minutes_to_restart,
            );
        }

        if ups.status.remaining_capacity < ups_settings.battery_low_threshold {
            if ups.status.charging {
                mailer.send(
                    "Battery low capacity.",
                    &format!("{:#?}", ups.status).to_string(),
                );
            } else {
                mailer.send(
                    "Battery low capacity and not charging - shutting down.",
                    &format!("{:#?}", ups.status).to_string(),
                );
            }
        }

        thread::sleep(time::Duration::from_secs(poll_delay));
    }
}
