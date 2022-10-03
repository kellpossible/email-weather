use std::{collections::HashSet, path::Path, str::FromStr, sync::Arc};

use async_imap::types::Fetch;
use chrono_tz::OffsetComponents;
use email_weather::inreach;
use eyre::Context;
use futures::{StreamExt, TryStreamExt};
use open_meteo::{Forecast, ForecastParameters, Hourly, HourlyVariable, TimeZone, WeatherCode};
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
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
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
    mut reply_sender: yaque::Sender,
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
            // .hourly_entry(HourlyVariable::Temperature2m)
            .hourly_entry(HourlyVariable::CloudCover)
            .hourly_entry(HourlyVariable::FreezingLevelHeight)
            .hourly_entry(HourlyVariable::WeatherCode)
            .hourly_entry(HourlyVariable::Precipitation)
            .timezone(TimeZone::Auto)
            .build();

        tracing::debug!(
            "Obtaining forecast for forecast parameters {}",
            serde_json::to_string_pretty(&forecast_parameters)?
        );
        let forecast: Forecast = open_meteo::obtain_forecast(&http_client, &forecast_parameters)
            .await
            .wrap_err("Error obtaining forecast")?;
        tracing::info!("Successfully obtained forecast");

        let hourly: Hourly = forecast
            .hourly
            .ok_or_else(|| eyre::eyre!("expected hourly forecast to be present"))?;
        let time: &[chrono::NaiveDateTime] = &hourly.time;

        // let temperature: &[f32] = &hourly
        //     .temperature_2m
        //     .ok_or_else(|| eyre::eyre!("expected temperature_2m to be present"))?;
        let weather_code: &[WeatherCode] = &hourly
            .weather_code
            .ok_or_else(|| eyre::eyre!("expected weather_code to be present"))?;
        let precipitation: &[f32] = &hourly
            .precipitation
            .ok_or_else(|| eyre::eyre!("expected precipitation to be present"))?;
        let cloud_cover: &[f32] = &hourly
            .cloud_cover
            .ok_or_else(|| eyre::eyre!("expected cloud_cover to be present"))?;
        let freezing_level_height: &[f32] = &hourly
            .freezing_level_height
            .ok_or_else(|| eyre::eyre!("expected freezing_level_height to be present"))?;

        if [
            time.len(),
            // temperature.len(),
            precipitation.len(),
            freezing_level_height.len(),
            cloud_cover.len(),
        ]
        .into_iter()
        .collect::<HashSet<usize>>()
        .len()
            != 1
        {
            eyre::bail!("forecast hourly array lengths don't match")
        }

        let mut messages: Vec<String> = Vec::new();
        let utc_now: chrono::NaiveDateTime = chrono::Utc::now().naive_utc();
        let offset = chrono::TimeZone::offset_from_utc_datetime(&forecast.timezone, &utc_now);
        let current_local_time: chrono::NaiveDateTime =
            chrono::TimeZone::from_utc_datetime(&forecast.timezone, &utc_now).naive_local();
        tracing::debug!("current local time: {}", current_local_time);
        let total_offset: chrono::Duration = offset.base_utc_offset() + offset.dst_offset();

        if total_offset.num_seconds() != forecast.utc_offset_seconds {
            tracing::warn!(
                "Reported timezone offsets don't match {} != {}",
                total_offset.num_seconds(),
                forecast.utc_offset_seconds
            );
        }

        let formatted_offset: String = if total_offset.is_zero() {
            format!("GMT")
        } else {
            let formatted_duration = format!(
                "{:02}:{:02}",
                total_offset.num_hours(),
                total_offset.num_minutes() % 60
            );
            if total_offset > chrono::Duration::zero() {
                format!("+{}", formatted_duration)
            } else {
                format!("-{}", formatted_duration)
            }
        };

        messages.push(format!("Tz{} E{:.0}", formatted_offset, forecast.elevation));

        // Skip times that are after the current local time.
        let start_i: usize = time.iter().enumerate().fold(0, |acc, (i, local_time)| {
            if current_local_time > *local_time {
                usize::min(i + 1, time.len() - 1)
            } else {
                acc
            }
        });

        let mut i = start_i;
        let mut acc_precipitation: f32 = 0.0;
        while i <= usize::min(time.len() - 1, i + 48) {
            acc_precipitation += precipitation[i];
            if (i - start_i) % 6 == 0 {
                let formatted_time = time[i].format("%dT%H");
                messages.push(format!(
                    "{} F{:.0} C{:.0} W{} P{:.0}",
                    formatted_time,
                    freezing_level_height[i],
                    cloud_cover[i],
                    weather_code[i] as u8,
                    acc_precipitation,
                ));
                acc_precipitation = 0.0;
            }
            i += 1;
        }

        tracing::info!("Sending reply for email {:?}", received_email);

        let mut message: String = String::new();
        for (i, m) in messages.into_iter().enumerate() {
            if message.len() + m.len() > 160 {
                break;
            }

            if i > 0 {
                message.push('\n')
            }
            message.push_str(&m);
        }
        tracing::info!("message (len: {}):\n{}", message.len(), message);

        let reply = match received_email {
            Email::Inreach(email) => Reply::InReach(InReachReply {
                referral_url: email.referral_url,
                message,
            }),
        };

        let reply_bytes = serde_json::to_vec(&reply).wrap_err("Failed to serialize reply")?;
        reply_sender.send(&reply_bytes).await?;

        received.commit()?;
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct InReachReply {
    referral_url: url::Url,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
enum Reply {
    InReach(InReachReply),
}

#[tracing::instrument(skip(emails_receiver, reply_sender, shutdown_rx, http_client))]
async fn process_emails(
    emails_receiver: yaque::Receiver,
    reply_sender: yaque::Sender,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    tracing::debug!("Starting processing emails job");
    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            result.map_err(eyre::Error::from)
        }
        result = process_emails_impl(emails_receiver, reply_sender, http_client) => { result }
    }
}

