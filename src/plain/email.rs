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
