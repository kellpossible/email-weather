use std::{fmt::Display, time::Duration};

use crate::time;

/// A utility for performing sleeps which progressively get exponentially longer according to
/// `start * e^(i)` where `i` is the iteration, incremented each time
/// [`ExponentialBackoff::sleep()`] is called, and `start` is the starting delay provided in
/// [`ExponentialBackoff::new()`]. The delay increases until `max` duration is reached, whereupon
/// subsequent calls to [`ExponentialBackoff::sleep()`] are capped at `max` specified in
/// [`ExponentialBackoff::new()`].
pub struct ExponentialBackoff {
    start: std::time::Duration,
    max: std::time::Duration,
    at_max: bool,
    i: usize,
}

/// Error created while using [`ExponentialBackoff`].
#[derive(Debug, thiserror::Error)]
pub enum ExponentialBackoffError {
    /// `start` is not less than `max`.
    StartNotLessThanMax {
        /// Specified starting sleep duration.
        start: Duration,
        /// Specified maximum sleep duration.
        max: Duration,
    },
}

impl Display for ExponentialBackoffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExponentialBackoffError::StartNotLessThanMax { start, max } => write!(
                f,
                "Start sleep duration ({}) is not less than max sleep duration ({})",
                humantime::format_duration(*start),
                humantime::format_duration(*max)
            ),
        }
    }
}

impl ExponentialBackoff {
    /// Construct a new [`ExponentialBackoff`].
    pub fn new(start: Duration, max: Duration) -> Result<Self, ExponentialBackoffError> {
        if start >= max {
            return Err(ExponentialBackoffError::StartNotLessThanMax { start, max });
        }
        Ok(Self {
            start,
            max,
            i: 0,
            at_max: false,
        })
    }

    /// Perform one iteration of sleep, see [`ExponentialBackoff`] for a more detailed description.
    pub async fn sleep(&mut self, t: &dyn time::Port) {
        let exp_duration =
            Duration::from_secs_f64(self.start.as_secs_f64() * (self.i as f64).exp());
        let sleep_duration = Duration::min(exp_duration, self.max);
        t.async_sleep(sleep_duration).await;
        self.at_max = sleep_duration == self.max;
        self.i += 1;
    }

    /// Reset the backoff sleep duration to `start` (from [`ExponentialBackoff::new()`]).
    pub fn reset(&mut self) {
        self.i = 0;
        self.at_max = false;
    }

    /// How many iterations of [`ExponentialBackoff::sleep()`] have ben performed.
    pub fn iteration(&self) -> usize {
        self.i
    }

    /// Whether the sleep duration is currently capped at the `max` value specified in
    /// [`ExponentialBackoff::new()`].
    pub fn at_max(&self) -> bool {
        self.at_max
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use approx::relative_eq;

    use crate::time;

    use super::ExponentialBackoff;

    #[tokio::test]
    async fn test_exponential_backoff() {
        let mut backoff =
            ExponentialBackoff::new(Duration::from_millis(10), Duration::from_secs(10)).unwrap();
        assert_eq!(0, backoff.iteration());
        assert!(!backoff.at_max());
        let mut t = time::MockPort::new();

        let expected_times: &[f64] = &[
            0.01,
            0.027182818,
            0.073890561,
            0.200855369,
            0.5459815,
            1.484131591,
            4.034287935,
            10.0,
            10.0,
        ];

        for (i, et) in expected_times.into_iter().enumerate() {
            t.expect_async_sleep()
                .withf(move |d| relative_eq!(d.as_secs_f64(), et))
                .times(1)
                .returning(|_| {});
            backoff.sleep(&t).await;
            assert_eq!(i + 1, backoff.iteration());
            t.checkpoint();
        }
    }
}
