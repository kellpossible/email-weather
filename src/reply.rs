//! See [`send_replies()`].

use std::sync::Arc;

use eyre::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{inreach, task::run_retry_log_errors};

#[derive(Serialize, Deserialize, Debug)]
pub struct InReachReply {
    pub referral_url: url::Url,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Reply {
    InReach(InReachReply),
}

async fn send_replies_impl(
    reply_receiver: &mut yaque::Receiver,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    loop {
        let reply_bytes = reply_receiver.recv().await?;
        let reply: Reply =
            serde_json::from_slice(&*reply_bytes).wrap_err("Failed to deserialize reply")?;
        match reply {
            Reply::InReach(reply) => {
                tracing::info!("Sending reply: {:?}", reply);
                // TODO: re-enable
                // inreach::reply::reply(&http_client, &reply.referral_url, &reply.message)
                //     .await
                //     .wrap_err("Error sending reply message")?;
                tracing::info!("Successfully sent reply!");
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
    .await
}
