use serde::{Deserialize, Serialize};

use crate::{
    email,
    gis::Position,
    receive::{self, from_account, message_id, ParseReceivedEmail},
};

/// A plain text email that was received.
#[derive(Debug, Deserialize, Serialize)]
pub struct Received {
    /// Requested position for forecast.
    pub position: Position,
    /// Address that this email was received from.
    pub from: email::Account,
    /// Identifier for the received message, will be used to specify the reply.
    pub message_id: Option<String>,
}

impl receive::Received for Received {
    fn position(&self) -> Position {
        self.position
    }
}

impl ParseReceivedEmail for Received {
    type Err = eyre::Error;

    fn parse_email(message: mail_parser::Message) -> Result<Self, Self::Err> {
        // TODO: parse position from body
        let position = Position::new(-37.8259243, 145.2931204);
        let from = from_account(&message)?;
        let message_id = message_id(&message).map(|id| id.to_string());

        Ok(Self {
            position,
            from,
            message_id,
        })
    }
}
