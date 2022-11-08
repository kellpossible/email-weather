use email_weather::{
    fs,
    oauth2::RedirectParameters,
    options::{self, Options},
    process::process_emails,
    receive::receive_emails,
    reply::send_replies,
    reporting,
    secrets::Secrets,
    serve_http, time,
};
use eyre::Context;
use tokio::{
    signal::unix::SignalKind,
    sync::{broadcast, mpsc},
};
use tracing_appender::rolling::Rotation;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    reporting::setup_error_hooks()?;
    let options_init = options::Options::initialize().await;
    let options: &'static Options = options_init
        .result
        .map(|options| Box::leak(Box::new(options)))
        .map_err(|error| {
            options_init.logs.print();
            error
        })?;

    fs::create_dir_if_not_exists(&options.data_dir)
        .wrap_err_with(|| format!("Unable to create data directory {:?}", options.data_dir))
        .map_err(|error| {
            options_init.logs.print();
            error
        })?;

    let reporting_options: &'static reporting::Options = Box::leak(Box::new(reporting::Options {
        data_dir: options.data_dir.clone(),
        log_rotation: Rotation::DAILY,
    }));

    let _reporting_guard = reporting::setup_logging(reporting_options).map_err(|error| {
        options_init.logs.print();
        error
    })?;

    options_init.logs.present();

    fs::create_dir_if_not_exists(&options.secrets_dir).wrap_err_with(|| {
        format!(
            "Unable to create secrets directory {:?}",
            options.secrets_dir
        )
    })?;

    let time: &'static time::Gateway = Box::leak(Box::new(time::Gateway));

    let secrets = Box::leak(Box::new(
        Secrets::initialize(&options.secrets_dir)
            .await
            .wrap_err("Error while initializing secrets")?,
    ));

    let http_client = reqwest::Client::new();

    let (shutdown_tx, emails_receive_shutdown_rx) = broadcast::channel::<()>(1);
    let emails_process_shutdown_rx = shutdown_tx.subscribe();
    let send_replies_shutdown_rx = shutdown_tx.subscribe();
    let serve_http_shutdown_rx = shutdown_tx.subscribe();

    let (oauth_redirect_tx, oauth_redirect_rx) = mpsc::channel::<RedirectParameters>(1);

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

    let process_queue_path = options.data_dir.join("process");
    let reply_queue_path = options.data_dir.join("reply");
    let (process_sender, process_receiver) = yaque::channel(&process_queue_path)
        .wrap_err_with(|| format!("Unable to create process queue at {:?}", process_queue_path))?;
    let (reply_sender, reply_receiver) = yaque::channel(&reply_queue_path)
        .wrap_err_with(|| format!("Unable to create reply queue at {:?}", reply_queue_path))?;

    let receive_join = tokio::spawn(receive_emails(
        process_sender,
        emails_receive_shutdown_rx,
        oauth_redirect_rx,
        &secrets.imap_secrets,
        &options.base_url,
        options.email_account.email_str(),
        time,
    ));
    let process_join = tokio::spawn(process_emails(
        process_receiver,
        reply_sender,
        emails_process_shutdown_rx,
        http_client.clone(),
        time,
    ));
    let reply_join = tokio::spawn(send_replies(
        reply_receiver,
        send_replies_shutdown_rx,
        http_client,
        &options.email_account,
        time,
    ));

    let serve_http_options = serve_http::Options {
        reporting: reporting_options,
        admin_password_hash: secrets.admin_password_hash.as_ref(),
        oauth_redirect_tx,
        base_url: options.base_url.clone(),
        listen_address: options.listen_address,
    };
    let serve_http_join = tokio::spawn(serve_http::serve_http(
        serve_http_shutdown_rx,
        serve_http_options,
    ));

    serve_http_join.await?;
    receive_join.await?;
    process_join.await?;
    reply_join.await?;

    Ok(())
}
