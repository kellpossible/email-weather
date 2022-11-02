//! Global options for the application.
//!
//! See [`Options`].

use std::{
    borrow::Cow,
    fmt::Display,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use color_eyre::Help;
use eyre::Context;
use ron::ser::PrettyConfig;
use serde::{ser::Error, Deserialize, Serialize};
use tracing::{Level, Metadata};

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
    /// Address by the http server for listening.
    ///
    /// Default is `127.0.0.1:3000`.
    #[serde(default = "default_listen_address")]
    pub listen_address: SocketAddr,
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

fn default_listen_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 3000))
}

fn default_delete_token_cache() -> bool {
    false
}

fn default_overwrite_token_cache() -> bool {
    false
}

impl Display for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let options_str = ron::ser::to_string_pretty(self, PrettyConfig::default())
            .map_err(|error| std::fmt::Error::custom(error))?;
        f.write_str("Options")?;
        f.write_str(&options_str)
    }
}

/// Storage for logging entries that will be printed later, when application logging setup is
/// completed.
#[derive(Default)]
pub struct Logs {
    logs: Vec<(Level, Cow<'static, str>)>,
}

impl Logs {
    fn push(&mut self, level: Level, message: impl Into<Cow<'static, str>>) {
        self.logs.push((level, message.into()))
    }

    /// Print logs using println.
    pub fn print(&self) {
        for event in &self.logs {
            println!("{}: {}", event.0, event.1)
        }
    }

    /// Present logs using tracing methods.
    pub fn present(&self) {
        for event in &self.logs {
            match event.0 {
                Level::INFO => tracing::info!("{}", event.1),
                Level::WARN => tracing::warn!("{}", event.1),
                Level::DEBUG => tracing::debug!("{}", event.1),
                Level::ERROR => tracing::error!("{}", event.1),
                Level::TRACE => tracing::trace!("{}", event.1),
            }
        }
    }
}

/// Result of [`Options::initialize()`].
pub struct OptionsInit {
    /// Options that were initialized.
    pub result: eyre::Result<Options>,
    /// Messages that are destined to logged after tracing has been
    /// initialized.
    pub logs: Logs,
}

impl Options {
    /// Initialize the options using the `OPTIONS` environment variable, otherwise load from file
    /// `options.ron` by default. If `OPTIONS` contains a file path, it will load the options from
    /// that path, if `OPTIONS` contains a RON file definition then it will load the options from
    /// the string contained in the variable.
    pub async fn initialize() -> OptionsInit {
        let mut logs = Logs::default();
        let result = initialize_impl(&mut logs).await;

        OptionsInit { result, logs }
    }
}

async fn initialize_impl(logs: &mut Logs) -> eyre::Result<Options> {
    let result = match std::env::var("OPTIONS") {
        Ok(options) => match ron::from_str(&options) {
            Ok(options) => {
                logs.push(
                    Level::INFO,
                    "Options loaded from `OPTIONS` environment variable",
                );
                Ok(options)
            }
            Err(error) => {
                let path = PathBuf::from(&options);
                if path.is_file() {
                    let options_str = tokio::fs::read_to_string(&path).await?;
                    let options: Options = ron::from_str(&options_str).wrap_err_with(|| {
                        format!("Error deserializing options file: {:?}", path)
                    })?;
                    logs.push(Level::INFO, format!("Options loaded from file specified in `OPTIONS` environment variable: {:?}", path));
                    Ok(options)
                } else {
                    Err(error).wrap_err_with(|| {
                        format!(
                            "Error deserializing options from `OPTIONS` environment variable \
                        string, or you have specified a file path which does not exist: {:?}",
                            options
                        )
                    })
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
                .wrap_err_with(|| format!("Error deserializing options file: {:?}", path))?;

            logs.push(
                Level::INFO,
                format!("Options loaded from default file: {:?}", path),
            );

            Ok(options)
        }
        Err(error) => return Err(error).wrap_err("Error reading `OPTIONS` environment variable"),
    };
    if let Ok(options) = &result {
        logs.push(Level::INFO, format!("{}", options));
    }
    result
}
