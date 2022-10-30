use eyre::Context;

/// Global options for the application.
pub struct Options {
    /// Base url used for http server.
    ///
    /// Default is `http://localhost:3000/`.
    /// Can be specified by setting the environment variable `BASE_URL`.
    pub base_url: url::Url,
}

impl Options {
    /// Initialize the options using either the environment variables or default values.
    pub fn initialize() -> eyre::Result<Self> {
        let base_url: url::Url = match std::env::var("BASE_URL") {
            Ok(redirect_url) => {
                tracing::debug!("Reading base url from BASE_URL environment variable.");
                redirect_url
                    .parse()
                    .wrap_err("Error while parsing environment variable BASE_URL")?
            }
            Err(std::env::VarError::NotPresent) => "http://localhost:3000/"
                .parse()
                .expect("Expected redirect url to be in correct format"),
            Err(error) => {
                return Err(error).wrap_err("Error reading environment variable BASE_URL")
            }
        };

        Ok(Self { base_url })
    }
}
