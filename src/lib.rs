//! email-weather library crate

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

pub mod email;
pub mod fs;
pub mod gis;
pub mod inreach;
pub mod oauth2;
pub mod options;
pub mod plain;
pub mod process;
pub mod receive;
pub mod reply;
pub mod reporting;
pub mod request;
pub mod retry;
pub mod secrets;
pub mod serve_http;
pub mod smtp;
pub mod task;
pub mod time;
