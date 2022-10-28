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
    authenticate_with_token_cache, AuthenticationFlow, ClientSecretDefinition,
    StandardTokenResponse,
};

pub struct DeviceFlow {
    client_secret: ClientSecretDefinition,
    scopes: Vec<Scope>,
    token_cache_path: PathBuf,
    device_authorization_url: DeviceAuthorizationUrl,
}

impl DeviceFlow {
    /// Create a new [`DeviceFlow`].
    pub fn new(
        client_secret: ClientSecretDefinition,
        scopes: Vec<Scope>,
        token_cache_path: impl Into<PathBuf>,
        device_authorization_url: DeviceAuthorizationUrl,
    ) -> Self {
        Self {
            client_secret,
            scopes,
            token_cache_path: token_cache_path.into(),
            device_authorization_url,
        }
    }
}

#[async_trait]
impl AuthenticationFlow for DeviceFlow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        let client: BasicClient = match &self.client_secret {
            ClientSecretDefinition::Installed(definition) => BasicClient::new(
                definition.client_id.clone(),
                Some(definition.client_secret.clone()),
                definition.auth_uri.clone(),
                Some(definition.token_uri.clone()),
            ),
        }
        .set_device_authorization_url(self.device_authorization_url.clone())
        .set_auth_type(oauth2::AuthType::RequestBody);

        authenticate_with_token_cache(
            &client,
            self.scopes.clone(),
            &self.token_cache_path,
            obtain_new_token,
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
