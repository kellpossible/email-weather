use std::net::SocketAddr;

use axum::{http::HeaderValue, response::IntoResponse, Router};
use eyre::Context;
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc;
use tower_http::auth::AuthorizeRequest;

use crate::{oauth2::RedirectParameters, reporting};

/// Options for running this application's http server.
pub struct Options {
    /// Options relating to reporting/logging.
    pub reporting: &'static reporting::Options,
    /// `admin` user's password hash using `bcrypt`. See [`MyBasicAuth`].
    pub admin_password_hash: Option<&'static SecretString>,
    /// A channel to send authorization codes received.
    pub oauth_redirect_tx: mpsc::Sender<RedirectParameters>,
}

// TODO: turn this into a generic web server, and provide a channel for transmitting the
// result of OAUTH2 redirect back to the InstalledFlow.
/// Run this service's http server.
#[tracing::instrument(skip(shutdown_rx, options))]
pub async fn serve_http(mut shutdown_rx: tokio::sync::broadcast::Receiver<()>, options: Options) {
    tokio::select! {
        result = shutdown_rx.recv() => {
            tracing::debug!("Received shutdown broadcast");
            let result = result.wrap_err("Error receiving shutdown message");
            if let Err(error) = &result {
                tracing::error!("{:?}", error);
            }
        }
        _ = serve_http_impl(options) => {}
    }
}

/// Basic authentication for accessing logs.
#[derive(Clone, Copy)]
pub struct MyBasicAuth {
    /// `admin` user password hash, hashed using bcrypt.
    pub admin_password_hash: &'static SecretString,
}

impl<B> AuthorizeRequest<B> for MyBasicAuth {
    type ResponseBody = http_body::combinators::UnsyncBoxBody<axum::body::Bytes, axum::Error>;

    fn authorize(
        &mut self,
        request: &mut axum::http::Request<B>,
    ) -> Result<(), axum::http::Response<Self::ResponseBody>> {
        if check_auth(request, self.admin_password_hash) {
            Ok(())
        } else {
            let unauthorized_response = axum::http::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(
                    "WWW-Authenticate",
                    r#"Basic realm="User Visible Realm", charset="UTF-8""#,
                )
                .body(axum::body::Body::empty())
                .unwrap();

            Err(unauthorized_response.into_response())
        }
    }
}

struct BasicCredentials {
    username: String,
    password: SecretString,
}

fn parse_auth_header_credentials(header: &HeaderValue) -> Option<BasicCredentials> {
    let header_str: &str = header.to_str().ok()?;
    let credentials_base64: &str = header_str.split_once("Basic ")?.1;
    let credentials = String::from_utf8(base64::decode(credentials_base64).ok()?).ok()?;
    let (username, password) = credentials.split_once(':')?;
    Some(BasicCredentials {
        username: username.to_string(),
        password: SecretString::new(password.to_string()),
    })
}

/// Check authorization for a request. Returns `true` if the request is authorized, returns `false` otherwise. Uses Basic http authentication and bcrypt for password hashing.
fn check_auth<B>(
    request: &axum::http::Request<B>,
    admin_password_hash: &'static SecretString,
) -> bool {
    let credentials: BasicCredentials =
        if let Some(auth_header) = request.headers().get("Authorization") {
            if let Some(credentials) = parse_auth_header_credentials(auth_header) {
                credentials
            } else {
                return false;
            }
        } else {
            return false;
        };

    let password_match = bcrypt::verify(
        credentials.password.expose_secret(),
        admin_password_hash.expose_secret(),
    )
    .unwrap_or(false);
    credentials.username == "admin" && password_match
}

async fn serve_http_impl(options: Options) {
    let app = Router::new().nest(
        "/oauth2/",
        crate::oauth2::redirect_server(options.oauth_redirect_tx),
    );

    let addr: SocketAddr = if let Ok(var) = std::env::var("LISTEN_ADDR") {
        var.parse()
            .expect("Error parsing LISTEN_ADDR environment variable")
    } else {
        SocketAddr::from(([127, 0, 0, 1], 3000))
    };

    let app = if let Some(admin_password_hash) = &options.admin_password_hash {
        tracing::info!("Serving logs at http://{}/logs", addr);
        app.nest(
            "/logs/",
            reporting::serve_logs(options.reporting, admin_password_hash),
        )
    } else {
        tracing::info!("No admin password secret provided, logs will not be served");
        app
    };

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
