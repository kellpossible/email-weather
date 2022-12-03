//! Abstraction over system provided time, as part of the hexagonal architecture.

use async_trait::async_trait;
use chrono::Utc;

/// Interface for accessing system provided time functionality.
/// See [`Gateway`] for implementation.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Port: Send + Sync {
    /// Wrapper over [`tokio::time::sleep()`].
    async fn async_sleep(&self, duration: std::time::Duration);
    /// Wrapper over [`std::thread::sleep()`].
    fn sleep(&self, duration: std::time::Duration);
    /// Obtain the current time in UTC.
    fn utc_now(&self) -> chrono::DateTime<Utc>;
}

/// Implementation of [`Port`].
pub struct Gateway;

#[async_trait]
impl Port for Gateway {
    async fn async_sleep(&self, duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }

    fn sleep(&self, duration: std::time::Duration) {
        std::thread::sleep(duration);
    }

    fn utc_now(&self) -> chrono::DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod test {
    use super::{Gateway, Port};
    fn gateway_is_send_sync<P: Port + Send + Sync>(_: P) {}

    #[test]
    fn test_gateway_is_send_sync() {
        gateway_is_send_sync(Gateway);
    }
}
