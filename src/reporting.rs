//! Utilities for logging and automated bug reporting.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use eyre::Context;
use futures::{Stream, TryStreamExt};
use html_builder::Html5;
use reqwest::StatusCode;
use tokio_stream::wrappers::ReadDirStream;
use tracing_appender::{
    non_blocking::{NonBlockingBuilder, WorkerGuard},
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

use crate::fs;

/// Options for writing to log file.
#[derive(Clone)]
struct LogFileOptions {
    /// The directory to store the log files in.
    /// Will be created if it doesn't yet exist.
    pub directory: PathBuf,
    /// How often to rotate the log files
    pub rotation: Rotation,
}

#[derive(Clone)]
struct ReportWriterOptions {
    /// Whether to write to stdout.
    stdout: bool,
    /// Whether to write to stdout.
    stderr: bool,
    /// Whether to write to the log file.
    log_file: Option<LogFileOptions>,
}

/// Implements [std::io::Write] to write `tracing`/panic messages to
/// multiple outputs.
struct ReportWriter {
    stdout: bool,
    stderr: bool,
    log_file_writer: Option<RollingFileAppender>,
}

impl ReportWriter {
    /// Try creating a new [TracingWriter].
    fn try_new(options: &ReportWriterOptions) -> eyre::Result<Self> {
        let log_file_writer = if let Some(log_file_options) = &options.log_file {
            if !log_file_options.directory.exists() {
                fs::create_dir_if_not_exists(&log_file_options.directory)
                    .wrap_err("Unable to create log file directory")?;
            }
            let appender = RollingFileAppender::new(
                log_file_options.rotation.clone(),
                log_file_options.directory.clone(),
                "email-weather.log",
            );

            Some(appender)
        } else {
            None
        };

        Ok(Self {
            stdout: options.stdout,
            stderr: options.stderr,
            log_file_writer,
        })
    }
}

impl std::io::Write for ReportWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut retval: usize = buf.len();

        if self.stdout || self.stderr {
            let out_str = String::from_utf8_lossy(buf);
            if self.stdout {
                print!("{}", out_str);
            }

            if self.stderr {
                eprint!("{}", out_str);
            }
        }

        if let Some(writer) = &mut self.log_file_writer {
            retval = usize::min(retval, writer.write(buf)?);
        }

        Ok(retval)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.stdout {
            std::io::stdout().flush()?;
        }

        if self.stderr {
            std::io::stderr().flush()?;
        }

        if let Some(writer) = &mut self.log_file_writer {
            writer.flush()?;
        }

        Ok(())
    }
}

impl Drop for ReportWriter {
    fn drop(&mut self) {
        use std::io::Write;
        let _ = self.write("\n".as_bytes());
    }
}

pub struct ReportingGuard {
    _sentry: Option<sentry::ClientInitGuard>,
    _writer: WorkerGuard,
}

pub struct ReportingOptions {
    pub data_dir: PathBuf,
    pub log_rotation: Rotation,
}

impl ReportingOptions {
    fn log_dir(&self) -> PathBuf {
        self.data_dir.join("log")
    }
}

pub fn setup_reporting(options: &ReportingOptions) -> eyre::Result<ReportingGuard> {
    let sentry = if let Ok(sentry_dsn) = std::env::var("SENTRY_DSN") {
        Some(sentry::init(sentry::ClientOptions {
            dsn: Some(sentry_dsn.parse().unwrap()),
            release: sentry::release_name!(),
            // TODO: set this lower for production
            traces_sample_rate: 1.0,
            ..sentry::ClientOptions::default()
        }))
    } else {
        None
    };

    let log_dir = options.log_dir();

    let report_writer = ReportWriter::try_new(&ReportWriterOptions {
        stdout: true,
        stderr: false,
        log_file: Some(LogFileOptions {
            directory: log_dir,
            rotation: options.log_rotation.clone(),
        }),
    })?;

    let (non_blocking_writer, report_writer_guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(1000)
        .lossy(false)
        .finish(report_writer);

    let rust_log_env: String =
        std::env::var("RUST_LOG").unwrap_or_else(|_| "warn,email_weather=debug".to_string());

    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(non_blocking_writer);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(tracing_subscriber::EnvFilter::from_str(rust_log_env.as_str()).unwrap_or_default())
        .with(tracing_error::ErrorLayer::default())
        .with(sentry.as_ref().map(|_| sentry_tracing::layer()))
        .init();

    let (eyre_panic_hook, eyre_hook) = color_eyre::config::HookBuilder::new().into_hooks();
    let eyre_panic_hook = eyre_panic_hook.into_panic_hook();
    eyre::set_hook(eyre_hook.into_eyre_hook())?;

    let sentry_enabled: bool = sentry.is_some();
    std::panic::set_hook(Box::new(move |panic_info| {
        eyre_panic_hook(panic_info);
        // if sentry_enabled {
        //     sentry::integrations::panic::panic_handler(panic_info);
        // }
    }));

    if sentry_enabled {
        tracing::info!("sentry.io reporting is enabled");
    }

    Ok(ReportingGuard {
        _sentry: sentry,
        _writer: report_writer_guard,
    })
}

/// Serve the application logs.
#[tracing::instrument(skip(shutdown_rx, options))]
pub async fn serve_logs(
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    options: &'static ReportingOptions,
) {
    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            let result = result.wrap_err("Error receiving shutdown message");
            if let Err(error) = &result {
                tracing::error!("{:?}", error);
            }
        }
        _ = serve_logs_impl(options) => {}
    }
}

