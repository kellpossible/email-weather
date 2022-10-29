//! OAUTH2 authentication with a Google service account.

use std::path::PathBuf;

use super::{authenticate_with_token_cache, AuthenticationFlow, StandardTokenResponse};
use async_trait::async_trait;
use chrono::serde::ts_seconds::serialize as to_ts;
use color_eyre::Help;
use eyre::Context;
use jsonwebtoken::EncodingKey;
use oauth2::{AccessToken, AuthUrl, ClientId, Scope, TokenUrl};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KeyKind {
    ServiceAccount,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
struct ClientEmail(String);

/// Service account private key json file.
#[allow(unused)]
#[derive(Clone, Deserialize)]
pub struct Key {
    #[serde(rename = "type")]
    kind: KeyKind,
    /// Name of the google project the credentials are associated with.
    project_id: Option<String>,
    private_key_id: String,
    private_key: SecretString,
    /// Email address of the service account.
    client_email: ClientEmail,
    client_id: ClientId,
    /// The authorization server endpoint URI.
    auth_uri: AuthUrl,
    /// The token server endpoint URI.
    token_uri: TokenUrl,
    /// The URL of the public x509 certificate, used to verify the signature on JWTs, such
    /// as ID tokens, signed by the authentication provider.
    auth_provider_x509_cert_url: url::Url,
    client_x509_cert_url: url::Url,
}

impl Key {
    fn encoding_key(&self) -> jsonwebtoken::errors::Result<EncodingKey> {
        EncodingKey::from_rsa_pem(self.private_key.expose_secret().as_bytes())
    }
}

#[derive(Serialize)]
struct Claims {
    /// Email address of the service account.
    iss: ClientEmail,
    /// A space-delimited list of the permissions that the application requests. **Note**:
    /// currently we only support a single scope.
    scope: Scope,
    /// A descriptor of the intended target of the assertion.
    aud: TokenUrl,
    /// The expiration time of the assertion, specified as seconds since 00:00:00 UTC, January 1,
    /// 1970. This value has a maximum of 1 hour after the issued time.
    #[serde(serialize_with = "to_ts")]
    exp: chrono::DateTime<chrono::Utc>,
    /// The time the assertion was issued, specified as seconds since 00:00:00 UTC, January 1,
    /// 1970.
    #[serde(serialize_with = "to_ts")]
    iat: chrono::DateTime<chrono::Utc>,
}

impl Claims {
    fn create_now(client_email: ClientEmail, scope: Scope, token_url: TokenUrl) -> Self {
        Self {
            iss: client_email,
            scope,
            aud: token_url,
            exp: chrono::Utc::now() + chrono::Duration::minutes(30),
            iat: chrono::Utc::now(),
        }
    }
}

fn encode_jwt(key: &Key, scopes: Vec<Scope>) -> eyre::Result<String> {
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let claims = Claims::create_now(
        key.client_email.clone(),
        scopes
            .get(0)
            .ok_or_else(|| eyre::eyre!("No scopes provided, expected one scope"))?
            .clone(),
        key.token_uri.clone(),
    );

    let encoding_key = key.encoding_key().wrap_err("Error parsing encoding key")?;
    jsonwebtoken::encode(&header, &claims, &encoding_key).map_err(eyre::Error::from)
}

async fn obtain_new_token(key: &Key, scopes: Vec<Scope>) -> eyre::Result<StandardTokenResponse> {
    let assertion = encode_jwt(key, scopes)?;
    let client = reqwest::Client::new();

    let mut body = String::new();
    let grant_type = urlencoding::encode("urn:ietf:params:oauth:grant-type:jwt-bearer");
    body.push_str("grant_type=");
    body.push_str(&*grant_type);
    body.push_str("&assertion=");
    body.push_str(&assertion);
    let response = client
        .post(&*key.token_uri)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;

    let status = response.status();
    if status.is_success() {
        let token: StandardTokenResponse = response
            .json()
            .await
            .wrap_err("Error parsing token response")?;
        Ok(token)
    } else {
        let body_json: Option<serde_json::Value> = response.json().await.ok();
        let body_json_string: Option<String> =
            body_json.and_then(|b| serde_json::to_string_pretty(&b).ok());
        let error = eyre::eyre!("POST request response status is an error {}.", status);

        Err(if let Some(body_json_string) = body_json_string {
            error.with_note(|| body_json_string)
        } else {
            error
        })
    }
}


/// A flow for authenticating with a Google service account.
pub struct ServiceAccountFlow {
    key: Key,
    scopes: Vec<Scope>,
    token_cache_path: PathBuf,
}

impl ServiceAccountFlow {
    /// Create a new [`ServiceAccountFlow`].
    pub fn new(key: Key, scopes: Vec<Scope>, token_cache_path: PathBuf) -> Self {
        Self {
            key,
            scopes,
            token_cache_path,
        }
    }
}

#[async_trait]
impl AuthenticationFlow for ServiceAccountFlow {
    async fn authenticate(&self) -> eyre::Result<AccessToken> {
        authenticate_with_token_cache(
            self.scopes.clone(),
            &self.token_cache_path,
            |scopes| obtain_new_token(&self.key, scopes),
            // Refresh involves just obtaining another token (no refresh token involved).
            |_, scopes| obtain_new_token(&self.key, scopes),
        )
        .await
    }
}

#[cfg(test)]
mod test {
    use super::{encode_jwt, Key};

    #[test]
    fn test_encode_token() {
        // This is an expired secret, don't try to use it for real.
        let key_str: &str = r#"{
  "type": "service_account",
  "project_id": "email-weather",
  "private_key_id": "0a27c33354a35e6ffc5363f5cda9126f7c4e559f",
  "private_key": "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCKhodfLl4VoYqS\nLY0JjxG6KeeTcRlMsZev9a/OhG/mrISkossh+qGBXz6w2EFSvpPPqMY8xlCEF0Xu\nxQS2++hN+vnhfi/3kqlwnqRmasj2kmKQ8hcuFCuxq7++tGWADfZkig4qB5G/Ytzg\nIrDVwpfbj63Lla92LgES/lmGzxKU+rNW5TBN2iV8vEz+Hh7EprmfFxu8gfkoEVhb\nmu/yQNOP5Sne58CdjrV3AygAZlLrfnDW5rxlvJlaryGsymOUA0C0uHx9YcjGwiP5\nvkV9smtQ92nEDqq+PlL0Us+BxNIDj7K3ud/uG4/+4cVA5chn80KZrs++Ot1+PfhO\nfHfPpQpRAgMBAAECggEADfnMppmy/FOz+1OFKzW4ACRCLOn4N3ijaSlMd3V9JLS7\nHTEfdWon6TmGxajLzmFT4FuSxIbtkKYYdCKEe0GnClcL5ugoRr4RQj9/LqYPaHEU\naLNEC24VinNdgQwKQYUnGrWjADKLTdfXmPVnCen8EDbKvgN4FGBH03a96Y/yu/z5\nHzw70LRZ1pHhUfs6f9n5od0SjNk4yw2xIovymBzwBk9Qgnw7puec4p8i4Gd87d7Y\nHwIhG57wb0IDgrlB52lNzjekmY3QvcPXo5Kp9mlPFZpusdnzkcM8uolzPVz9QTtt\npH5vvGhvw1Lk20EvB4HEb179uQLxoTRjcnwuFpQ20QKBgQDB5R7/r+CtJLTolYaP\nb5FoVwQ9Q24gQyHWeoGelGH66txXyt428tMjyKyrQGvczasSb3e8VsoEQk7qwrMp\nT22ESAxZWCq5T7Jz/8AvTHXHfy2Xr29YGqIr+v+LxLKCzHH0zpJ0ZE+ZuQ5Oz6sn\n5PZcrXZ0mrETOvZy/bbO/dt7FwKBgQC25UExgo5bRSVJT8ql+p7wfnuzMJtzIlKd\n5smEAb2y4rLCNjwY9Hqvy0w1N8GSj1ql4dk8i3YQmKsQemZXgy7a89FaE+5WaWj+\nNV+YZXTMv0zUPN0Pl2vkKAL5Ix83lsyiMSpajBzzzdqUZ2SpxvHnlQzzOJMiiG2X\n8Yqhvlbm1wKBgQCktLQTcNzDV9YRaMsoVxbG8nwYaopG/5/j6KbpBZUBp7ZLIXqI\nZNd0o0gCJTQ7Gb6DZ4rnwzXSTl1pUMEOi3k1kFplHt8UEZ4+qXcg9qtqLx+UpaNI\nzT8LayjfGtSlBXScB0ojcv6nT6rWydPTjMy2R2fDf5CCDGlDn0BGLyDdOwKBgAyl\nDvvQTe1Le4d1B8qv6Bsyc3TxEF5GajXWheolgKsEd11sCH2lMXJD+PHY9/4dASRk\n1/MSpUgCdhk+jSLRxASJRNkYdartwL+KiyBrK0cYlsQ5rQLt8hylE4eMARWDzIQO\nKCJ4e2vzuH/4IgKG6aScLngGWk3R5tnRbkc+dJ2jAoGBAJ5R7jcWjCwopzpCpzIw\nLutDR2dNHeWtAb/UDSkbxwz2xG5WeTZ9iS5/1gN+M1RBOc32k18fob7i2daokV7r\nCeQw7a4Vt8UfQDxz+6pU0kQbIb6RRD9hCtW6nAOmjISrB8Hyt//QM9rPJd1vBKmi\ngWfwGLgRKQ0+V5y6EEtGopTe\n-----END PRIVATE KEY-----\n",
  "client_email": "forecast@email-weather.iam.gserviceaccount.com",
  "client_id": "109549041441737817187",
  "auth_uri": "https://accounts.google.com/o/oauth2/auth",
  "token_uri": "https://oauth2.googleapis.com/token",
  "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
  "client_x509_cert_url": "https://www.googleapis.com/robot/v1/metadata/x509/forecast%40email-weather.iam.gserviceaccount.com"
}"#;
        let key: Key = serde_json::from_str(key_str).unwrap();
        let jwt = encode_jwt(
            &key,
            vec![oauth2::Scope::new("https://mail.google.com/".to_string())],
        )
        .unwrap();
        assert_eq!(jwt.len(), 606);
    }
}
