use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{
    gis::Position,
    receive::{self, EmailAddress, ParseReceivedEmail},
};

/// A plain text email that was received.
#[derive(Debug, Deserialize, Serialize)]
pub struct Received {
    /// Requested position for forecast.
    pub position: Position,
    /// Address that this email was received from.
    pub from: EmailAddress,
}

impl receive::Received for Received {
    fn position(&self) -> Position {
        self.position
    }
}

impl ParseReceivedEmail for Received {
    type Err = eyre::Error;

    fn parse_email<'a>(from: EmailAddress, _body: Cow<'a, str>) -> Result<Self, Self::Err> {
        // TODO: parse position from body
        let position = Position::new(-37.8259243, 145.2931204);

        let from = from.clone();

        Ok(Self { position, from })
    }
}
