//! See [`process_emails()`].

use std::{collections::HashSet, sync::Arc};

use chrono_tz::OffsetComponents;
use eyre::Context;
use open_meteo::{Forecast, ForecastParameters, Hourly, HourlyVariable, TimeZone, WeatherCode};
use tokio::sync::Mutex;

use crate::{
    receive::Email,
    reply::{InReach, Reply},
    task::run_retry_log_errors,
};

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
            // .hourly_entry(HourlyVariable::Temperature2m)
            .hourly_entry(HourlyVariable::CloudCover)
            .hourly_entry(HourlyVariable::FreezingLevelHeight)
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

        // let temperature: &[f32] = &hourly
        //     .temperature_2m
        //     .ok_or_else(|| eyre::eyre!("expected temperature_2m to be present"))?;
        let weather_code: &[WeatherCode] = &hourly
            .weather_code
            .ok_or_else(|| eyre::eyre!("expected weather_code to be present"))?;
        let precipitation: &[f32] = &hourly
            .precipitation
            .ok_or_else(|| eyre::eyre!("expected precipitation to be present"))?;
        let cloud_cover: &[f32] = &hourly
            .cloud_cover
            .ok_or_else(|| eyre::eyre!("expected cloud_cover to be present"))?;
        let freezing_level_height: &[f32] = &hourly
            .freezing_level_height
            .ok_or_else(|| eyre::eyre!("expected freezing_level_height to be present"))?;

        if [
            time.len(),
            // temperature.len(),
            precipitation.len(),
            freezing_level_height.len(),
            cloud_cover.len(),
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
                    "{} F{:.0} C{:.0} W{} P{:.0}",
                    formatted_time,
                    freezing_level_height[i],
                    cloud_cover[i],
                    weather_code[i] as u8,
                    acc_precipitation,
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
