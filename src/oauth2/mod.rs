//! Library for handling oauth2 authentication.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use eyre::Context;
use html_builder::Html5;
use oauth2::{
    basic::BasicClient, AccessToken, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    ErrorResponse, RedirectUrl, RefreshToken, RequestTokenError, Scope, TokenResponse, TokenUrl,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::sync::{mpsc, Mutex, MutexGuard};

mod device;
mod installed;
pub mod service_account;

pub use service_account::ServiceAccountFlow;

use crate::secrets::OauthSecrets;

/// Method used to redirect the user to obtain their consent for authentication.
pub enum ConsentRedirect {
    /// Out of band redirect, exchange code using user's clipboard.
    /// **Warning**: Google has deprecated this method.
    OutOfBand,
    /// With a http redirect/request.
    Http {
        /// Channel to recieve redirect result from http server.
        redirect_rx: Arc<Mutex<mpsc::Receiver<RedirectParameters>>>,
        /// Url to use for sending the redirect.
        url: RedirectUrl,
    },
}

impl ConsentRedirect {
    /// Obtain the redirect URL
    pub fn redirect_url(&self) -> RedirectUrl {
        match self {
            ConsentRedirect::OutOfBand => RedirectUrl::new("urn:ietf:wg:oauth:2.0:oob".to_string())
                .expect("Expected oob url to be formatted correctly"),
            ConsentRedirect::Http { url, .. } => url.clone(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSecretDefinition {
    Installed(InstalledClientSecretDefinition),
    Web(InstalledClientSecretDefinition),
}

impl ClientSecretDefinition {
    pub fn client_id(&self) -> &ClientId {
        match self {
            ClientSecretDefinition::Installed(s) => &s.client_id,
            ClientSecretDefinition::Web(s) => &s.client_id,
        }
    }
    pub fn client_secret(&self) -> &ClientSecret {
        match self {
            ClientSecretDefinition::Installed(s) => &s.client_secret,
            ClientSecretDefinition::Web(s) => &s.client_secret,
        }
    }

    pub fn auth_url(&self) -> &AuthUrl {
        match self {
            ClientSecretDefinition::Installed(s) => &s.auth_uri,
            ClientSecretDefinition::Web(s) => &s.auth_uri,
        }
    }

    pub fn token_url(&self) -> &TokenUrl {
        match self {
            ClientSecretDefinition::Installed(s) => &s.token_uri,
            ClientSecretDefinition::Web(s) => &s.token_uri,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct InstalledClientSecretDefinition {
    /// The client ID.
    pub client_id: ClientId,
    /// The client secret.
    pub client_secret: ClientSecret,
    /// Name of the google project the credentials are associated with.
    pub project_id: Option<String>,
    /// The authorization server endpoint URI.
    pub auth_uri: AuthUrl,
    /// The token server endpoint URI.
    pub token_uri: TokenUrl,
    /// The URL of the public x509 certificate, used to verify the signature on JWTs, such
    /// as ID tokens, signed by the authentication provider.
    pub auth_provider_x509_cert_url: Option<url::Url>,
    /// The redirect uris.
    #[serde(default)]
    pub redirect_uris: Vec<RedirectUrl>,
}

type StandardTokenResponse =
    oauth2::StandardTokenResponse<oauth2::EmptyExtraTokenFields, oauth2::basic::BasicTokenType>;

struct TokenCache {
    /// Path to token cache file.
    path: PathBuf,
    lock: Mutex<()>,
}

impl TokenCache {
    fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    async fn lock<'a>(&'a self) -> TokenCacheGuard<'a> {
        TokenCacheGuard {
            path: &self.path,
            _guard: self.lock.lock().await,
        }
    }
}

impl std::fmt::Debug for TokenCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenCache")
            .field("path", &self.path)
            .finish()
    }
}

/// Organises simultaneous access to the token cache, to prevent data races.
/// Obtain this guard using [`TokenCache::lock()`].
struct TokenCacheGuard<'a> {
    path: &'a Path,
    _guard: MutexGuard<'a, ()>,
}

impl std::fmt::Debug for TokenCacheGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenCacheGuard")
            .field("path", &self.path)
            .finish()
    }
}

impl TokenCacheGuard<'_> {
    fn exists(&self) -> bool {
        self.path.exists()
    }

    async fn read(&self) -> eyre::Result<TokenCacheData> {
        let token_cache_string = tokio::fs::read_to_string(self.path).await?;
        let mut token_cache: TokenCacheData = serde_json::from_str(&token_cache_string)?;

        // Update the expires_in field
        token_cache.response.set_expires_in(None);

        Ok(token_cache)
    }

    async fn write(&mut self, data: &TokenCacheData) -> eyre::Result<()> {
        let overwritten = self.path.exists();
        let token_cache_json =
            serde_json::to_string_pretty(data).wrap_err("Error serializing token cache")?;
        tokio::fs::write(self.path, &token_cache_json)
            .await
            .wrap_err_with(|| format!("Error writing token cache to {:?}", self.path))?;

        if overwritten {
            tracing::debug!("Overwritten token cache {:?}", self.path);
        } else {
            tracing::debug!("Wrote new token cache {:?}", self.path);
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct TokenCacheData {
    response: StandardTokenResponse,
    expires_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl TokenCacheData {
    fn try_new(response: StandardTokenResponse) -> eyre::Result<Self> {
        let expires_time = Option::<eyre::Result<_>>::transpose(
            response
                .expires_in()
                .map(|duration| Ok(chrono::Utc::now() + chrono::Duration::from_std(duration)?)),
        )?;
        Ok(Self {
            response,
            expires_time,
        })
    }

    fn expires_in_now(&self) -> Option<chrono::Duration> {
        let now = chrono::Utc::now();
        self.expires_time.as_ref().map(|expires_time| {
            if now >= *expires_time {
                chrono::Duration::zero()
            } else {
                *expires_time - now
            }
        })
    }
}

fn map_request_token_error<RE, T>(error: RequestTokenError<RE, T>) -> eyre::Error
where
    RE: std::error::Error + Send + Sync,
    T: ErrorResponse + Send + Sync,
{
    match error {
        oauth2::RequestTokenError::ServerResponse(response) => {
            let response_json = match serde_json::to_string_pretty(&response) {
                Ok(response_json) => response_json,
                Err(error) => format!(
                    "Unable to display response, error while serializing response to json ({})",
                    error
                ),
            };
            eyre::eyre!("Server returned error response:\n{}", response_json)
        }
        _ => eyre::Error::from(error),
    }
}

async fn refresh_token(
    client: &BasicClient,
    refresh_token: RefreshToken,
    scopes: &[Scope],
) -> eyre::Result<StandardTokenResponse> {
    let mut response = client
        .exchange_refresh_token(&refresh_token)
        .add_scopes(scopes.iter().cloned())
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .map_err(map_request_token_error)
        .wrap_err("Error while exchanging refresh token")?;

    // Re-use the refresh token if none is provided
    if response.refresh_token().is_none() {
        tracing::debug!("No new refresh token in the response, re-using current refresh token");
        response.set_refresh_token(Some(refresh_token.clone()))
    }

    Ok(response)
}

/// A flow for performing authentication using OAUTH2.
#[async_trait]
pub trait AuthenticationFlow {
    /// Authenticate using OAUTH2 provider.
    async fn authenticate(&self) -> eyre::Result<AccessToken>;
}

async fn authenticate_with_token_cache<'a, Fut1, Fut2>(
    scopes: &'a [Scope],
    token_cache: &mut TokenCacheGuard<'_>,
    obtain_new_token: impl FnOnce(&'a [Scope]) -> Fut1,
    refresh_token: impl FnOnce(RefreshToken, &'a [Scope]) -> Fut2,
) -> eyre::Result<AccessToken>
where
    Fut1: Future<Output = eyre::Result<StandardTokenResponse>> + 'a,
    Fut2: Future<Output = eyre::Result<StandardTokenResponse>> + 'a,
{
    let token_cache_data: TokenCacheData = if token_cache.exists() {
        tracing::debug!(
            "Token cache {:?} exists, attempting to read from file",
            token_cache
        );
        let token_cache_data = token_cache
            .read()
            .await
            .wrap_err_with(|| format!("Error reading token cache {:?}", token_cache))?;

        let token_expired: bool = token_cache_data
            .expires_time
            .map(|expires_time| expires_time < chrono::Utc::now())
            .unwrap_or(false);

        if token_expired {
            tracing::debug!("Token in cache has expired.");
            let token_response = if let Some(token) = token_cache_data.response.refresh_token() {
                tracing::debug!("Using refresh token to automatically obtain a new token");
                refresh_token(token.clone(), &scopes)
                    .await
                    .wrap_err("Error while refreshing token")?
            } else {
                tracing::debug!("No refresh token available, manually obtaining a new token");
                obtain_new_token(&scopes)
                    .await
                    .wrap_err("Error while obtaining new token")?
            };
            let token_cache_data = TokenCacheData::try_new(token_response)?;
            token_cache.write(&token_cache_data).await?;
            token_cache_data
        } else {
            token_cache_data
        }
    } else {
        tracing::debug!(
            "Token cache {:?} does not exist, obtaining new token",
            token_cache
        );
        let token_response = obtain_new_token(&scopes).await?;
        tracing::debug!("Successfully obtained new token!");
        let token_cache_data = TokenCacheData::try_new(token_response)?;
        token_cache.write(&token_cache_data).await?;
        token_cache_data
    };

    if let Some(expires_in) = token_cache_data.expires_in_now() {
        let refresh_message = if token_cache_data.response.refresh_token().is_some() {
            "It can be refreshed using the cached refresh token."
        } else {
            "It cannot be refreshed, the cache does not contain a refresh token, a new token will be need obtained upon expire."
        };
        tracing::debug!(
            "Token expires in: {}. {}",
            humantime::format_duration(expires_in.to_std()?),
            refresh_message,
        );
    } else {
        tracing::warn!("Token has no expiration time")
    }

    Ok(token_cache_data.response.access_token().clone())
}

#[derive(Debug, Deserialize)]
pub struct RedirectParameters {
    pub code: AuthorizationCode,
    pub state: CsrfToken,
}

#[derive(Debug, thiserror::Error)]
enum RedirectError {
    #[error("Internal server error")]
    InternalServerError(#[from] eyre::Error),
}

impl From<std::fmt::Error> for RedirectError {
    fn from(error: std::fmt::Error) -> Self {
        Self::InternalServerError(eyre::Error::from(error))
    }
}

impl IntoResponse for RedirectError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RedirectError::InternalServerError(error) => {
                tracing::error!("Error receiving redirect: {:?}", error);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

async fn get_redirect(
    axum::extract::Query(parameters): axum::extract::Query<RedirectParameters>,
    tx: mpsc::Sender<RedirectParameters>,
) -> axum::response::Result<Html<String>, RedirectError> {
    use std::fmt::Write;
    tx.send_timeout(parameters, Duration::from_millis(50))
        .await
        .wrap_err("Error sending redirect authorization code via channel")?;
    let mut buf = html_builder::Buffer::new();
    let mut html = buf.html();
    let mut head = html.head();
    let mut title = head.title();
    write!(title, "email-weather Authentication Successful")?;
    let mut body = html.body();
    write!(body, "Authentication with the email-weather service was successful, you may close this browser tab.")?;

    Ok(Html(buf.finish()))
}

/// Http server for accepting OAUTH2 authentication redirects.
pub fn redirect_server(tx: mpsc::Sender<RedirectParameters>) -> Router {
    Router::new().route(
        "/",
        get(|path| async move { get_redirect(path, tx.clone()).await }),
    )
}

/// Set up the authentication flow.
pub fn setup_flow(
    secrets: &OauthSecrets,
    base_url: &url::Url,
    oauth_redirect_rx: mpsc::Receiver<RedirectParameters>,
) -> eyre::Result<installed::Flow> {
    let scopes = vec![
        // https://developers.google.com/gmail/imap/xoauth2-protocol
        oauth2::Scope::new("https://mail.google.com/".to_string()),
    ];

    let redirect_url = RedirectUrl::from_url(base_url.join("oauth2")?);
    Ok(crate::oauth2::installed::Flow::new(
        ConsentRedirect::Http {
            redirect_rx: Arc::new(Mutex::new(oauth_redirect_rx)),
            url: redirect_url,
        },
        &secrets.client_secret.clone().ok_or_else(|| {
            eyre::eyre!(
                "Client secret has not been provided, and is required for Installed OAUTH2 flow"
            )
        })?,
        scopes,
        secrets.token_cache_path.clone(),
    ))
}

#[cfg(test)]
mod test {
    use super::ClientSecretDefinition;

    #[test]
    fn test_deserialize_installed_client_secret() {
        let client_secret_definition = r#"
{
  "installed": {
    "client_id": "1045440812292-5e6tro8vcpdl67cd8q9s9v59kvrt27u7.apps.googleusercontent.com",
    "project_id": "email-weather",
    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
    "token_uri": "https://oauth2.googleapis.com/token",
    "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
    "client_secret": "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",
    "redirect_uris": ["http://localhost"]
  }
}
        "#;

        let definition: ClientSecretDefinition =
            serde_json::from_str(client_secret_definition).unwrap();

        assert_eq!(
            "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",
            definition.client_secret().secret()
        );
    }
}
