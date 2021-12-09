use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct MailerSettings {
    pub user: String,
    pub pass: String,
    pub relay: String,
    pub from: String,
    pub to: Vec<String>,
    pub machine_id: String,
}

pub struct Mailer {
    from: String,
    vec_to: Vec<String>,
    machine_id: String,
    relay: SmtpTransport,
}

impl Mailer {
    pub fn new(settings: MailerSettings) -> Mailer {
        return Mailer {
            from: settings.from,
            vec_to: settings.to,
            machine_id: settings.machine_id,
            relay: SmtpTransport::relay(&settings.relay)
                .unwrap()
                .credentials(Credentials::new(settings.user, settings.pass))
                .build(),
        };
    }

    pub fn send(&self, subject: &str, message: &str) {
        if cfg!(debug_assertions) {
            println!("In debug build, not sending emails.")
        } else {
            let mut builder = Message::builder().from(self.from.parse().unwrap());
            for to in &self.vec_to {
                builder = builder.to(to.parse().unwrap())
            }
            let email = builder
                .subject(format!("{}: {}", self.machine_id, subject))
                .body(message.to_string())
                .unwrap();

            if let Err(e) = self.relay.send(&email) {
                eprintln!("Failed to send email: {:?}", e)
            }
        }
    }
}
