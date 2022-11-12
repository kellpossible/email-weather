//! See [`send_replies()`].

use std::{sync::Arc, time::Duration};

use eyre::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{email, inreach, retry::ExponentialBackoff, task::run_retry_log_errors, time};

/// A reply to an inreach device.
#[derive(Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct InReach {
    /// The url used to send the reply via the web interface (that was supplied in the original
    /// message from the device).
    pub referral_url: url::Url,
    /// The message to send in the reply.
    pub message: String,
}

/// Reply to a standard plain text email.
#[derive(Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Plain {
    /// The message to send in the reply.
    pub message: String,
    /// Who the reply is addressed to.
    pub to: email::Account,
    /// Message id that this is in reply to.
    pub in_reply_to_message_id: Option<String>,
}

/// A reply message.
#[derive(Eq, PartialEq, Serialize, Deserialize, Debug)]
pub enum Reply {
    /// See [`InReach`].
    InReach(InReach),
    /// See [`Plain`].
    Plain(Plain),
}

async fn send_reply(
    reply: &Reply,
    http_client: &reqwest::Client,
    email_account: &email::Account,
) -> eyre::Result<()> {
    tracing::info!("Sending reply: {:?}", reply);

    match reply {
        Reply::InReach(reply) => {
            // TODO refactor move Reply into inreach::reply
            inreach::reply::reply(http_client, &reply.referral_url, &reply.message)
                .await
                .wrap_err("Error sending reply message")?;
        }
        Reply::Plain(reply) => {
            // TODO send plain reply
            //https://docs.rs/lettre/latest/lettre/transport/smtp/authentication/enum.Mechanism.html
            //XOAUTH2
            // lettre::Message::builder()
            //     .from(lettre::message::Mailbox::from(reply.to)
            let builder = lettre::Message::builder()
                .from(email_account.clone().into())
                .to(reply.to.clone().into());

            let builder = if let Some(id) = &reply.in_reply_to_message_id {
                builder.in_reply_to(id.clone())
            } else {
                builder
            };

            let message: lettre::Message = builder.body(reply.message.clone())?;

            tracing::debug!("Replying: {:?}", message);
        }
    }
    tracing::info!("Successfully sent reply!");

    Ok(())
}

/// Number of attempts to retry sending a message before discarding it.
const RETRY_ATTEMPTS: usize = 5;

async fn send_replies_impl(
    reply_receiver: &mut yaque::Receiver,
    http_client: reqwest::Client,
    email_account: &email::Account,
    time: &dyn time::Port,
) -> eyre::Result<()> {
    loop {
        let reply_bytes = reply_receiver.recv().await?;
        let reply: Reply =
            serde_json::from_slice(&*reply_bytes).wrap_err("Failed to deserialize reply")?;

        let mut backoff =
            ExponentialBackoff::new(Duration::from_secs(5), Duration::from_secs(60 * 10))
                .expect("Invalid backoff");
        'retry: loop {
            match send_reply(&reply, &http_client, email_account).await {
                Ok(_) => break 'retry,
                Err(error) => {
                    tracing::error!("{:?}", error);
                    if backoff.iteration() < RETRY_ATTEMPTS {
                        backoff.sleep(time).await;
                        tracing::warn!("Retrying {}/{}...", backoff.iteration(), RETRY_ATTEMPTS);
                        continue;
                    } else {
                        let reply_json = serde_json::to_string(&reply)?;
                        tracing::error!("Max retries exceeded, discarding reply\n{}", reply_json);
                        break;
                    }
                }
            }
        }
        reply_bytes.commit()?;
    }
}

/// This function spawns a task to send replies to received emails using the results of
/// [`crate::processing`].
#[tracing::instrument(skip_all)]
pub async fn send_replies(
    reply_receiver: yaque::Receiver,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
    email_account: &email::Account,
    time: &dyn time::Port,
) {
    let reply_receiver = Arc::new(Mutex::new(reply_receiver));
    tracing::debug!("Starting send replies job");
    run_retry_log_errors(
        move || {
            let http_client = http_client.clone();
            let reply_receiver = reply_receiver.clone();
            async move {
                let mut reply_receiver = reply_receiver.lock().await;
                send_replies_impl(
                    &mut reply_receiver,
                    http_client.clone(),
                    email_account,
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
