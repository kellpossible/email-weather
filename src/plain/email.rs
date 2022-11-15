use serde::{Deserialize, Serialize};

use crate::{
    email,
    gis::Position,
    receive::{self, from_account, message_id, ParseReceivedEmail, text_body},
};

/// A plain text email that was received.
#[derive(Debug, Deserialize, Serialize)]
pub struct Received {
    /// Address that this email was received from.
    pub from: email::Account,
    /// Identifier for the received message, will be used to specify the reply.
    pub message_id: Option<String>,
    /// Subject of the received email.
    pub subject: Option<String>,
    /// The body of the received email.
    pub body: String,
}

impl receive::Received for Received {
    fn position(&self) -> Option<Position> {
        None
    }

    fn request_message(&self) -> &str {
        &self.body
    }
}

impl ParseReceivedEmail for Received {
    type Err = eyre::Error;

    fn parse_email(message: mail_parser::Message) -> Result<Self, Self::Err> {
        let from = from_account(&message)?;
        let message_id = message_id(&message).map(|id| id.to_string());
        let subject = match message.get_header("Subject") {
            Some(subject_header) => match subject_header {
                mail_parser::HeaderValue::Text(text) => Some(text.to_string()),
                mail_parser::HeaderValue::Empty => None,
                _ => {
                    return Err(eyre::eyre!(
                        "Unexpected subject header: {:?}",
                        subject_header
                    ))
                }
            },
            None => None,
        };
        let body = text_body(&message)?.to_string();

        Ok(Self {
            from,
            message_id,
            subject,
            body,
        })
    }
}
