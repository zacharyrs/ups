use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

pub struct Mailer {
    from: String,
    vec_to: Vec<String>,
    relay: SmtpTransport,
}

impl Mailer {
    pub fn new(
        username: String,
        password: String,
        relay: &str,
        from: String,
        vec_to: Vec<String>,
    ) -> Mailer {
        return Mailer {
            from,
            vec_to,
            relay: SmtpTransport::relay(relay)
                .unwrap()
                .credentials(Credentials::new(username, password))
                .build(),
        };
    }

    pub fn send(&self, subject: &str, message: &str) {
        let mut builder = Message::builder().from(self.from.parse().unwrap());
        for to in &self.vec_to {
            builder = builder.to(to.parse().unwrap())
        }
        let email = builder.subject(subject).body(message.to_string()).unwrap();

        if let Err(e) = self.relay.send(&email) {
            eprintln!("Failed to send email: {:?}", e)
        }
    }
}
