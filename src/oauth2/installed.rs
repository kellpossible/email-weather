use std::{borrow::Cow, path::PathBuf};

use async_trait::async_trait;
use color_eyre::Help;
use eyre::Context;
use oauth2::{
    basic::BasicClient, AccessToken, AuthorizationCode, CsrfToken, PkceCodeChallenge, Scope,
    TokenResponse,
};

use super::{
    authenticate_with_token_cache, refresh_token, AuthenticationFlow, ClientSecretDefinition,
    ConsentRedirect, StandardTokenResponse, TokenCache,
};

/// Used for the "installed" authentication flow.
pub struct Flow {
    redirect: ConsentRedirect,
    scopes: Vec<Scope>,
    client: BasicClient,
    token_cache: TokenCache,
}

impl Flow {
    /// Create a new [`InstalledFlow`].
    pub fn new(
        redirect: ConsentRedirect,
        client_secret: &ClientSecretDefinition,
        scopes: Vec<Scope>,
        token_cache_path: impl Into<PathBuf>,
    ) -> Self {
        let client = BasicClient::new(
            client_secret.client_id().clone(),
            Some(client_secret.client_secret().clone()),
            client_secret.auth_url().clone(),
            Some(client_secret.token_url().clone()),
        );

        let token_cache = TokenCache::new(token_cache_path);

        Self {
            redirect,
            scopes,
            client,
            token_cache,
        }
    }

    #[tracing::instrument(skip(self, scopes))]
    async fn obtain_new_token(&self, scopes: &[Scope]) -> eyre::Result<StandardTokenResponse> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let redirect_uri = self.redirect.redirect_url();
        let (auth_url, csrf_state) = self
            .client
            .authorize_url(CsrfToken::new_random)
            // access_type Indicates whether your application can refresh access tokens when the user is not
            // present at the browser. Valid parameter values are online, which is the default
            // value, and offline.
            //
            // Set the value to offline if your application needs to refresh access tokens when the
            // user is not present at the browser. This is the method of refreshing access tokens
            // described later in this document. This value instructs the Google authorization
            // server to return a refresh token and an access token the first time that your
            // application exchanges an authorization code for tokens.
            .add_extra_param("access_type", "offline")
            .add_scopes(scopes.iter().cloned())
            .set_pkce_challenge(pkce_challenge)
            // Out of band copy/paste code
            .set_redirect_uri(Cow::Borrowed(&redirect_uri))
            .url();

        let code: AuthorizationCode = match &self.redirect {
            ConsentRedirect::OutOfBand => {
                tracing::info!(
                    "Open this URL to obtain the OAUTH2 authentication code for your email account:\n{}",
                    auth_url
                );
                AuthorizationCode::new(rpassword::prompt_password("Enter the code:")?)
            }
            ConsentRedirect::Http { redirect_rx, .. } => {
                tracing::info!(
                    "Open this URL to obtain the OAUTH2 authentication approval for your email account:\n{}",
                    auth_url
                );

                let mut rx = redirect_rx.lock().await;
                let parameters = rx.recv()
                    .await
                    .ok_or_else(|| eyre::eyre!("Redirect receiving channel has been closed, there is no code to be received"))?;

                if parameters.state.secret() != csrf_state.secret() {
                    return Err(eyre::eyre!("CSRF states don't match"));
                }
                parameters.code
            }
        };

        let token_response = self
            .client
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
            .wrap_err("Error exchanging authentication code")?;

        if token_response.refresh_token().is_none() {
            let expire_message: String = if let Some(expires_in) = token_response.expires_in() {
                format!(
                    "Current token will expire after {}",
                    humantime::format_duration(expires_in)
                )
            } else {
                "Current token will never expire".to_string()
            };
            tracing::warn!(
                "No refresh token provided with token response. {}.",
                expire_message
            );
        }

        Ok(token_response)
    }
}

#[async_trait]
impl AuthenticationFlow for Flow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        let mut token_cache = self.token_cache.lock().await;
        if token_cache.exists() {
            let data = token_cache
                .read()
                .await
                .wrap_err_with(|| format!("Error reading token cache {:?}", token_cache))?;
            if data.response.refresh_token().is_none() {
                if let Some(expires_in) = data.expires_in_now() {
                    tracing::warn!(
                        "No refresh token available, current token expires after: {}",
                        expires_in
                    );
                }
            }
        }
        authenticate_with_token_cache(
            &self.scopes,
            &mut token_cache,
            |scopes| self.obtain_new_token(scopes),
            |rt, scopes| refresh_token(&self.client, rt, scopes),
        )
        .await
    }
}
