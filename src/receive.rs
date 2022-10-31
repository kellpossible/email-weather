//! See [`receive_emails()`].

use std::{borrow::Cow, str::FromStr, sync::Arc};

use async_imap::types::Fetch;
use eyre::Context;
use futures::{StreamExt, TryStreamExt};
use oauth2::{AccessToken, RedirectUrl};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc, Mutex},
};
use tracing::Instrument;

use crate::{
    gis::Position,
    inreach,
    oauth2::{AuthenticationFlow, ConsentRedirect, RedirectParameters},
    plain,
    secrets::ImapSecrets,
    task::run_retry_log_errors,
    time,
};

/// An email received via IMAP.
pub trait Email {
    /// Position (latitude, longitude).
    fn position(&self) -> Position;
}

/// Sum type of all possible [`Email`]s that can be received and parsed via IMAP.
#[derive(Serialize, Deserialize, Debug)]
pub enum EmailKind {
    /// Email received from an inreach device.
    Inreach(inreach::email::Email),
    /// Plain text email.
    Plain(plain::email::Email),
}

impl EmailKind {
    /// Parses an email into [`EmailKind`]. Returns `None` if the email will deliberately not be
    /// parsed (e.g. not on the whitelist of `from_address`).
    fn parse(from_address: mail_parser::Addr, body: &str) -> eyre::Result<Option<EmailKind>> {
        let from_address = if let Some(from_address) = from_address.address {
            from_address
        } else {
            return Ok(None);
        };

        let email = match from_address.as_ref() {
            "no.reply.inreach@garmin.com" => Self::Inreach(inreach::email::Email::from_str(body)?),
            "l.frisken@gmail.com" => Self::Plain(plain::email::Email::from_str(body)?),
            _ => {
                tracing::warn!(
                    "Skipping processing message because it is not from a whitelisted address: {}",
                    from_address
                );
                return Ok(None);
            }
        };

        Ok(Some(email))
    }
}

impl Email for EmailKind {
    fn position(&self) -> Position {
        match self {
            EmailKind::Inreach(email) => email.position(),
            EmailKind::Plain(email) => email.position(),
        }
    }
}

struct GmailOAuth2 {
    user: String,
    access_token: AccessToken,
}

impl async_imap::Authenticator for &GmailOAuth2 {
    type Response = String;

    fn process(&mut self, _data: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user,
            self.access_token.secret()
        )
    }
}

#[derive(Debug)]
enum PollEmailsError {
    Connection {
        error: async_imap::error::Error,
        message: Cow<'static, str>,
    },
    Unexpected(eyre::Error),
}

impl PollEmailsError {
    /// Convert this error into an [`eyre::Error`].
    fn into_eyre(self) -> eyre::Error {
        match self {
            PollEmailsError::Connection { .. } => self.into(),
            PollEmailsError::Unexpected(error) => error,
        }
    }
}

impl std::error::Error for PollEmailsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PollEmailsError::Connection { error, .. } => Some(error),
            PollEmailsError::Unexpected(_) => None,
        }
    }
}

impl std::fmt::Display for PollEmailsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollEmailsError::Connection { message, .. } => {
                write!(f, "An IMAP connection error occurred: {}", message)
            }
            PollEmailsError::Unexpected(error) => error.fmt(f),
        }
    }
}

impl From<eyre::Error> for PollEmailsError {
    fn from(error: eyre::Error) -> Self {
        Self::Unexpected(error)
    }
}

fn map_imap_connection_error(
    error: async_imap::error::Error,
    message: impl Into<Cow<'static, str>>,
) -> PollEmailsError {
    let message = message.into();
    match error {
        async_imap::error::Error::Io(_) | async_imap::error::Error::ConnectionLost => {
            PollEmailsError::Connection { error, message }
        }
        _ => PollEmailsError::Unexpected(
            eyre::Error::from(error)
                .wrap_err(format!("Unexpected IMAP error occurred: {}", message)),
        ),
    }
}

