use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use color_eyre::Help;
use eyre::Context;
use oauth2::{
    basic::BasicClient,
    devicecode::{DeviceAuthorizationResponse, ExtraDeviceAuthorizationFields},
    AccessToken, DeviceAuthorizationUrl, Scope,
};
use serde::{Deserialize, Serialize};

use crate::oauth2::map_request_token_error;

use super::{
    authenticate_with_token_cache, refresh_token, AuthenticationFlow, ClientSecretDefinition,
    StandardTokenResponse,
};

pub struct DeviceFlow {
    client: BasicClient,
    scopes: Vec<Scope>,
    token_cache_path: PathBuf,
}

impl DeviceFlow {
    /// Create a new [`DeviceFlow`].
    pub fn new(
        client_secret: ClientSecretDefinition,
        scopes: Vec<Scope>,
        token_cache_path: impl Into<PathBuf>,
        device_authorization_url: DeviceAuthorizationUrl,
    ) -> Self {
        let client = BasicClient::new(
            client_secret.client_id().clone(),
            Some(client_secret.client_secret().clone()),
            client_secret.auth_url().clone(),
            Some(client_secret.token_url().clone()),
        )
        .set_device_authorization_url(device_authorization_url)
        .set_auth_type(oauth2::AuthType::RequestBody);

        Self {
            client,
            scopes,
            token_cache_path: token_cache_path.into(),
        }
    }
}

#[async_trait]
impl AuthenticationFlow for DeviceFlow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        authenticate_with_token_cache(
            self.scopes.clone(),
            &self.token_cache_path,
            |scopes| obtain_new_token(&self.client, scopes),
            |rt, scopes| refresh_token(&self.client, rt, scopes),
        )
        .await
    }
}
#[derive(Debug, Serialize, Deserialize)]
struct StoringFields(HashMap<String, serde_json::Value>);

impl ExtraDeviceAuthorizationFields for StoringFields {}
type StoringDeviceAuthorizationResponse = DeviceAuthorizationResponse<StoringFields>;

async fn obtain_new_token(
    client: &BasicClient,
    scopes: Vec<Scope>,
) -> eyre::Result<StandardTokenResponse> {
    let details: StoringDeviceAuthorizationResponse = client
        .exchange_device_code()?
        .add_scopes(scopes)
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .map_err(map_request_token_error)
        .wrap_err("Error exchanging device code")?;

    tracing::info!(
        "Open this URL in your browser:\n{}\nand enter the code: {}",
        details.verification_uri().to_string(),
        details.user_code().secret().to_string()
    );

    client
        .exchange_device_access_token(&details)
        .request_async(oauth2::reqwest::async_http_client, tokio::time::sleep, None)
        .await
        .map_err(|error| match &error {
            oauth2::RequestTokenError::ServerResponse(server_response) => {
                let server_response_message = match serde_json::to_string_pretty(&server_response) {
                    Ok(server_response) => server_response,
                    Err(err) => format!("Error serializing server response: {:?}", err),
                };
                eyre::Error::from(error)
                    .with_section(|| format!("Server Response: {}", server_response_message))
            }
            _ => eyre::Error::from(error),
        })
        .wrap_err("Error exchanging authentication code")
}
