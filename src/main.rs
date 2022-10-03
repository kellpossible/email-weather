use std::str::FromStr;

use eyre::Context;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};
use yup_oauth2::InstalledFlowAuthenticator;

struct GmailOAuth2 {
    user: String,
    access_token: String,
}
impl async_imap::Authenticator for &GmailOAuth2 {
    type Response = String;

    fn process(&mut self, _data: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let rust_log_env: String = std::env::var("RUST_LOG").unwrap_or("debug".to_string());
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_str(rust_log_env.as_str()).unwrap_or_default())
        .init();
    color_eyre::install()?;

    let tls = async_native_tls::TlsConnector::new();
    let http_client = reqwest::Client::new();

    let imap_domain = "imap.gmail.com";
    let imap_username = "email.weather.service@gmail.com";

    let secret = yup_oauth2::read_application_secret("secrets/clientsecret.json").await.wrap_err("Error reading oauth2 secret `secrets/clientsecret.json`")?;

    let mut auth = InstalledFlowAuthenticator::builder(secret, yup_oauth2::InstalledFlowReturnMethod::Interactive)
        .persist_tokens_to_disk("secrets/tokencache.json")
        .build()
        .await
        .wrap_err("Error running OAUTH2 installed flow")?;

    let scopes = &[
        // https://developers.google.com/gmail/imap/xoauth2-protocol
        "https://mail.google.com/"
    ];

    let access_token: yup_oauth2::AccessToken = auth.token(scopes).await.wrap_err("Error obtaining OAUTH2 access token")?;

    let gmail_auth = GmailOAuth2 {
        user: String::from(imap_username),
        access_token: access_token.as_str().to_string(),
    };

    tracing::info!("Logging in to {} email via IMAP", imap_username);
    let imap_client = async_imap::connect((imap_domain, 993), imap_domain, tls).await?;
    let mut imap_session = imap_client.authenticate("XOAUTH2", &gmail_auth).await.map_err(|e| e.0).wrap_err("Error authenticating with XOAUTH2")?;
    // let mut imap_session = imap_client.login(imap_username, imap_password).await.map_err(|error| error.0)?;
    tracing::debug!("Successful imap session login");
    imap_session.logout().await?;


    Ok(())
}
