//! External weather forecasting service.
//! See [Port].

use async_trait::async_trait;
use open_meteo::{Forecast, ForecastParameters};

/// Trait used to allow mocking the [open_meteo] forecasting service.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Port: Send + Sync {
    /// Obtain a weather forecast using [open_meteo::obtain_forecast()].
    async fn obtain_forecast(
        &self,
        parameters: &ForecastParameters,
    ) -> Result<Forecast, open_meteo::Error>;
}

/// Concrete implementation of [Port].
pub struct Gateway {
    http_client: reqwest::Client,
}

impl Gateway {
    /// Construct a new [Gateway].
    #[must_use]
    pub fn new(http_client: reqwest::Client) -> Self {
        Self { http_client }
    }
}

#[async_trait]
impl Port for Gateway {
    async fn obtain_forecast(
        &self,
        parameters: &ForecastParameters,
    ) -> Result<Forecast, open_meteo::Error> {
        open_meteo::obtain_forecast(&self.http_client, parameters).await
    }
}
