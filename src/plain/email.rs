use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{gis::Position, receive};

#[derive(Debug, Deserialize, Serialize)]
pub struct Email {
    /// Requested position for forecast.
    position: Position,
}

impl receive::Email for Email {
    fn position(&self) -> Position {
        self.position
    }
}

impl FromStr for Email {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        eyre::bail!("TODO")
    }
}