async fn receive_emails_poll_inbox<T>(
    emails_sender: Arc<Mutex<yaque::Sender>>,
    imap_session: &mut async_imap::Session<T>,
) -> Result<(), PollEmailsError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    tracing::trace!("Polling IMAP INBOX");
    imap_session
        .select("INBOX")
        .await
        .map_err(|error| map_imap_connection_error(error, "Error while selecting INBOX"))?;

    let unseen_messages =
        imap_session
            .search("UNSEEN")
            .await
            .map_err(|error: async_imap::error::Error| {
                map_imap_connection_error(error, "Error while searching for UNSEEN messages")
            })?;
    let sequence_set: Vec<String> = unseen_messages.iter().map(ToString::to_string).collect();

    if !sequence_set.is_empty() {
        tracing::debug!("Obtained UNSEEN messages: {:?}", sequence_set);
        let fetch_sequences: String = sequence_set.join(",");
        {
            let fetch_stream = imap_session
                .fetch(fetch_sequences, "RFC822")
                .await
                .map_err(|error: async_imap::error::Error| {
                    map_imap_connection_error(
                        error,
                        "Error while constructing stream to fetch RFC822 from messages",
                    )
                })?;
            fetch_stream
                .zip(futures::stream::iter(sequence_set.iter()))
                .map(|(result, sequence)| {
                    match result {
                        Ok(ok) => Ok((sequence, ok)),
                        Err(error) => {
                            Err(map_imap_connection_error(error, format!("Error while fetching RFC822 from message with sequence ID {}", sequence)))
                        },
                    }
                })
                .and_then(|(sequence, fetch): (&String, Fetch)| {
                    let emails_sender = emails_sender.clone();
                    async move {
                        let rfc822_body = if let Some(body) = fetch.body() {
                            body
                        } else {
                            tracing::debug!("Ignoring fetched message with no body: {:?}", fetch);
                            return Ok(());
                        };

                        let message =
                            mail_parser::Message::parse(rfc822_body).ok_or_else(|| {
                                eyre::eyre!("Unable to parse fetched message body: {:?}", fetch)
                            })?;

                        let from_header: &mail_parser::HeaderValue = message
                            .get_header("From")
                            .ok_or_else(|| eyre::eyre!("No From header for message"))?;

                        let from_address = if let mail_parser::HeaderValue::Address(address) = from_header {
                            address.address.as_ref().ok_or_else(|| {
                                eyre::eyre!(
                                    "From header is missing the email address: {:?}",
                                    from_header
                                )
                            })?
                        } else {
                            tracing::debug!(
                                "Skipping message due to unexpected From header value: {:?}",
                                from_header
                            );
                            return Ok(());
                        };

                        if from_address.as_ref() != "no.reply.inreach@garmin.com" {
                            tracing::warn!(
                                "Skipping processing message because it is not from a whitelisted address: {}",
                                from_address
                            );
                            return Ok(());
                        }

                        let text_body = message
                            .get_text_body(0)
                            .ok_or_else(|| eyre::eyre!("No text body for message"))?;

                        tracing::debug!("text_body: {}", text_body);

                        let email: EmailKind = EmailKind::Inreach(text_body.parse().wrap_err("Unable to parse text body as a valid inreach email")?);
                        let email_data = serde_json::to_vec(&email).wrap_err("Error serializing email data to json bytes")?;

                        let mut sender = emails_sender.lock().await;
                        sender.send(email_data).await.wrap_err("Error submitting email data to send queue")?;

                        tracing::debug!("email added to queue: {:?}", email);

                        Ok(())
                    }
                    .instrument(tracing::info_span!("process_message", seq = sequence))
                })
                .for_each(|result| async move {
                    match result {
                        Ok(_) => {}
                        Err(error) => {
                            tracing::error!("Error processing message: {:?}", error);
                        }
                    }
                })
                .await;
        }
    }

    Ok(())
}

async fn receive_emails_poll_inbox_loop<T>(
    process_sender: Arc<Mutex<yaque::Sender>>,
    imap_session: &mut async_imap::Session<T>,
    time: &dyn time::Port,
) -> Result<(), PollEmailsError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    loop {
        receive_emails_poll_inbox(process_sender.clone(), imap_session).await?;
        time.async_sleep(std::time::Duration::from_secs(10)).await;
    }
}

