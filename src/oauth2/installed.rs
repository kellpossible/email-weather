use std::{borrow::Cow, path::PathBuf};

use async_trait::async_trait;
use color_eyre::Help;
use eyre::Context;
use oauth2::{
    basic::BasicClient, AccessToken, AuthorizationCode, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope,
};

use super::{
    authenticate_with_token_cache, AuthenticationFlow, ClientSecretDefinition,
    StandardTokenResponse,
};

/// Used for the "installed" authentication flow.
pub struct InstalledFlow {
    client_secret: ClientSecretDefinition,
    scopes: Vec<Scope>,
    token_cache_path: PathBuf,
}

impl InstalledFlow {
    /// Create a new [`InstalledFlow`].
    pub fn new(
        client_secret: ClientSecretDefinition,
        scopes: Vec<Scope>,
        token_cache_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            client_secret,
            scopes,
            token_cache_path: token_cache_path.into(),
        }
    }
}

#[async_trait]
impl AuthenticationFlow for InstalledFlow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        let client: BasicClient = match &self.client_secret {
            ClientSecretDefinition::Installed(definition) => BasicClient::new(
                definition.client_id.clone(),
                Some(definition.client_secret.clone()),
                definition.auth_uri.clone(),
                Some(definition.token_uri.clone()),
            ),
        };

        authenticate_with_token_cache(
            &client,
            self.scopes.clone(),
            &self.token_cache_path,
            obtain_new_token,
        )
        .await
    }
}

async fn obtain_new_token(
    client: &BasicClient,
    scopes: Vec<Scope>,
) -> eyre::Result<StandardTokenResponse> {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let redirect_uri = RedirectUrl::new("urn:ietf:wg:oauth:2.0:oob".to_string())?;
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(scopes)
        .set_pkce_challenge(pkce_challenge)
        // Out of band copy/paste code
        .set_redirect_uri(Cow::Borrowed(&redirect_uri))
        .url();

    tracing::info!(
        "Open this URL to obtain the OAUTH2 authentication code for your email account:\n{}",
        auth_url
    );

    let code: AuthorizationCode =
        AuthorizationCode::new(rpassword::prompt_password("Enter the code:")?);
    tracing::debug!("code.len() = {}", code.secret().len());

    client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .set_redirect_uri(Cow::Borrowed(&redirect_uri))
        .request_async(oauth2::reqwest::async_http_client)
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
