use std::{path::Path, str::FromStr};

use email_weather::{process::process_emails, receive::receive_emails, reply::send_replies};
use eyre::Context;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

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
    let process_queue_path = data_path.join("process");
    let reply_queue_path = data_path.join("reply");
    let (process_sender, process_receiver) =
        yaque::channel(process_queue_path).wrap_err("unable to create emails queue")?;
    let (reply_sender, reply_receiver) =
        yaque::channel(reply_queue_path).wrap_err("unable to create dispatch queue")?;

    let receive_join = tokio::spawn(receive_emails(process_sender, emails_receive_shutdown_rx));
    let process_join = tokio::spawn(process_emails(
        process_receiver,
        reply_sender,
        emails_process_shutdown_rx,
        http_client.clone(),
    ));
    let reply_join = tokio::spawn(send_replies(
        reply_receiver,
        send_replies_shutdown_rx,
        http_client,
    ));

    receive_join.await??;
    process_join.await?;
    reply_join.await?;

    Ok(())
}