async fn send_replies_impl(
    mut reply_receiver: yaque::Receiver,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    loop {
        let reply_bytes = reply_receiver.recv().await?;
        let reply: Reply =
            serde_json::from_slice(&*reply_bytes).wrap_err("Failed to deserialize reply")?;
        match reply {
            Reply::InReach(reply) => {
                tracing::info!("Sending reply: {:?}", reply);
                inreach::reply::reply(&http_client, &reply.referral_url, &reply.message)
                    .await
                    .wrap_err("Error sending reply message")?;
                tracing::info!("Successfully sent reply!");
            }
        }

        reply_bytes.commit()?;
    }
}

#[tracing::instrument(skip(reply_receiver, shutdown_rx, http_client))]
async fn send_replies(
    reply_receiver: yaque::Receiver,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    tracing::debug!("Starting processing emails job");
    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            result.map_err(eyre::Error::from)
        }
        result = send_replies_impl(reply_receiver, http_client) => { result }
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
    let send_replies_shutdown_rx = shutdown_tx.subscribe();

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
    let reply_queue_path = data_path.join("reply");
    let (emails_sender, emails_receiver) =
        yaque::channel(emails_queue_path).wrap_err("unable to create emails queue")?;
    let (reply_sender, reply_receiver) =
        yaque::channel(reply_queue_path).wrap_err("unable to create dispatch queue")?;

    let receive_join = tokio::spawn(receive_emails(emails_sender, emails_receive_shutdown_rx));
    let process_join = tokio::spawn(process_emails(
        emails_receiver,
        reply_sender,
        emails_process_shutdown_rx,
        http_client.clone(),
    ));
    let reply_join = tokio::spawn(send_replies(
        reply_receiver,
        send_replies_shutdown_rx,
        http_client,
    ));

    // TODO: perhaps use a select here instead? What happens if failure occurs during setup before
    // running properly actually starts?
    receive_join.await??;
    process_join.await??;
    reply_join.await??;

    Ok(())
}