use std::{path::PathBuf, str::FromStr};

use email_weather::{
    fs,
    process::process_emails,
    receive::{receive_emails, ImapSecrets},
    reply::send_replies,
};
use eyre::Context;
use tokio::signal::unix::SignalKind;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let rust_log_env: String = std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string());
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

    let ctrl_c_shutdown_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen to ctrl-c or SIGINT event");
        tracing::warn!("ctrl-c or SIGINT event detected, broadcasting shutdown");
        ctrl_c_shutdown_tx 
            .send(())
            .expect("failed to send shutdown broadcast");
    });

    let sigterm_shutdown_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::unix::signal(SignalKind::terminate())
            .expect("failed to create SIGTERM signal listener")
            .recv()
            .await
            .expect("failed to listen to SIGTERM signal");
        tracing::warn!("SIGTERM signal detected, broadcasting shutdown");
        sigterm_shutdown_tx 
            .send(())
            .expect("failed to send shutdown broadcast");
    });

    let data_dir = std::env::var("DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data"));

    fs::create_dir_if_not_exists(&data_dir)
        .wrap_err_with(|| format!("Unable to create data directory {:?}", data_dir))?;

    let secrets_dir = std::env::var("SECRETS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("secrets"));

    fs::create_dir_if_not_exists(&secrets_dir)
        .wrap_err_with(|| format!("Unable to create secrets directory {:?}", secrets_dir))?;

    // install_secrets().wrap_err("Error while installing secrets")?;

    let process_queue_path = data_dir.join("process");
    let reply_queue_path = data_dir.join("reply");
    let (process_sender, process_receiver) = yaque::channel(&process_queue_path)
        .wrap_err_with(|| format!("Unable to create process queue at {:?}", process_queue_path))?;
    let (reply_sender, reply_receiver) = yaque::channel(&reply_queue_path)
        .wrap_err_with(|| format!("Unable to create reply queue at {:?}", reply_queue_path))?;

    let imap_secrets = ImapSecrets::initialize(&secrets_dir)
        .await
        .wrap_err("Error while initializing imap secrets")?;

    let receive_join = tokio::spawn(receive_emails(
        process_sender,
        emails_receive_shutdown_rx,
        imap_secrets,
    ));
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
