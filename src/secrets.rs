use std::{
    env::VarError,
    path::{Path, PathBuf},
};

use eyre::Context;
use secrecy::SecretString;

use crate::oauth2::{service_account, ClientSecretDefinition};

/// Secrets used to access email account via IMAP.
pub struct ImapSecrets {
    /// The path to the json file used for the OAUTH2 token cache. This file will be updated by
    /// this application when tokens expire and are refreshed.
    pub token_cache_path: PathBuf,
    /// OAUTH2 Installed client secret.
    pub client_secret: Option<ClientSecretDefinition>,
    pub service_account_key: Option<service_account::Key>,
}

async fn initialize_client_secret(
    secrets_dir: &Path,
) -> eyre::Result<Option<ClientSecretDefinition>> {
    Ok(match std::env::var("CLIENT_SECRET") {
        Ok(client_secret) => {
            tracing::debug!("Reading client secret from CLIENT_SECRET environment variable.");
            Some(
                serde_json::from_str::<ClientSecretDefinition>(&client_secret).wrap_err(
                    "Unable to parse client secret from CLIENT_SECRET environment variable",
                )?,
            )
        }
        Err(std::env::VarError::NotPresent) => {
            let secret_path = secrets_dir.join("client_secret.json");
            tracing::debug!("Reading client secret from file {:?}", &secret_path);

            if secret_path.exists() {
                Some(
                    {
                        let client_secret = tokio::fs::read_to_string(&secret_path).await?;
                        serde_json::from_str::<ClientSecretDefinition>(&client_secret)
                            .wrap_err("Unable to parse client secret")
                    }
                    .wrap_err_with(|| {
                        format!("Error reading oauth2 secret from file {:?}", secret_path)
                    })?,
                )
            } else {
                None
            }
        }
        Err(unexpected) => {
            return Err(eyre::Error::from(unexpected))
                .wrap_err("Error attempting to read CLIENT_SECRET environment variable")
        }
    })
}

async fn initialize_token_cache(secrets_dir: &Path) -> eyre::Result<PathBuf> {
    let token_cache_path = secrets_dir.join("token_cache.json");

    if std::env::var("DELETE_TOKEN_CACHE").is_ok() && token_cache_path.is_file() {
        tracing::warn!("Deleting existing token cache file: {:?}", token_cache_path);
        tokio::fs::remove_file(&token_cache_path)
            .await
            .wrap_err_with(|| format!("Error removing token cache file: {:?}", token_cache_path))?;
    }

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
                tokio::fs::write(&token_cache_path, &secret)
                    .await
                    .wrap_err_with(|| {
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
                tracing::debug!(
                    "Token cache {:?} will be automatically generated with Installed OAUTH2 flow",
                    token_cache_path
                );
            }
        }
        Err(unexpected) => {
            return Err(unexpected)
                .wrap_err("Error while reading TOKEN_CACHE environment variable");
        }
    }
    Ok(token_cache_path)
}

async fn initialize_service_account_key(
    secrets_dir: &Path,
) -> eyre::Result<Option<service_account::Key>> {
    Ok(match std::env::var("SERVICE_ACCOUNT_KEY") {
        Ok(service_account_key) => {
            tracing::debug!(
                "Reading service account key from SERVICE_ACCOUNT_KEY environment variable."
            );
            Some(serde_json::from_str::<service_account::Key>(&service_account_key).wrap_err(
                "Unable to parse service account key from SERVICE_ACCOUNT_KEY environment variable",
            )?)
        }
        Err(std::env::VarError::NotPresent) => {
            let secret_path = secrets_dir.join("service_account_key.json");
            tracing::debug!("Reading service account key from file {:?}", &secret_path);

            if secret_path.exists() {
                Some(
                    {
                        let service_account_key = tokio::fs::read_to_string(&secret_path).await?;
                        serde_json::from_str::<service_account::Key>(&service_account_key)
                            .wrap_err("Unable to parse service account key")
                    }
                    .wrap_err_with(|| {
                        format!("Error reading oauth2 secret from file {:?}", secret_path)
                    })?,
                )
            } else {
                None
            }
        }
        Err(unexpected) => {
            return Err(eyre::Error::from(unexpected))
                .wrap_err("Error attempting to read SERVICE_ACCOUNT_KEY environment variable")
        }
    })
}

