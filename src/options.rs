use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use color_eyre::Help;
use eyre::Context;
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

/// An email account address/username e.g. `my.email@example.com`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EmailAccount(String);

impl AsRef<str> for EmailAccount {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Display for EmailAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Global options for the application.
#[derive(Debug, Serialize, Deserialize)]
pub struct Options {
    /// Directory where application data is stored (including logs).
    ///
    /// Default is `data`.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Directory where secrets are loaded from (and the token cache is stored).
    ///
    /// Default is `secrets`.
    #[serde(default = "default_secrets_dir")]
    pub secrets_dir: PathBuf,
    /// Email account used for receiving/sending emails, the username for IMAP and SMTP.
    pub email_account: EmailAccount,
    /// Base url used for http server.
    ///
    /// Default is `http://localhost:3000/`.
    /// Can be specified by setting the environment variable `BASE_URL`.
    #[serde(default = "default_base_url")]
    pub base_url: url::Url,
    /// If `true` then the existing token cache file is deleted.
    ///
    /// Default is `false`.
    #[serde(default = "default_delete_token_cache")]
    pub delete_token_cache: bool,
    /// If `true`, and the `TOKEN_CACHE` secret is also set, then the existing token cache file is
    /// overwritten with the contents of `TOKEN_CACHE`.
    ///
    /// Default is `false`.
    #[serde(default = "default_overwrite_token_cache")]
    pub overwrite_token_cache: bool,
}

fn default_data_dir() -> PathBuf {
    "data".into()
}

fn default_secrets_dir() -> PathBuf {
    "secrets".into()
}

fn default_base_url() -> url::Url {
    "http://localhost:3000"
        .parse()
        .expect("Unable to parse url")
}

fn default_delete_token_cache() -> bool {
    false
}

fn default_overwrite_token_cache() -> bool {
    false
}

impl Options {
    /// Initialize the options using the `OPTIONS` environment variable, otherwise load from file
    /// `options.ron` by default. If `OPTIONS` contains a file path, it will load the options from
    /// that path, if `OPTIONS` contains a RON file definition then it will load the options from
    /// the string contained in the variable.
    pub async fn initialize() -> eyre::Result<Self> {
        let options_result = match std::env::var("OPTIONS") {
            Ok(options) => match ron::from_str(&options) {
                Ok(options) => {
                    println!("Options loaded from `OPTIONS` environment variable");
                    Ok(options)
                }
                Err(error) => {
                    let path = PathBuf::from(options);
                    if path.is_file() {
                        let options_str = tokio::fs::read_to_string(&path).await?;
                        let options: Options = ron::from_str(&options_str).wrap_err_with(|| {
                            format!("Error deserializing options file: {:?}", path)
                        })?;
                        println!("Options loaded from file specified in `OPTIONS` environment variable: {:?}", path);
                        Ok(options)
                    } else {
                        Err(error).wrap_err(
                            "Error deserializing options from `OPTIONS` environment variable \
                            string, or you have specified a file path which does not exist",
                        )
                    }
                }
            },
            Err(std::env::VarError::NotPresent) => {
                let path = Path::new("options.ron");
                if !path.is_file() {
                    return Err(eyre::eyre!(
                        "No `OPTIONS` environment variable specified, and options file \
                        `options.ron` does not exist."
                    )
                    .suggestion(
                        "The following options are available to solve this:\n\
                        + Create `options.ron`.\n\
                        + Specify options file location with `OPTIONS` environment variable.\n\
                        + Specify options in RON format in `OPTIONS` environment variable as a string.",
                    ));
                }
                let options_str = tokio::fs::read_to_string(&path).await?;
                let options = ron::from_str(&options_str)
                    .wrap_err_with(|| format!("Error deserializing options file: {:?}", path));

                println!("Options loaded from default file: {:?}", path);
                options
            }
            Err(error) => {
                return Err(error).wrap_err("Error reading `OPTIONS` environment variable")
            }
        };

        if let Ok(options) = &options_result {
            let options_str = ron::ser::to_string_pretty(options, PrettyConfig::default())?;
            println!("Options{}", options_str)
        }

        options_result
    }
}
