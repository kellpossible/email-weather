//! See [`process_emails()`].

use std::{collections::HashSet, convert::TryFrom, fmt::Display, sync::Arc};

use chrono_tz::OffsetComponents;
use eyre::Context;
use open_meteo::{Forecast, ForecastParameters, Hourly, HourlyVariable, TimeZone, WeatherCode};
use tokio::sync::Mutex;

use crate::{
    receive::Email,
    reply::{InReach, Reply},
    task::run_retry_log_errors,
};

#[derive(PartialEq, Debug)]
enum WindDirection {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl TryFrom<f32> for WindDirection {
    type Error = eyre::Error;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if (0.0 <= value && value < 45.0 / 2.0) || ((360.0 - 45.0 / 2.0) < value && value <= 360.0)
        {
            Ok(Self::N)
        } else if (45.0 / 2.0) <= value && value < (90.0 - 45.0 / 2.0) {
            Ok(Self::NE)
        } else if (90.0 - 45.0 / 2.0) <= value && value < (90.0 + 45.0 / 2.0) {
            Ok(Self::E)
        } else if (90.0 + 45.0 / 2.0) <= value && value < (180.0 - 45.0 / 2.0) {
            Ok(Self::SE)
        } else if (180.0 - 45.0 / 2.0) <= value && value < (180.0 + 45.0 / 2.0) {
            Ok(Self::S)
        } else if (180.0 + 45.0 / 2.0) <= value && value < (270.0 - 45.0 / 2.0) {
            Ok(Self::SW)
        } else if (270.0 - 45.0 / 2.0) <= value && value < (270.0 + 45.0 / 2.0) {
            Ok(Self::W)
        } else if (270.0 + 45.0 / 2.0) <= value && value < (360.0 - 45.0 / 2.0) {
            Ok(Self::NW)
        } else {
            Err(eyre::eyre!(
                "Unable to parse {} as a valid wind direction",
                value
            ))
        }
    }
}

impl Display for WindDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                WindDirection::N => "N",
                WindDirection::NE => "NE",
                WindDirection::E => "E",
                WindDirection::SE => "SE",
                WindDirection::S => "S",
                WindDirection::SW => "SW",
                WindDirection::W => "W",
                WindDirection::NW => "NW",
            }
        )
    }
}

async fn process_emails_impl(
    process_receiver: &mut yaque::Receiver,
    reply_sender: &mut yaque::Sender,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    loop {
        let received = process_receiver.recv().await?;
        let received_email: Email = serde_json::from_slice(&*received)?;

        let (latitude, longitude): (f32, f32) = match &received_email {
            Email::Inreach(email) => (email.latitude, email.longitude),
        };

        let forecast_parameters = ForecastParameters::builder()
            .latitude(latitude)
            .longitude(longitude)
            .hourly_entry(HourlyVariable::FreezingLevelHeight)
            .hourly_entry(HourlyVariable::WindSpeed10m)
            .hourly_entry(HourlyVariable::WindDirection10m)
            .hourly_entry(HourlyVariable::WeatherCode)
            .hourly_entry(HourlyVariable::Precipitation)
            .timezone(TimeZone::Auto)
            .build();

        tracing::debug!(
            "Obtaining forecast for forecast parameters {}",
            serde_json::to_string_pretty(&forecast_parameters)?
        );
        let forecast: Forecast = open_meteo::obtain_forecast(&http_client, &forecast_parameters)
            .await
            .wrap_err("Error obtaining forecast")?;
        tracing::info!("Successfully obtained forecast");

        let hourly: Hourly = forecast
            .hourly
            .ok_or_else(|| eyre::eyre!("expected hourly forecast to be present"))?;
        let time: &[chrono::NaiveDateTime] = &hourly.time;

        let freezing_level_height: &[f32] = &hourly
            .freezing_level_height
            .ok_or_else(|| eyre::eyre!("expected freezing_level_height to be present"))?;
        let wind_speed_10m: &[f32] = &hourly
            .wind_speed_10m
            .ok_or_else(|| eyre::eyre!("expected wind_speed_10m to be present"))?;
        let wind_direction_10m: &[f32] = &hourly
            .wind_direction_10m
            .ok_or_else(|| eyre::eyre!("expected wind_direction_10m to be present"))?;
        let weather_code: &[WeatherCode] = &hourly
            .weather_code
            .ok_or_else(|| eyre::eyre!("expected weather_code to be present"))?;
        let precipitation: &[f32] = &hourly
            .precipitation
            .ok_or_else(|| eyre::eyre!("expected precipitation to be present"))?;

        if [
            time.len(),
            freezing_level_height.len(),
            wind_speed_10m.len(),
            wind_direction_10m.len(),
            weather_code.len(),
            precipitation.len(),
        ]
        .into_iter()
        .collect::<HashSet<usize>>()
        .len()
            != 1
        {
            eyre::bail!("forecast hourly array lengths don't match")
        }

        let mut messages: Vec<String> = Vec::new();
        let utc_now: chrono::NaiveDateTime = chrono::Utc::now().naive_utc();
        let offset = chrono::TimeZone::offset_from_utc_datetime(&forecast.timezone, &utc_now);
        let current_local_time: chrono::NaiveDateTime =
            chrono::TimeZone::from_utc_datetime(&forecast.timezone, &utc_now).naive_local();
        tracing::debug!("current local time: {}", current_local_time);
        let total_offset: chrono::Duration = offset.base_utc_offset() + offset.dst_offset();

        if total_offset.num_seconds() != forecast.utc_offset_seconds {
            tracing::warn!(
                "Reported timezone offsets don't match {} != {}",
                total_offset.num_seconds(),
                forecast.utc_offset_seconds
            );
        }

        let formatted_offset: String = if total_offset.is_zero() {
            "GMT".to_string()
        } else {
            let formatted_duration = format!(
                "{:02}:{:02}",
                total_offset.num_hours(),
                total_offset.num_minutes() % 60
            );
            if total_offset > chrono::Duration::zero() {
                format!("+{}", formatted_duration)
            } else {
                format!("-{}", formatted_duration)
            }
        };

        messages.push(format!("Tz{} E{:.0}", formatted_offset, forecast.elevation));

        // Skip times that are after the current local time.
        let start_i: usize = time.iter().enumerate().fold(0, |acc, (i, local_time)| {
            if current_local_time > *local_time {
                usize::min(i + 1, time.len() - 1)
            } else {
                acc
            }
        });

        let mut i = start_i;
        let mut acc_precipitation: f32 = 0.0;
        while i <= usize::min(time.len() - 1, i + 48) {
            acc_precipitation += precipitation[i];
            if (i - start_i) % 6 == 0 {
                let formatted_time = time[i].format("%dT%H");
                messages.push(format!(
                    "{} C{:.0} F{:.0} W{:.0}@{} P{:.0}",
                    formatted_time,
                    weather_code[i] as u8,
                    freezing_level_height[i].round(),
                    (wind_speed_10m[i] / 10.0).round(),
                    (wind_direction_10m[i] / 10.0).round(),
                    acc_precipitation.round(),
                ));
                acc_precipitation = 0.0;
            }
            i += 1;
        }

        tracing::info!("Sending reply for email {:?}", received_email);

        let mut message: String = String::new();
        for (i, m) in messages.into_iter().enumerate() {
            if message.len() + m.len() > 160 {
                break;
            }

            if i > 0 {
                message.push('\n');
            }
            message.push_str(&m);
        }
        tracing::info!("message (len: {}):\n{}", message.len(), message);

        let reply = match received_email {
            Email::Inreach(email) => Reply::InReach(InReach {
                referral_url: email.referral_url,
                message,
            }),
        };

        let reply_bytes = serde_json::to_vec(&reply).wrap_err("Failed to serialize reply")?;
        reply_sender.send(&reply_bytes).await?;

        received.commit()?;
    }
}

