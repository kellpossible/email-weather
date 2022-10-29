use std::{borrow::Cow, path::PathBuf};

use async_trait::async_trait;
use color_eyre::Help;
use eyre::Context;
use oauth2::{
    basic::BasicClient, AccessToken, AuthorizationCode, CsrfToken, PkceCodeChallenge, Scope,
};

use super::{
    authenticate_with_token_cache, refresh_token, AuthenticationFlow, ClientSecretDefinition,
    ConsetRedirect, StandardTokenResponse,
};

/// Used for the "installed" authentication flow.
pub struct InstalledFlow {
    redirect: ConsetRedirect,
    scopes: Vec<Scope>,
    token_cache_path: PathBuf,
    client: BasicClient,
}

impl InstalledFlow {
    /// Create a new [`InstalledFlow`].
    pub fn new(
        redirect: ConsetRedirect,
        client_secret: ClientSecretDefinition,
        scopes: Vec<Scope>,
        token_cache_path: PathBuf,
    ) -> Self {
        let client = BasicClient::new(
            client_secret.client_id().clone(),
            Some(client_secret.client_secret().clone()),
            client_secret.auth_url().clone(),
            Some(client_secret.token_url().clone()),
        );

        Self {
            redirect,
            client,
            scopes,
            token_cache_path,
        }
    }
    async fn obtain_new_token(&self, scopes: Vec<Scope>) -> eyre::Result<StandardTokenResponse> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let redirect_uri = self.redirect.redirect_url();
        let (auth_url, _csrf_token) = self
            .client
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

        let code: AuthorizationCode = match self.redirect {
            ConsetRedirect::OutOfBand => {
                AuthorizationCode::new(rpassword::prompt_password("Enter the code:")?)
            }
        };

        self.client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .set_redirect_uri(Cow::Borrowed(&redirect_uri))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|error| match &error {
                oauth2::RequestTokenError::ServerResponse(server_response) => {
                    let server_response_message =
                        match serde_json::to_string_pretty(&server_response) {
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
}

#[async_trait]
impl AuthenticationFlow for InstalledFlow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        authenticate_with_token_cache(
            self.scopes.clone(),
            &self.token_cache_path,
            |scopes| self.obtain_new_token(scopes),
            |rt, scopes| refresh_token(&self.client, rt, scopes),
        )
        .await
    }
}
