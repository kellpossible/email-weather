use std::{
    env::VarError,
    path::{Path, PathBuf},
};

use eyre::Context;
use secrecy::{ExposeSecret, SecretString};

use crate::oauth2::ClientSecretDefinition;

pub struct ImapSecrets {
    pub token_cache_path: PathBuf,
    pub client_secret: ClientSecretDefinition,
}

impl ImapSecrets {
    /// Initializes secrets required for accessing IMAP.
    ///
    /// + If `CLIENT_SECRET` environment variable is set, the contents will be parsed, otherwise it
    ///   will be read from `clientsecret.json` in the specified `secrets_dir` directory.
    /// + If `TOKEN_CACHE` environment variable is set, the contents will be written to
    ///   `tokencache.json` inside the specified `secrets_dir` directory. If the file already
    ///   exists then the existing file will be used instead. If the environment variable is not
    ///   set, then the cache will be initialized automatically using the interactive Installed
    ///   OAUTH2 flow.
    /// + If `OVERWRITE_TOKEN_CACHE` environment variable is set, and `TOKEN_CACHE` is also set,
    ///   then the
    /// + `secrets_dir` needs to exist and have read/write permissions for this application.
    pub async fn initialize(secrets_dir: &Path) -> eyre::Result<Self> {
        let client_secret = match std::env::var("CLIENT_SECRET") {
            Ok(client_secret) => {
                tracing::debug!("Reading client secret from CLIENT_SECRET environment variable.");
                serde_json::from_str(&client_secret).wrap_err(
                    "Unable to parse client secret from CLIENT_SECRET environment variable",
                )
            }
            Err(std::env::VarError::NotPresent) => {
                let secret_path = secrets_dir.join("clientsecret.json");
                tracing::debug!("Reading client secret from file {:?}", &secret_path);

                {
                    let client_secret = tokio::fs::read_to_string(&secret_path).await?;
                    serde_json::from_str(&client_secret).wrap_err("Unable to parse client secret")
                }
                .wrap_err_with(|| {
                    format!("Error reading oauth2 secret from file {:?}", secret_path)
                })
            }
            Err(unexpected) => Err(eyre::Error::from(unexpected))
                .wrap_err("Error attempting to read CLIENT_SECRET environment variable"),
        }
        .wrap_err("Error reading oauth2 client secret")?;

        let token_cache_path = secrets_dir.join("tokencache.json");
        match std::env::var("TOKEN_CACHE") {
            Ok(secret) => {
                tracing::debug!("Reading token cache from TOKEN_CACHE environment variable.");
                let write: bool = if token_cache_path.exists() {
                    if let Ok(var) = std::env::var("OVERWRITE_TOKEN_CACHE") {
                        var == "true"
                    } else {
                        tracing::debug!(
                            "Token cache file {:?} already exists, will not overwrite",
                            token_cache_path
                        );
                        false
                    }
                } else {
                    true
                };

                if write {
                    if token_cache_path.exists() {
                        tracing::warn!("Overwriting token cache file {:?}", token_cache_path);
                    } else {
                        tracing::info!("Writing to new token cache file {:?}", token_cache_path);
                    }
                    std::fs::write(&token_cache_path, &secret).wrap_err_with(|| {
                        format!("Error writing token cache file: {:?}", token_cache_path)
                    })?;
                }
            }
            Err(std::env::VarError::NotPresent) => {
                if token_cache_path.exists() {
                    tracing::debug!(
                        "Pre-existing token cache file {:?} will be used",
                        token_cache_path
                    );
                } else {
                    tracing::debug!("Token cache {:?} will be automatically generated with Installed OAUTH2 flow", token_cache_path);
                }
            }
            Err(unexpected) => {
                return Err(unexpected)
                    .wrap_err("Error while reading TOKEN_CACHE environment variable");
            }
        }

        Ok(Self {
            token_cache_path,
            client_secret,
        })
    }
}

pub struct Secrets {
    pub imap_secrets: ImapSecrets,
    pub admin_password: Option<SecretString>,
}

impl Secrets {
    /// See [ImapSecrets].
    pub async fn initialize(secrets_dir: &Path) -> eyre::Result<Self> {
        let imap_secrets = ImapSecrets::initialize(secrets_dir)
            .await
            .wrap_err("Error initializing secrets for IMAP client")?;

        let admin_password_path = secrets_dir.join("admin_password");
        let admin_password = if admin_password_path.is_file() {
            tracing::info!(
                "Reading admin password from secret file: {:?}",
                admin_password_path
            );
            let password = tokio::fs::read_to_string(&admin_password_path)
                .await
                .wrap_err_with(|| {
                    format!(
                        "Error while reading admin password secret file {:?}",
                        admin_password_path
                    )
                })?;

            let stripped_password = password.strip_suffix('\n').unwrap_or(&password).to_string();
            Some(SecretString::new(stripped_password))
        } else {
            tracing::info!("Reading admin password from ADMIN_PASSWORD environment variable");
            match std::env::var("ADMIN_PASSWORD") {
                Ok(admin_password) => Some(SecretString::new(admin_password)),
                Err(VarError::NotPresent) => None,
                Err(unexpected) => {
                    return Err(unexpected)
                        .wrap_err("Error while reading ADMIN_PASSWORD environment variable")
                }
            }
        };

        Ok(Self {
            imap_secrets,
            admin_password,
        })
    }
}
