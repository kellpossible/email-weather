//! External topographical data service.
//! See [Port].
//!
use async_trait::async_trait;
use open_topo_data::{Error, Parameters};

/// Trait used to allow mocking the [open_topo_data] service.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Port: Send + Sync {
    /// Obtain a weather forecast using [open_meteo::obtain_forecast()].
    async fn obtain_elevation(&self, paramters: &Parameters) -> Result<f32, Error>;
}

/// Concrete implementation of [Port].
pub struct Gateway {
    http_client: reqwest::Client,
}

impl Gateway {
    /// Construct a new [Gateway].
    pub fn new(http_client: reqwest::Client) -> Self {
        Self { http_client }
    }
}

#[async_trait]
impl Port for Gateway {
    async fn obtain_elevation(&self, parameters: &Parameters) -> Result<f32, Error> {
        open_topo_data::obtain_elevation(&self.http_client, parameters).await
    }
}