async fn receive_emails_impl(
    process_sender: Arc<Mutex<yaque::Sender>>,
    imap_secrets: &ImapSecrets,
    oauth_redirect_rx: Arc<Mutex<mpsc::Receiver<RedirectParameters>>>,
    base_url: &url::Url,
    time: &dyn time::Port,
) -> eyre::Result<()> {
    loop {
        tracing::debug!("Starting receiving emails job");
        let tls = async_native_tls::TlsConnector::new();

        let imap_domain = "imap.gmail.com";
        let imap_username = "email.weather.service@gmail.com";

        let scopes = vec![
            // https://developers.google.com/gmail/imap/xoauth2-protocol
            oauth2::Scope::new("https://mail.google.com/".to_string()),
        ];

        let redirect_url = RedirectUrl::from_url(base_url.join("oauth2")?);
        let flow = crate::oauth2::InstalledFlow::new(
            ConsentRedirect::Http {
                redirect_rx: oauth_redirect_rx.clone(),
                url: redirect_url,
            },
            imap_secrets
                .client_secret
                .clone()
                .ok_or_else(|| eyre::eyre!("Client secret has not been provided, and is required for Installed OAUTH2 flow"))?,
            scopes,
            imap_secrets.token_cache_path.clone(),
            // DeviceAuthorizationUrl::new("https://oauth2.googleapis.com/device/code".into())?,
        );

        let access_token = flow
            .authenticate()
            .await
            .wrap_err("Error obtaining OAUTH2 access token")?;

        let gmail_auth = GmailOAuth2 {
            user: String::from(imap_username),
            access_token,
        };

        tracing::info!("Logging in to {} email via IMAP", imap_username);
        let imap_client = async_imap::connect((imap_domain, 993), imap_domain, tls).await?;
        let mut imap_session: async_imap::Session<_> = imap_client
            .authenticate("XOAUTH2", &gmail_auth)
            .await
            .map_err(|(error, _)| error)
            .wrap_err("Error authenticating with XOAUTH2")?;
        // let mut imap_session = imap_client.login(imap_username, imap_password).await.map_err(|error| error.0)?;
        tracing::info!("Successful IMAP session login");

        match receive_emails_poll_inbox_loop(process_sender.clone(), &mut imap_session, time).await
        {
            Ok(_) => {}
            Err(error) => match error {
                PollEmailsError::Connection { .. } => {
                    tracing::debug!(
                        "Restarting IMAP session after anticipated connection error: {:?}",
                        error
                    );
                    continue;
                }
                PollEmailsError::Unexpected(_) => {
                    return Err(error
                        .into_eyre()
                        .wrap_err("Unexpected error while polling email inbox"))
                }
            },
        };

        tracing::info!("Logging out of IMAP session");
        imap_session.logout().await?;
        break;
    }

    Ok(())
}

/// This function spawns a task to receive emails via IMAP, and submit them for processing.
#[tracing::instrument(skip(
    process_sender,
    shutdown_rx,
    oauth_redirect_rx,
    imap_secrets,
    base_url,
    time,
))]
pub async fn receive_emails(
    process_sender: yaque::Sender,
    shutdown_rx: broadcast::Receiver<()>,
    oauth_redirect_rx: mpsc::Receiver<RedirectParameters>,
    imap_secrets: &ImapSecrets,
    base_url: &url::Url,
    time: &dyn time::Port,
) {
    let process_sender = Arc::new(Mutex::new(process_sender));
    let oauth_redirect_rx = Arc::new(Mutex::new(oauth_redirect_rx));
    run_retry_log_errors(
        move || {
            let process_sender = process_sender.clone();
            let oauth_redirect_rx = oauth_redirect_rx.clone();
            async move {
                receive_emails_impl(
                    process_sender,
                    imap_secrets,
                    oauth_redirect_rx,
                    base_url,
                    time,
                )
                .await
            }
        },
        shutdown_rx,
        time,
    )
    .await;
}
