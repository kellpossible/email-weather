use std::{
    borrow::{Borrow, Cow},
    path::Path,
};

use color_eyre::Help;
use eyre::Context;
use oauth2::{
    basic::BasicClient, AccessToken, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSecretDefinition {
    Installed(InstalledClientSecretDefinition),
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
    pub redirect_uris: Vec<RedirectUrl>,
}

type StandardTokenResponse =
    oauth2::StandardTokenResponse<oauth2::EmptyExtraTokenFields, oauth2::basic::BasicTokenType>;

#[derive(Serialize, Deserialize)]
struct TokenCache {
    response: StandardTokenResponse,
    expires_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl TokenCache {
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

    async fn read(path: &Path) -> eyre::Result<Self> {
        let token_cache_string = tokio::fs::read_to_string(path).await?;
        let mut token_cache: Self = serde_json::from_str(&token_cache_string)?;

        // Update the expires_in field
        token_cache.response.set_expires_in(None);

        Ok(token_cache)
    }

    async fn write(&self, path: &Path) -> eyre::Result<()> {
        let overwritten = path.exists();
        let token_cache_json =
            serde_json::to_string_pretty(&self).wrap_err("Error serializing token cache")?;
        tokio::fs::write(path, &token_cache_json)
            .await
            .wrap_err_with(|| format!("Error writing token cache to {:?}", path))?;

        if overwritten {
            tracing::debug!("Overwritten token cache {:?}", path);
        } else {
            tracing::debug!("Wrote new token cache {:?}", path);
        }

        Ok(())
    }
}

async fn refresh_token(
    client: &BasicClient,
    refresh_token: &RefreshToken,
    scopes: Vec<Scope>,
) -> eyre::Result<StandardTokenResponse> {
    let mut response = client
        .exchange_refresh_token(refresh_token)
        .add_scopes(scopes)
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .wrap_err("Error while exchanging refresh token")?;

    // Re-use the refresh token if none is provided
    if response.refresh_token().is_none() {
        tracing::debug!("No new refresh token in the response, re-using current refresh token");
        response.set_refresh_token(Some(refresh_token.clone()))
    }

    Ok(response)
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

pub async fn authenticate(
    client_secret: ClientSecretDefinition,
    scopes: Vec<Scope>,
    token_cache_path: &Path,
) -> eyre::Result<AccessToken> {
    let client: BasicClient = match client_secret {
        ClientSecretDefinition::Installed(definition) => BasicClient::new(
            definition.client_id,
            Some(definition.client_secret),
            definition.auth_uri,
            Some(definition.token_uri),
        ),
    };

    let token_cache: TokenCache = if token_cache_path.exists() {
        tracing::debug!(
            "Token cache file {:?} exists, attempting to read from file",
            token_cache_path
        );
        let token_cache = TokenCache::read(token_cache_path).await.wrap_err_with(|| {
            format!("Error reading token cache from file {:?}", token_cache_path)
        })?;

        let token_expired: bool = token_cache
            .expires_time
            .map(|expires_time| expires_time < chrono::Utc::now())
            .unwrap_or(false);

        if token_expired {
            tracing::debug!("Token in cache has expired.");
            let token_response = if let Some(token) = token_cache.response.refresh_token() {
                tracing::debug!("Using refresh token to automatically obtain a new token");
                refresh_token(&client, token, scopes).await?
            } else {
                tracing::debug!("No refresh token available, manually obtaining a new token");
                obtain_new_token(&client, scopes).await?
            };
            let token_cache = TokenCache::try_new(token_response)?;
            token_cache.write(token_cache_path).await?;
            token_cache
        } else {
            token_cache
        }
    } else {
        tracing::debug!(
            "Token cache file {:?} does not exist, obtaining new token",
            token_cache_path
        );
        let token_response = obtain_new_token(&client, scopes).await?;
        tracing::debug!("Successfully obtained new token!");
        let token_cache = TokenCache::try_new(token_response)?;
        token_cache.write(token_cache_path).await?;
        token_cache
    };

    if let Some(expires_in) = token_cache.expires_in_now() {
        tracing::debug!(
            "Token expires in: {}",
            humantime::format_duration(expires_in.to_std()?)
        );
    } else {
        tracing::warn!("Token has no expiration time")
    }

    Ok(token_cache.response.access_token().clone())
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

        match definition {
            ClientSecretDefinition::Installed(definition) => {
                assert_eq!(
                    "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",
                    definition.client_secret.secret()
                );
            }
        }
    }
}
