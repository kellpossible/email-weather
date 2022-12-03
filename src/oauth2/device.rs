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
    StandardTokenResponse, TokenCache,
};

/// Device OAUTH2 flow.
pub struct Flow {
    client: BasicClient,
    scopes: Vec<Scope>,
    token_cache: TokenCache,
}

#[allow(unused)]
impl Flow {
    /// Create a new [`DeviceFlow`].
    pub fn new(
        client_secret: &ClientSecretDefinition,
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

        let token_cache = TokenCache::new(token_cache_path);

        Self {
            client,
            scopes,
            token_cache,
        }
    }
}

#[async_trait]
impl AuthenticationFlow for Flow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        let mut token_cache = self.token_cache.lock().await;
        authenticate_with_token_cache(
            &self.scopes,
            &mut token_cache,
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
    scopes: &[Scope],
) -> eyre::Result<StandardTokenResponse> {
    let details: StoringDeviceAuthorizationResponse = client
        .exchange_device_code()?
        .add_scopes(scopes.iter().cloned())
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .map_err(map_request_token_error)
        .wrap_err("Error exchanging device code")?;

    let uri_string: &String = details.verification_uri();
    tracing::info!(
        "Open this URL in your browser:\n{}\nand enter the code: {}",
        uri_string,
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
