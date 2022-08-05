# UPS

This repo houses a hastily written rust-based client for the Dynamix Defender 2000 series UPSes.

## Depencies

The client has two dependencies - `libusb-1.0` and `libssl`.
You can install these on Ubuntu-based systems with:

```bash
sudo apt install libusb-1.0-0 libssl
```

## Building

To build the client, we require headers for the above dependencies:
On Ubuntu-based systems, install these with:

```bash
sudo apt install libusb-1.0-0-dev libssl-dev
```

Now build with cargo - note that debug builds will disable emailing and shutdowns.

```bash
cargo build --release
```

## Usage

```text
ups 0.1.1
Zachary Riedlshah <git@zacharyrs.me>
Cross-platform client for Dynamix Defender ups units.

USAGE:
    ups [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

    -m, --mailer-settings-path <FILE>
            Path to mailer settings toml file [default: /usr/local/etc/ups/mailer.toml]

    -u, --ups-settings-path <FILE>
            Path to optional UPS settings toml file [default: /usr/local/etc/ups/ups.toml]

    -V, --version
            Print version information
```

### Permission Issues

If you run Linux, you might need to configure permissions on the USB device so your user can access it.
I do this with a `udev` rule:

```bash
# /etc/udev/rules.d/69-ups.rules
SUBSYSTEMS=="usb", ATTRS{idVendor}=="0665", ATTRS{idProduct}=="5161", TAG+="uaccess", SYMLINK+="ups_usb", TAG+="systemd", GROUP="plugdev", MODE="660"
KERNEL=="hidraw*", ATTRS{idVendor}=="0665", ATTRS{idProduct}=="5161", TAG+="uaccess", SYMLINK+="ups_raw", TAG+="systemd", GROUP="plugdev", MODE="660"
```

### Configuration

There are two configuration files, both in `toml` format.

#### UPS Settings

The first config file handles UPS settings, for which there are some defaults included in the code.
You likely will not need this one, but for reference:

```toml
# /etc/ups/ups.toml
poll_delay = 10 # Seconds to wait between polls.
utility_failed_poll_delay = 1 # Seconds to wait between polls while utility is failed.
communication_failed_poll_delay = 2 # Seconds to wait between polls if communication failed.
seconds_to_shutdown = 30 # Seconds to wait before shutting down.
battery_low_threshold = 50 # Threshold capacity for a low battery.
minutes_to_shutdown = 2.0 # Time to wait for PC to shutdown before UPS shuts down.
minutes_to_restart = 0 # Time after shutdown before restart. 0 means no restart.
```

#### Mailer Settings

The second config file is required and specifies the desired recipients and the SMTP relay.
Leave `user` empty if your relay doesn't require authentication.

```toml
# /etc/ups/mailer.toml
user = "user" # Your smtp relay username.
pass = "pass" # Your smtp relay password.
relay = "relay.example.com" # Your smtp relay address.
from = "ups@example.com" # The 'from' email address.
to = ["dev@example.com", "sysadmin@example.com", "..."] # Your recipient email addresses.
machine_id = "not the hostname" # Optional identifier for the machine, falls back to hostname.
```

### Running as a Service

I run this as a service via `systemd`.
My `ups` user has passwordless `sudo` access to run `/sbin/halt`.
Note the `dev-ups_raw.device`, which refers to the USB device (created by the `udev` rule above).

```text
# /etc/systemd/system/ups.service
[Unit]
Description=UPS Monitor
After=network.target dev-ups_raw.device
Requires=dev-ups_raw.device

[Service]
Type=simple
User=ups
ExecStart=/usr/local/bin/ups

[Install]
WantedBy=multi-user.target
```
