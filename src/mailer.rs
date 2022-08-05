use hostname;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct MailerSettings {
    // Outline for the expected mailer settings config object
    //
    // Parameters
    // ----------
    // - `user`, `pass` outline connection to the smtp relay given in `relay`
    // - `from` refers to the sender address
    // - `to` can contain multiple recipients
    // - `machine_id` is an optional identifier for the machine
    pub user: String,
    pub pass: String,
    pub relay: String,
    pub from: String,
    pub to: Vec<String>,
    pub machine_id: Option<String>,
}

pub struct Mailer {
    from: String,
    vec_to: Vec<String>,
    machine_id: String,
    transport: SmtpTransport,
}

impl Mailer {
    pub fn new(settings: MailerSettings) -> Mailer {
        let relay = SmtpTransport::relay(&settings.relay).unwrap();

        // Construct a new mailer instance - used to send UPS alerts over SMTP.
        return Mailer {
            from: settings.from,
            vec_to: settings.to,
            // Specify a fallback for `machine_id`, being simply the machine hostname.
            machine_id: settings.machine_id.unwrap_or(
                hostname::get()
                    .expect("Failed to retrieve hostname")
                    .into_string()
                    .expect("Failed to convert hostname to string"),
            ),
            // The actual `SmtpTransport::relay` instance, which internally includes the credentials
            // from the above config.
            transport: if settings.user.is_empty() {
                relay.build()
            } else {
                relay
                    .credentials(Credentials::new(settings.user, settings.pass))
                    .build()
            },
        };
    }

    pub fn send(&self, subject: &str, message: &str) {
        // Send a UPS alert email
        println!("{}", subject);
        if cfg!(debug_assertions) {
            // In debug builds, don't spam anyone!
            println!("In debug build, not sending emails.");
        } else {
            // For production, construct a message from the configured email.
            let mut builder = Message::builder().from(self.from.parse().unwrap());
            // Loop recipients and add them to the mail builder.
            for to in &self.vec_to {
                builder = builder.to(to.parse().unwrap())
            }
            // Finally, set the subject and content, including the `machine_id`.
            let email = builder
                .subject(format!("{}: {}", self.machine_id, subject))
                .body(message.to_string())
                .unwrap();

            // Attempt to send it, print an error if it fails
            if let Err(e) = self.transport.send(&email) {
                eprintln!("Failed to send email: {:?}", e)
            }
        }
    }
}
