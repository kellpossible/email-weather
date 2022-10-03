use std::{collections::HashSet, path::Path, str::FromStr, sync::Arc};

use async_imap::types::{Fetch, Seq};
use email_weather::inreach;
use eyre::Context;
use futures::{StreamExt, TryStreamExt};
use open_meteo::{DailyWeatherVariable, Forecast, ForecastParameters, HourlyVariable};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex,
};
use tracing::Instrument;
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

#[derive(Serialize, Deserialize, Debug)]
enum Email {
    Inreach(inreach::email::Email),
}

async fn receive_emails_poll_inbox<T>(
    emails_sender: Arc<Mutex<yaque::Sender>>,
    imap_session: &mut async_imap::Session<T>,
) -> eyre::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    imap_session.select("INBOX").await?;
    tracing::debug!("IMAP INBOX selected");

    let sequence_set: Vec<String> = imap_session
        .search("UNSEEN")
        .await
        .wrap_err("Error while searching for UNSEEN messages")?
        .iter()
        .map(ToString::to_string)
        .collect();

    tracing::debug!("Obtained UNSEEN messages: {:?}", sequence_set);

    if !sequence_set.is_empty() {
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

                        let from_address = match from_header {
                            mail_parser::HeaderValue::Address(address) => {
                                address.address.as_ref().ok_or_else(|| {
                                    eyre::eyre!(
                                        "From header is missing the email address: {:?}",
                                        from_header
                                    )
                                })?
                            }
                            _ => {
                                tracing::debug!(
                                    "Skipping message due to unexpected From header value: {:?}",
                                    from_header
                                );
                                return Ok(());
                            }
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
    emails_sender: Arc<Mutex<yaque::Sender>>,
    imap_session: &mut async_imap::Session<T>,
) -> eyre::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    loop {
        receive_emails_poll_inbox(emails_sender.clone(), imap_session).await?;
    }
}

#[tracing::instrument(skip(emails_sender, shutdown_rx))]
async fn receive_emails(
    emails_sender: yaque::Sender,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> eyre::Result<()> {
    tracing::debug!("Starting receiving emails job");
    let emails_sender = Arc::new(Mutex::new(emails_sender));
    let tls = async_native_tls::TlsConnector::new();

    let imap_domain = "imap.gmail.com";
    let imap_username = "email.weather.service@gmail.com";

    let secret = yup_oauth2::read_application_secret("secrets/clientsecret.json")
        .await
        .wrap_err("Error reading oauth2 secret `secrets/clientsecret.json`")?;

    let auth = InstalledFlowAuthenticator::builder(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::Interactive,
    )
    .persist_tokens_to_disk("secrets/tokencache.json")
    .build()
    .await
    .wrap_err("Error running OAUTH2 installed flow")?;

    let scopes = &[
        // https://developers.google.com/gmail/imap/xoauth2-protocol
        "https://mail.google.com/",
    ];

    let access_token: yup_oauth2::AccessToken = auth
        .token(scopes)
        .await
        .wrap_err("Error obtaining OAUTH2 access token")?;

    let gmail_auth = GmailOAuth2 {
        user: String::from(imap_username),
        access_token: access_token.as_str().to_string(),
    };

    tracing::info!("Logging in to {} email via IMAP", imap_username);
    let imap_client = async_imap::connect((imap_domain, 993), imap_domain, tls).await?;
    let mut imap_session: async_imap::Session<_> = imap_client
        .authenticate("XOAUTH2", &gmail_auth)
        .await
        .map_err(|e| e.0)
        .wrap_err("Error authenticating with XOAUTH2")?;
    // let mut imap_session = imap_client.login(imap_username, imap_password).await.map_err(|error| error.0)?;
    tracing::info!("Successful imap session login");

    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            result.map_err(eyre::Error::from)
        }
        result = receive_emails_poll_inbox_loop(emails_sender, &mut imap_session) => result
    }?;

    tracing::info!("Logging out of IMAP session");

    imap_session.logout().await?;

    Ok(())
}

async fn process_emails_impl(
    mut emails_receiver: yaque::Receiver,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    loop {
        let received = emails_receiver.recv().await?;
        let received_email: Email = serde_json::from_slice(&*received)?;

        let (latitude, longitude): (f32, f32) = match &received_email {
            Email::Inreach(email) => (email.latitude, email.longitude),
        };

        let forecast_parameters = ForecastParameters::builder()
            .latitude(latitude)
            .longitude(longitude)
            .hourly_entry(HourlyVariable::Temperature2m)
            .hourly_entry(HourlyVariable::CloudCover)
            .hourly_entry(HourlyVariable::FreezingLevelHeight)
            .build();

        tracing::debug!("Obtaining forecast for {:?}", forecast_parameters);
        let forecast: Forecast = open_meteo::obtain_forecast(&http_client, &forecast_parameters)
            .await
            .wrap_err("Error obtaining forecast")?;
        tracing::info!("Obtained forecast: {:?}", forecast);

        tracing::info!("Sending reply for email {:?}", received_email);
        received.commit()?;
    }
}

#[tracing::instrument(skip(emails_receiver, shutdown_rx, http_client))]
async fn process_emails(
    emails_receiver: yaque::Receiver,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    tracing::debug!("Starting processing emails job");
    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            result.map_err(eyre::Error::from)
        }
        result = process_emails_impl(emails_receiver, http_client) => { result }
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let rust_log_env: String = std::env::var("RUST_LOG").unwrap_or("debug".to_string());
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_str(rust_log_env.as_str()).unwrap_or_default())
        .with(tracing_error::ErrorLayer::default())
        .init();
    color_eyre::install()?;
    let http_client = reqwest::Client::new();

    let (shutdown_tx, emails_receive_shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let emails_process_shutdown_rx = shutdown_tx.subscribe();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen to ctrl-c event");
        tracing::warn!("ctrl-c event detected, broadcasting shutdown");
        shutdown_tx
            .send(())
            .expect("failed to send shutdown broadcast");
    });

    let data_path = Path::new("data");
    if !data_path.exists() {
        std::fs::create_dir(data_path).wrap_err("unable to create data/ directory")?;
    }
    let emails_queue_path = data_path.join("emails");
    let (emails_sender, emails_receiver) =
        yaque::channel(emails_queue_path).wrap_err("unable to create emails queue")?;

    let receive_join = tokio::spawn(receive_emails(emails_sender, emails_receive_shutdown_rx));
    let process_join = tokio::spawn(process_emails(
        emails_receiver,
        emails_process_shutdown_rx,
        http_client,
    ));

    // TODO: perhaps use a select here instead? What happens if failure occurs during setup before
    // running properly actually starts?
    receive_join.await??;
    process_join.await??;

    Ok(())
}