impl ImapSecrets {
    /// Initializes secrets required for accessing IMAP.
    ///
    /// + If `CLIENT_SECRET` environment variable is set, the contents will be parsed, otherwise it
    ///   will be read from `client_secret.json` in the specified `secrets_dir` directory.
    /// + If `TOKEN_CACHE` environment variable is set, the contents will be written to
    ///   `token_cache.json` inside the specified `secrets_dir` directory. If the file already
    ///   exists then the existing file will be used instead. If the environment variable is not
    ///   set, then the cache will be initialized automatically using the interactive Installed
    ///   OAUTH2 flow.
    /// + If `OVERWRITE_TOKEN_CACHE` environment variable is set, and `TOKEN_CACHE` is also set,
    ///   then the existing token cache file is overwritten with the contents of `TOKEN_CACHE`.
    /// + If `DELETE_TOKEN_CACHE` environment variable is set, then the existing token cache file
    ///   is deleted.
    /// + `secrets_dir` needs to exist and have read/write permissions for this application.
    pub async fn initialize(secrets_dir: &Path) -> eyre::Result<Self> {
        if !secrets_dir.is_dir() {
            return Err(eyre::eyre!(
                "secrets_dir {:?} does not exist or is not a directory",
                secrets_dir
            ));
        }
        let client_secret = initialize_client_secret(secrets_dir)
            .await
            .wrap_err("Error initializing client secret")?;
        let token_cache_path = initialize_token_cache(secrets_dir)
            .await
            .wrap_err("Error initializing token cache")?;
        let service_account_key = initialize_service_account_key(secrets_dir)
            .await
            .wrap_err("Error initializing service account key")?;

        Ok(Self {
            token_cache_path,
            client_secret,
            service_account_key,
        })
    }
}

/// Secrets necessary for the operation of this application.
pub struct Secrets {
    /// Secrets used for accessing the service email account via IMAP.
    pub imap_secrets: ImapSecrets,
    /// `admin` user's password hashed using bcrypt
    pub admin_password_hash: Option<SecretString>,
}

impl Secrets {
    /// In addition to the secrets loaded by [`ImapSecrets`], there are the following:
    ///
    /// + `ADMIN_PASSWORD_HASH`: A `bcrypt` hash of the administrator password used to access the
    ///   application logs.
    pub async fn initialize(secrets_dir: &Path) -> eyre::Result<Self> {
        let imap_secrets = ImapSecrets::initialize(secrets_dir)
            .await
            .wrap_err("Error initializing secrets for IMAP client")?;

        let admin_password_hash = match std::env::var("ADMIN_PASSWORD_HASH") {
            Ok(admin_password) => {
                tracing::info!(
                    "Admin password hash was read from ADMIN_PASSWORD_HASH environment variable"
                );
                Some(SecretString::new(admin_password))
            }
            Err(VarError::NotPresent) => {
                let admin_password_path = secrets_dir.join("admin_password_hash");
                if admin_password_path.is_file() {
                    tracing::info!(
                        "Reading admin password hash from secret file: {:?}",
                        admin_password_path
                    );
                    let password = tokio::fs::read_to_string(&admin_password_path)
                        .await
                        .wrap_err_with(|| {
                            format!(
                                "Error while reading admin password secret hash file {:?}",
                                admin_password_path
                            )
                        })?;

                    let stripped_password =
                        password.strip_suffix('\n').unwrap_or(&password).to_string();
                    Some(SecretString::new(stripped_password))
                } else {
                    tracing::warn!("Admin debug/log interface disabled (because ADMIN_PASSWORD_HASH secret is unavailable)");
                    None
                }
            }
            Err(unexpected) => {
                return Err(unexpected)
                    .wrap_err("Error while reading ADMIN_PASSWORD_HASH environment variable")
            }
        };

        Ok(Self {
            imap_secrets,
            admin_password_hash,
        })
    }
}
