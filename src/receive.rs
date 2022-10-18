//! See [`receive_emails()`].

use std::sync::Arc;

use async_imap::types::Fetch;
use eyre::Context;
use futures::{StreamExt, TryStreamExt};
use oauth2::AccessToken;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex,
};
use tracing::Instrument;

use crate::{inreach, secrets::ImapSecrets, task::run_retry_log_errors};

#[derive(Serialize, Deserialize, Debug)]
pub enum Email {
    Inreach(inreach::email::Email),
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

// TODO: handle expected IMAP errors more gracefully
async fn receive_emails_poll_inbox<T>(
    emails_sender: Arc<Mutex<yaque::Sender>>,
    imap_session: &mut async_imap::Session<T>,
) -> eyre::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    tracing::trace!("Polling IMAP INBOX");
    imap_session.select("INBOX").await?;

    let sequence_set: Vec<String> = imap_session
        .search("UNSEEN")
        .await
        .wrap_err("Error while searching for UNSEEN messages")?
        .iter()
        .map(ToString::to_string)
        .collect();

    if !sequence_set.is_empty() {
        tracing::debug!("Obtained UNSEEN messages: {:?}", sequence_set);
        let fetch_sequences: String = sequence_set.join(",");
        {
            let fetch_stream = imap_session.fetch(fetch_sequences, "RFC822").await?;
            fetch_stream
                .zip(futures::stream::iter(sequence_set.iter()))
                .map(|(result, sequence)| result.map(|ok| (sequence, ok)))
                .map_err(eyre::Error::from)
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

                        let email: Email = Email::Inreach(text_body.parse().wrap_err("Unable to parse text body as a valid inreach email")?);
                        let email_data = serde_json::to_vec(&email)?;

                        let mut sender = emails_sender.lock().await;
                        sender.send(email_data).await?;

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
) -> eyre::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    loop {
        receive_emails_poll_inbox(process_sender.clone(), imap_session).await?;
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

async fn receive_emails_impl(
    process_sender: Arc<Mutex<yaque::Sender>>,
    imap_secrets: &ImapSecrets,
) -> eyre::Result<()> {
    tracing::debug!("Starting receiving emails job");
    let tls = async_native_tls::TlsConnector::new();

    let imap_domain = "imap.gmail.com";
    let imap_username = "email.weather.service@gmail.com";

    let scopes = vec![
        // https://developers.google.com/gmail/imap/xoauth2-protocol
        oauth2::Scope::new("https://mail.google.com/".to_string()),
    ];

    let access_token = crate::oauth2::authenticate(
        imap_secrets.client_secret.clone(),
        scopes,
        &imap_secrets.token_cache_path,
    )
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
        .map_err(|e| e.0)
        .wrap_err("Error authenticating with XOAUTH2")?;
    // let mut imap_session = imap_client.login(imap_username, imap_password).await.map_err(|error| error.0)?;
    tracing::info!("Successful IMAP session login");

    receive_emails_poll_inbox_loop(process_sender.clone(), &mut imap_session).await?;

    tracing::info!("Logging out of IMAP session");

    imap_session.logout().await?;

    Ok(())
}

/// This function spawns a task to receive emails via IMAP, and submit them for processing.
#[tracing::instrument(skip(process_sender, shutdown_rx, imap_secrets))]
pub async fn receive_emails(
    process_sender: yaque::Sender,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    imap_secrets: &ImapSecrets,
) {
    let process_sender = Arc::new(Mutex::new(process_sender));
    run_retry_log_errors(
        move || {
            let process_sender = process_sender.clone();
            async move { receive_emails_impl(process_sender, imap_secrets).await }
        },
        shutdown_rx,
    )
    .await
}
