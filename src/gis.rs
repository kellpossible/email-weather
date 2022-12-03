//! Types and functions relating to geography computation.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Position
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Position<CRS = WGS84> {
    /// Latitude of the position (in degrees).
    pub latitude: f32,
    /// Longitude of the position (in degrees).
    pub longitude: f32,
    #[serde(skip)]
    coordinate_reference_system: PhantomData<CRS>,
}

impl<CRS> std::fmt::Debug for Position<CRS>
where
    CRS: CoordinateReferenceSystem,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Position<{}>", CRS::name()))
            .field("latitude", &self.latitude)
            .field("longitude", &self.longitude)
            .finish()
    }
}

impl<CRS> Position<CRS> {
    /// Construct a new [`Position`].
    #[must_use]
    pub fn new(latitude: f32, longitude: f32) -> Position<CRS> {
        Self {
            latitude,
            longitude,
            coordinate_reference_system: PhantomData,
        }
    }
}

/// Coorindate reference system.
pub trait CoordinateReferenceSystem {
    /// Display name.
    fn name() -> &'static str;
}

/// WGS84 Coordinate system.
#[derive(PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub struct WGS84;

impl CoordinateReferenceSystem for WGS84 {
    fn name() -> &'static str {
        "WGS84"
    }
}
