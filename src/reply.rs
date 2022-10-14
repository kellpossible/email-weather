//! See [`send_replies()`].

use std::{sync::Arc, time::Duration};

use eyre::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{inreach, task::run_retry_log_errors};

/// A reply to an inreach device.
#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct InReach {
    /// The url used to send the reply via the web interface (that was supplied in the original
    /// message from the device).
    pub referral_url: url::Url,
    /// The message to send in the reply.
    pub message: String,
}

/// A reply message.
#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub enum Reply {
    /// Reply to an inreach device.
    InReach(InReach),
}

async fn send_reply(reply: &Reply, http_client: &reqwest::Client) -> eyre::Result<()> {
    match reply {
        Reply::InReach(reply) => {
            tracing::info!("Sending reply: {:?}", reply);
            inreach::reply::reply(http_client, &reply.referral_url, &reply.message)
                .await
                .wrap_err("Error sending reply message")?;
            tracing::info!("Successfully sent reply!");
        }
    }

    Ok(())
}

/// Number of attempts to retry sending a message before discarding it.
const RETRY_ATTEMPTS: usize = 5;

async fn send_replies_impl(
    reply_receiver: &mut yaque::Receiver,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    loop {
        let reply_bytes = reply_receiver.recv().await?;
        let reply: Reply =
            serde_json::from_slice(&*reply_bytes).wrap_err("Failed to deserialize reply")?;

        let mut retry_count: usize = 0;
        'retry: loop {
            match send_reply(&reply, &http_client).await {
                Ok(_) => break 'retry,
                Err(error) => {
                    tracing::error!("{:?}", error);
                    if retry_count < RETRY_ATTEMPTS {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        retry_count += 1;
                        tracing::warn!("Retrying {}/{}...", retry_count, RETRY_ATTEMPTS);
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
#[tracing::instrument(skip(reply_receiver, shutdown_rx, http_client))]
pub async fn send_replies(
    reply_receiver: yaque::Receiver,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
) {
    let reply_receiver = Arc::new(Mutex::new(reply_receiver));
    tracing::debug!("Starting send replies job");
    run_retry_log_errors(
        move || {
            let http_client = http_client.clone();
            let reply_receiver = reply_receiver.clone();
            async move {
                let mut reply_receiver = reply_receiver.lock().await;
                send_replies_impl(&mut reply_receiver, http_client.clone()).await
            }
        },
        shutdown_rx,
    )
    .await;
}
