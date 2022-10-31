//! Types and functions relating to geography computation.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Position
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position<CRS = WGS84> {
    /// Latitude of the position (in degrees).
    pub latitude: f32,
    /// Longitude of the position (in degrees).
    pub longitude: f32,
    coordinate_reference_system: PhantomData<CRS>,
}

impl<CRS> Position<CRS> {
    /// Construct a new [`Position`].
    pub fn new(latitude: f32, longitude: f32) -> Position<CRS> {
        Self {
            latitude,
            longitude,
            coordinate_reference_system: PhantomData,
        }
    }
}

/// WGS84 Coordinate system.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct WGS84;