async fn serve_logs_impl(options: &'static ReportingOptions) {
    let log_dir_1 = options.log_dir();
    let log_dir_2 = options.log_dir();
    // build our application with a route
    let app = Router::new()
        .route(
            "/log/",
            get(move || async move {
                match serve_logs_index(&log_dir_1).await {
                    Ok(html) => axum::response::Result::Ok(html),
                    Err(error) => {
                        tracing::error!("{:?}", error);
                        axum::response::Result::Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }),
        )
        .route(
            "/log/:filename",
            get(move |filename| async move { serve_log(filename, &log_dir_2).await }),
        );

    let addr: SocketAddr = if let Ok(var) = std::env::var("LISTEN_ADDR") {
        var.parse()
            .expect("Error parsing LISTEN_ADDR environment variable")
    } else {
        SocketAddr::from(([127, 0, 0, 1], 3000))
    };

    tracing::info!("Serving logs at http://{}/log", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap()
}

#[derive(Debug, thiserror::Error)]
enum ServeLogError {
    #[error("Log file not found")]
    NotFound,
    #[error("Internal server error")]
    InternalServerError(#[from] eyre::Error),
}

impl IntoResponse for ServeLogError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ServeLogError::NotFound => StatusCode::NOT_FOUND.into_response(),
            ServeLogError::InternalServerError(error) => {
                let mut response = format!("{}", error).into_response();
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                response
            }
        }
    }
}

async fn serve_log(
    axum::extract::Path(filename): axum::extract::Path<String>,
    log_dir: &Path,
) -> axum::response::Result<Html<String>, ServeLogError> {
    use std::fmt::Write;
    let find_file = files_stream(log_dir)
        .await
        .wrap_err("Error creating files stream in log directory")?
        .try_filter(|path| {
            futures::future::ready(
                if let Some(path_str) = path.file_name().and_then(|path| path.to_str()) {
                    path_str == &*filename
                } else {
                    false
                },
            )
        });
    futures::pin_mut!(find_file);

    let file_path = find_file
        .try_next()
        .await
        .wrap_err("Error finding log file")?
        .ok_or(ServeLogError::NotFound)?;

    let log_file_contents = tokio::fs::read_to_string(file_path)
        .await
        .wrap_err("Error reading log file")?;

    let mut buf = html_builder::Buffer::new();
    let mut html = buf.html();
    let mut head = html.head();
    let mut title = head.title();
    write!(title, "log {}", filename).unwrap();

    let mut style = head.style();
    write!(
        style,
        r#"body {{
        font-family: monospace;
    }}"#
    )
    .unwrap();

    let mut body = html.body();

    let formatted_html = tokio::task::spawn_blocking(move || {
        log_file_contents
            .lines()
            .map(|line| {
                let mut formatted_line = ansi_to_html::convert_escaped(line)?;
                formatted_line.push_str("<br>");
                Ok(formatted_line)
            })
            .collect::<Result<String, ansi_to_html::Error>>()
    })
    .await
    .map_err(eyre::Error::from)?
    .wrap_err("Error converting log file to html")?;

    write!(body, "{}", formatted_html).unwrap();

    Ok(Html::from(buf.finish()))
}

async fn files_stream(
    log_dir: &Path,
) -> tokio::io::Result<impl Stream<Item = tokio::io::Result<PathBuf>>> {
    Ok(
        ReadDirStream::new(tokio::fs::read_dir(log_dir).await?).try_filter_map(
            |entry| async move {
                let file_type = entry.file_type().await?;
                Ok(if file_type.is_file() {
                    Some(entry.path())
                } else {
                    None
                })
            },
        ),
    )
}

async fn serve_logs_index(log_dir: &Path) -> eyre::Result<Html<String>> {
    use std::fmt::Write;
    let mut buf = html_builder::Buffer::new();
    let mut html = buf.html();
    write!(html.head().title(), "email-weather logs")?;
    let mut body = html.body();
    let mut ul = body.ul();

    let files_stream = files_stream(log_dir).await?;
    futures::pin_mut!(files_stream);

    while let Some(path) = files_stream.try_next().await? {
        let mut li = ul.li();
        let filename = path
            .file_name()
            .ok_or_else(|| eyre::eyre!("Expected path to have a filename"))?
            .to_str()
            .ok_or_else(|| eyre::eyre!("Unable to convert filename to utf-8 string"))?;
        let href_attr = format!(r#"href="/log/{}""#, filename);
        let mut a = li.a().attr(&href_attr);
        write!(a, "{}", filename)?;
    }

    Ok(Html::from(buf.finish()))
}
