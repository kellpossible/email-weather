//! Utilitis for executing/spawning async tasks.

use std::time::Duration;

use eyre::Context;
use futures::Future;

use crate::{retry::ExponentialBackoff, time};

/// In a loop, runs a future created by `run`, logs an error if it occurs. In parallel using a
/// `select!`, it listens to `shutdown_rx` and cancels the loop if a shutdown message has been
/// broadcast.
pub async fn run_retry_log_errors<F, FUT>(
    run: F,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    time: &dyn time::Port,
) where
    F: Fn() -> FUT,
    FUT: Future<Output = eyre::Result<()>>,
{
    let run_loop = async move {
        let mut backoff =
            ExponentialBackoff::new(Duration::from_secs(10), Duration::from_secs(60 * 10))
                .expect("Invalid backoff");
        loop {
            if let Err(error) = run().await {
                tracing::error!("{:?}", error);
                backoff.sleep(time).await;
                tracing::warn!("Retrying...");
            } else {
                backoff.reset();
            }
        }
    };

    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            let result = result.wrap_err("Error receiving shutdown message");
            if let Err(error) = &result {
                tracing::error!("{:?}", error);
            }
        }
        _ = run_loop => {}
    }
}
