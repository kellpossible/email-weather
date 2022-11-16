//! See [`receive_emails()`].

use std::{borrow::Cow, sync::Arc};

use async_imap::types::Fetch;
use eyre::Context;
use futures::{StreamExt, TryStreamExt};
use oauth2::AccessToken;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, Mutex},
};
use tracing::Instrument;

use crate::{
    email, gis::Position, inreach, oauth2::AuthenticationFlow, plain,
    request::ParsedForecastRequest, task::run_retry_log_errors, time,
};

/// An email received via IMAP.
pub trait Received {
    /// Geographical position of sender of the message (if available).
    fn position(&self) -> Option<Position>;
    /// The subset of the received message containing the request specification.
    fn forecast_request(&self) -> &ParsedForecastRequest;
}

/// Sum type of all possible [`Email`]s that can be received and parsed via IMAP.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ReceivedKind {
    /// Email received from an inreach device.
    Inreach(inreach::email::Received),
    /// Plain text email.
    Plain(plain::email::Received),
}

/// Error that occurs while parsing a received email.
#[derive(Debug, thiserror::Error)]
pub enum ParseReceivedEmailError {
    /// The email being parsed was intentionally rejected.
    #[error("Rejected email because: {reason}")]
    Rejected {
        /// The reason why the email was rejected.
        reason: Cow<'static, str>,
    },
    /// An unexpected error occurred during parsing.
    #[error(transparent)]
    Unexpected(#[from] eyre::Error),
}

/// Parse a received email.
pub trait ParseReceivedEmail: Sized {
    /// Error produced while parsing the received email.
    type Err;

    /// Parses an email into self. Returns `None` if the email will deliberately not be
    /// parsed (e.g. not on the whitelist of `from_address`).
    fn parse_email(message: mail_parser::Message) -> Result<Self, Self::Err>;
}

pub(crate) fn text_body<'a>(message: &'a mail_parser::Message) -> eyre::Result<Cow<'a, str>> {
    let text_body = message
        .get_text_body(0)
        .ok_or_else(|| eyre::eyre!("No text body for message"))?;

    Ok(text_body)
}

pub(crate) fn from_account(message: &mail_parser::Message) -> eyre::Result<email::Account> {
    let from_header: &mail_parser::HeaderValue = message
        .get_header("From")
        .ok_or_else(|| eyre::eyre!("No From header for message"))?;

    if let mail_parser::HeaderValue::Address(address) = from_header {
        email::Account::try_from(address).wrap_err("Invalid From header address")
    } else {
        Err(eyre::eyre!(
            "Unexpected From header value: {:?}",
            from_header
        ))
    }
}

pub(crate) fn message_id<'a>(message: &'a mail_parser::Message) -> Option<&'a Cow<'a, str>> {
    message
        .get_header("Message-Id")
        .and_then(|header| match header {
            mail_parser::HeaderValue::Text(text) => Some(text),
            _ => {
                tracing::warn!("Unexpected `Message-Id` header format: {:?}", header);
                None
            }
        })
}

impl ParseReceivedEmail for ReceivedKind {
    type Err = ParseReceivedEmailError;

    fn parse_email(message: mail_parser::Message) -> Result<Self, Self::Err> {
        let from_account = from_account(&message)?;
        let email = match from_account.email_str() {
            "no.reply.inreach@garmin.com" => {
                Self::Inreach(inreach::email::Received::parse_email(message)?)
            }
            // TODO: use a whitelist from options as per #14
            _ => Self::Plain(plain::email::Received::parse_email(message)?),
        };

        Ok(email)
    }
}

impl Received for ReceivedKind {
    fn position(&self) -> Option<Position> {
        match self {
            ReceivedKind::Inreach(email) => email.position(),
            ReceivedKind::Plain(email) => email.position(),
        }
    }

    fn forecast_request(&self) -> &ParsedForecastRequest {
        match self {
            ReceivedKind::Inreach(email) => email.forecast_request(),
            ReceivedKind::Plain(email) => email.forecast_request(),
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
        // TODO: fetch and check RFC822.SIZE before fetching the entire body.
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
                .map(|(result, sequence)| match result {
                    Ok(ok) => Ok((sequence, ok)),
                    Err(error) => Err(map_imap_connection_error(
                        error,
                        format!(
                            "Error while fetching RFC822 from message with sequence ID {}",
                            sequence
                        ),
                    )),
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

                        let message: mail_parser::Message =
                            mail_parser::Message::parse(rfc822_body).ok_or_else(|| {
                                eyre::eyre!("Unable to parse fetched message body: {:?}", fetch)
                            })?;

                        match ReceivedKind::parse_email(message) {
                            Ok(email) => {
                                let email_data = serde_json::to_vec(&email)
                                    .wrap_err("Error serializing email data to json bytes")?;

                                let mut sender = emails_sender.lock().await;
                                sender
                                    .send(email_data)
                                    .await
                                    .wrap_err("Error submitting email data to send queue")?;

                                tracing::debug!("email added to queue: {:?}", email);
                            }
                            Err(error) => match error {
                                ParseReceivedEmailError::Rejected { .. } => {
                                    tracing::warn!("{}", error);
                                }
                                ParseReceivedEmailError::Unexpected(error) => {
                                    return Err(error.into())
                                }
                            },
                        }

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

async fn receive_emails_impl<AUTH>(
    process_sender: Arc<Mutex<yaque::Sender>>,
    oauth_flow: &AUTH,
    imap_username: &str,
    time: &dyn time::Port,
) -> eyre::Result<()>
where
    AUTH: AuthenticationFlow,
{
    loop {
        tracing::debug!("Starting receiving emails job");
        let tls = async_native_tls::TlsConnector::new();

        let imap_domain = "imap.gmail.com";

        let access_token = oauth_flow
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
#[tracing::instrument(skip_all)]
pub async fn receive_emails<AUTH>(
    shutdown_rx: broadcast::Receiver<()>,
    process_sender: yaque::Sender,
    oauth_flow: Arc<AUTH>,
    imap_username: &str,
    time: &dyn time::Port,
) where
    AUTH: AuthenticationFlow,
{
    let process_sender = Arc::new(Mutex::new(process_sender));
    run_retry_log_errors(
        move || {
            let process_sender = process_sender.clone();
            let oauth_flow = oauth_flow.clone();
            async move {
                receive_emails_impl(
                    process_sender,
                    &*oauth_flow,
                    imap_username,
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