/// This function spawns a task to process an incoming email, create a customized forecast that it
/// requested, and dispatch a reply.
#[tracing::instrument(skip(process_receiver, reply_sender, shutdown_rx, http_client))]
pub async fn process_emails(
    process_receiver: yaque::Receiver,
    reply_sender: yaque::Sender,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
) {
    tracing::debug!("Starting processing emails job");
    let queues = Arc::new(Mutex::new((process_receiver, reply_sender)));
    run_retry_log_errors(
        move || {
            let queues = queues.clone();
            let http_client = http_client.clone();
            async move {
                let (process_receiver, reply_sender) = &mut *queues.lock().await;
                process_emails_impl(process_receiver, reply_sender, http_client).await
            }
        },
        shutdown_rx,
    )
    .await;
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use super::WindDirection;

    #[test]
    fn test_wind_direction_from_float() {
        assert_eq!(WindDirection::N, WindDirection::try_from(350.0).unwrap());
        assert_eq!(WindDirection::N, WindDirection::try_from(0.0).unwrap());
        assert_eq!(WindDirection::N, WindDirection::try_from(10.0).unwrap());
        assert_eq!(WindDirection::NE, WindDirection::try_from(30.0).unwrap());
        assert_eq!(WindDirection::NE, WindDirection::try_from(45.0).unwrap());
        assert_eq!(WindDirection::NE, WindDirection::try_from(50.0).unwrap());
        assert_eq!(WindDirection::E, WindDirection::try_from(80.0).unwrap());
        assert_eq!(WindDirection::E, WindDirection::try_from(90.0).unwrap());
        assert_eq!(WindDirection::E, WindDirection::try_from(100.0).unwrap());
        assert_eq!(WindDirection::SE, WindDirection::try_from(120.0).unwrap());
        assert_eq!(WindDirection::SE, WindDirection::try_from(135.0).unwrap());
        assert_eq!(WindDirection::SE, WindDirection::try_from(140.0).unwrap());
        assert_eq!(WindDirection::S, WindDirection::try_from(170.0).unwrap());
        assert_eq!(WindDirection::S, WindDirection::try_from(180.0).unwrap());
        assert_eq!(WindDirection::S, WindDirection::try_from(190.0).unwrap());
        assert_eq!(WindDirection::SW, WindDirection::try_from(210.0).unwrap());
        assert_eq!(WindDirection::SW, WindDirection::try_from(225.0).unwrap());
        assert_eq!(WindDirection::SW, WindDirection::try_from(235.0).unwrap());
        assert_eq!(WindDirection::W, WindDirection::try_from(260.0).unwrap());
        assert_eq!(WindDirection::W, WindDirection::try_from(270.0).unwrap());
        assert_eq!(WindDirection::W, WindDirection::try_from(280.0).unwrap());
        assert_eq!(WindDirection::NW, WindDirection::try_from(310.0).unwrap());
        assert_eq!(WindDirection::NW, WindDirection::try_from(315.0).unwrap());
        assert_eq!(WindDirection::NW, WindDirection::try_from(325.0).unwrap());
    }
}
