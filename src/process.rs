//! See [`process_emails()`].

use std::{
    borrow::Cow,
    collections::HashSet,
    convert::TryFrom,
    fmt::{Display, Write},
    sync::Arc,
};

use chrono::NaiveDateTime;
use chrono_tz::OffsetComponents;
use eyre::Context;
use html_builder::Html5;
use open_meteo::{GroundLevel, Hourly, HourlyVariable, TimeZone, WeatherCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    forecast_service,
    receive::{Received, ReceivedKind},
    reply::Reply,
    request::{ForecastRequest, ParsedForecastRequest},
    task::run_retry_log_errors,
    time, topo_data_service,
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

#[derive(Debug, thiserror::Error)]
enum ProcessEmailError {
    #[error("No forecast position specified")]
    NoPosition,
    #[error(transparent)]
    Unexpected(#[from] eyre::Error),
    #[error("A networking error occurred")]
    Network,
}

trait FormatForecast {
    fn format(&self, options: &FormatForecastOptions) -> String;
}

/// Extra options for short [`FormatDetail`].
#[derive(Default, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ShortFormatDetail {
    /// Limit to length of message.
    pub length_limit: Option<usize>,
}

/// Extra options for long [`FormatDetail`].
#[derive(Default, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct LongFormatDetail {
    /// Render the table using html
    pub style: Option<LongFormatStyle>,
}

/// Extra options for long [`FormatDetail`].
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum LongFormatStyle {
    /// Render table and features using html.
    Html,
    /// Render table and features using plain text.
    PlainText,
}

/// What amount of detail to use for formatting the forecast message.
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum FormatDetail {
    /// As short as possible. e.g. `F24`
    Short(ShortFormatDetail),
    /// Expanded with full detail. e.g. `Freezing Level: 2400m`
    Long(LongFormatDetail),
}

impl Default for FormatDetail {
    fn default() -> Self {
        Self::Short(ShortFormatDetail::default())
    }
}

/// Options for formatting the forecast.
#[derive(Default, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct FormatForecastOptions {
    /// Detail to apply to formatting the message.
    pub detail: FormatDetail,
}

struct ForecastOutput {
    errors: Vec<String>,
    total_timezone_offset: chrono::Duration,
    forecast_elevation: f32,
    terrain_elevation: Option<f32>,
    rows: Vec<ForecastRow>,
}

fn newline(format_detail: &FormatDetail) -> &str {
    match format_detail {
        FormatDetail::Short(_) => "\n",
        FormatDetail::Long(long) => match long.style {
            Some(LongFormatStyle::Html) => "<br>",
            _ => "\n",
        },
    }
}
impl FormatForecast for ForecastOutput {
    fn format(&self, options: &FormatForecastOptions) -> String {
        let mut output = String::new();
        let total_offset = &self.total_timezone_offset;
        let formatted_offset: String = if total_offset.is_zero() {
            "GMT".to_string()
        } else {
            let formatted_duration = format!(
                "{:02}:{:02}",
                total_offset.num_hours(),
                total_offset.num_minutes() % 60
            );
            if total_offset > &chrono::Duration::zero() {
                format!("+{}", formatted_duration)
            } else {
                format!("-{}", formatted_duration)
            }
        };

        let forecast_elevation = self.forecast_elevation;

        output.push_str(&match options.detail {
            FormatDetail::Short(_) => format!("Tz{formatted_offset} FE{forecast_elevation}"),
            FormatDetail::Long(_) => {
                format!("Time Zone: {formatted_offset}, Forecast Elevation: {forecast_elevation}")
            }
        });

        if let Some(terrain_elevation) = self.terrain_elevation {
            output.push_str(&match options.detail {
                FormatDetail::Short(_) => format!(" TE{terrain_elevation}"),
                FormatDetail::Long(_) => format!(", Terrain Elevation: {terrain_elevation}"),
            });
        }

        if !self.errors.is_empty() {
            if let FormatDetail::Short(_) = options.detail {
                output.push_str(" E")
            }
        }

        output.push_str(newline(&options.detail));

        if !self.errors.is_empty() {
            if let FormatDetail::Long(_) = options.detail {
                output.push_str("These errors occured:");
                for error in &self.errors {
                    output.push_str(&error);
                    output.push_str(newline(&options.detail));
                }
                output.push_str(newline(&options.detail));
            }
        }

        match &options.detail {
            FormatDetail::Short(short) => {
                for (i, r) in self.rows.iter().enumerate() {
                    let row_output = r.format(options);

                    if let Some(length_limit) = short.length_limit {
                        if output.len() + row_output.len() > length_limit {
                            break;
                        }
                    }

                    if i > 0 {
                        output.push_str(newline(&options.detail))
                    }
                    output.push_str(&row_output);
                }
            }
            FormatDetail::Long(long) => match long.style {
                Some(LongFormatStyle::Html) => {
                    if !self.rows.is_empty() {
                        let style_attr =
                            r#"style="border: 1px solid black;border-collapse: collapse;""#;
                        let mut buffer = html_builder::Buffer::new();
                        let mut table = buffer.table().attr(style_attr);
                        let mut header_row = table.tr();

                        let mut th = header_row.th().attr(style_attr);
                        th.write_str("Time").unwrap();

                        let r = self.rows.first().expect("expected at least one row");
                        for p in &r.parameters {
                            let mut th = header_row.th().attr(style_attr);
                            th.write_str(&p.header()).unwrap();
                        }

                        for r in &self.rows {
                            let mut tr = table.tr();

                            let mut td = tr.td().attr(style_attr);
                            write!(td, "{}", r.time).unwrap();

                            for p in &r.parameters {
                                let mut td = tr.td().attr(style_attr);
                                td.write_str(&p.format(options)).unwrap();
                            }
                        }

                        output.push_str(&buffer.finish());
                    }
                }
                _ => {
                    if !self.rows.is_empty() {
                        let mut builder = tabled::builder::Builder::new();

                        for r in &self.rows {
                            let mut record = vec![r.time.to_string()];
                            for p in &r.parameters {
                                record.push(p.format(options))
                            }

                            builder.add_record(record);
                        }

                        let r = self.rows.first().expect("expected at least one row");
                        let mut columns = vec!["Time".to_string()];
                        for p in &r.parameters {
                            columns.push(p.header());
                        }
                        builder.set_columns(columns);
                        let mut table = builder.build();
                        table.with(tabled::Style::ascii());
                        output.push_str(&table.to_string());
                    }
                }
            },
        }

        output
    }
}

struct ForecastRow {
    time: NaiveDateTime,
    parameters: Vec<ForecastParameter>,
}

impl FormatForecast for ForecastRow {
    fn format(&self, options: &FormatForecastOptions) -> String {
        let mut output: String = self.time.format("%dT%H").to_string();

        for parameter in &self.parameters {
            output.push(' ');
            output.push_str(&parameter.format(options));
        }

        output
    }
}

enum ForecastParameter {
    WeatherCode(WeatherCode),
    FreezingLevelHeight(f32),
    Wind10m { speed: f32, direction: f32 },
    AccumulatedPrecipitation(f32),
}

impl ForecastParameter {
    fn header(&self) -> String {
        match self {
            ForecastParameter::WeatherCode(_) => "Weather Code",
            ForecastParameter::FreezingLevelHeight(_) => "Freezing Level",
            ForecastParameter::Wind10m { .. } => "Wind",
            ForecastParameter::AccumulatedPrecipitation(_) => "Precipitation",
        }
        .to_string()
    }
}

impl FormatForecast for ForecastParameter {
    fn format(&self, options: &FormatForecastOptions) -> String {
        match self {
            ForecastParameter::WeatherCode(code) => match options.detail {
                FormatDetail::Short(_) => format!("C{:.0}", *code as u8),
                FormatDetail::Long(_) => format!("{}", code),
            },

            ForecastParameter::FreezingLevelHeight(height) => match options.detail {
                FormatDetail::Short(_) => format!("F{:.0}", (height / 100.0).round()),
                FormatDetail::Long(_) => format!("{:.0}m", height.round()),
            },
            ForecastParameter::Wind10m { speed, direction } => match options.detail {
                FormatDetail::Short(_) => format!(
                    "W{:.0}@{:.0}",
                    (speed / 10.0).round(),
                    (direction / 10.0).round()
                ),
                FormatDetail::Long(_) => {
                    format!("{:.0} km/h at {:.0}°", speed.round(), direction.round())
                }
            },
            ForecastParameter::AccumulatedPrecipitation(precip) => match options.detail {
                FormatDetail::Short(_) => format!("P{:.0}", precip.round()),
                FormatDetail::Long(_) => format!("{:.1}mm", precip.round()),
            },
        }
    }
}

/// Validate the request from a received email, report any problems via logging, and transform it to a valid
/// request.
fn validate_transform_request(received_email: &ReceivedKind) -> Cow<'_, ParsedForecastRequest> {
    match received_email {
        ReceivedKind::Inreach(email) => {
            let mut request = email.forecast_request.clone();
            let format = &mut request.request.format;
            match &mut format.detail {
                FormatDetail::Short(short) => {
                    // Impose a message length limit of 160 characters for inreach.
                    if let Some(limit) = &mut short.length_limit {
                        if *limit > 160 {
                            tracing::warn!(
                                "User specified limit ({limit}) is too large, \
                        Inreach only supports up to 160 characters per message"
                            );
                            *limit = 160;
                        }
                    } else {
                        short.length_limit = Some(160);
                    }
                }
                _ => {
                    tracing::warn!(
                        "User specified format detail {:?} is not available, \
                        InReach only supports Short format detail.",
                        format.detail
                    );
                    format.detail = FormatDetail::Short(ShortFormatDetail::default());
                }
            }

            Cow::Owned(request)
        }
        _ => Cow::Borrowed(&received_email.forecast_request()),
    }
}

async fn process_email<FS: forecast_service::Port, TDS: topo_data_service::Port>(
    forecast_service: &FS,
    topo_data_service: &TDS,
    received_email: &ReceivedKind,
) -> Result<Reply, ProcessEmailError> {
    let parsed_request = validate_transform_request(received_email);
    let request = &parsed_request.request;

    let position = request
        .position
        .or(received_email.position())
        .ok_or_else(|| ProcessEmailError::NoPosition)?;
    let forecast_parameters = open_meteo::ForecastParameters::builder()
        .latitude(position.latitude)
        .longitude(position.longitude)
        .hourly_entry(HourlyVariable::FreezingLevelHeight)
        .hourly_entry(HourlyVariable::WindSpeed(GroundLevel::L10))
        .hourly_entry(HourlyVariable::WindDirection(GroundLevel::L10))
        .hourly_entry(HourlyVariable::WeatherCode)
        .hourly_entry(HourlyVariable::Precipitation)
        .timezone(TimeZone::Auto)
        .build();

    tracing::debug!(
        "Obtaining forecast for forecast parameters {}",
        serde_json::to_string_pretty(&forecast_parameters).map_err(eyre::Error::from)?
    );
    let forecast: open_meteo::Forecast = forecast_service
        .obtain_forecast(&forecast_parameters)
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
        .wind_speed
        .value(&GroundLevel::L10)
        .ok_or_else(|| eyre::eyre!("expected wind_speed_10m to be present"))?;
    let wind_direction_10m: &[f32] = &hourly
        .wind_direction
        .value(&GroundLevel::L10)
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
        return Err(eyre::eyre!("forecast hourly array lengths don't match").into());
    }

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

    let terrain_elevation = match topo_data_service
        .obtain_elevation(&open_topo_data::Parameters {
            latitude: position.latitude,
            longitude: position.longitude,
            dataset: open_topo_data::Dataset::Mapzen,
        })
        .await
        .wrap_err("Error obtaining terrain elevation")
    {
        Ok(terrain_elevation) => Some(terrain_elevation),
        Err(error) => {
            tracing::error!("{}", error);
            None
        }
    };

    let mut forecast_rows: Vec<ForecastRow> = Vec::with_capacity(16);

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
            forecast_rows.push(ForecastRow {
                time: time[i],
                parameters: vec![
                    ForecastParameter::WeatherCode(weather_code[i]),
                    ForecastParameter::FreezingLevelHeight(freezing_level_height[i]),
                    ForecastParameter::Wind10m {
                        speed: wind_speed_10m[i],
                        direction: wind_direction_10m[i],
                    },
                    ForecastParameter::AccumulatedPrecipitation(acc_precipitation),
                ],
            });
            acc_precipitation = 0.0;
        }
        i += 1;
    }

    let errors: Vec<String> = parsed_request
        .errors
        .iter()
        .map(|error| format!("Error parsing request: {}", error))
        .collect();

    let forecast_output = ForecastOutput {
        errors,
        total_timezone_offset: total_offset,
        forecast_elevation: forecast.elevation,
        terrain_elevation,
        rows: forecast_rows,
    };

    let message: String = forecast_output.format(&request.format);
    let (plain_message, html_message): (String, Option<String>) =
        if let FormatDetail::Long(long) = &request.format.detail {
            if let Some(LongFormatStyle::Html) = long.style {
                let mut plain_long = long.clone();
                let mut plain_format = request.format.clone();
                plain_long.style = Some(LongFormatStyle::PlainText);
                plain_format.detail = FormatDetail::Long(plain_long);

                let plain_message = forecast_output.format(&plain_format);
                (plain_message, Some(message))
            } else {
                (message, None)
            }
        } else {
            (message, None)
        };

    tracing::info!("Sending reply for email {:?}", received_email);

    tracing::info!(
        "plain_message (len: {}):\n{}",
        plain_message.len(),
        plain_message
    );
    if let Some(html_message) = &html_message {
        tracing::info!(
            "html_message (len: {}):\n{}",
            html_message.len(),
            html_message
        );
    }

    Ok(Reply::from_received(
        received_email.clone(),
        plain_message,
        html_message,
    ))
}

async fn process_emails_impl(
    process_receiver: &mut yaque::Receiver,
    reply_sender: &mut yaque::Sender,
    http_client: reqwest::Client,
) -> eyre::Result<()> {
    let forecast_service = forecast_service::Gateway::new(http_client.clone());
    let topo_data_service = topo_data_service::Gateway::new(http_client);
    loop {
        let received = process_receiver.recv().await?;
        let received_email: ReceivedKind = serde_json::from_slice(&*received)?;

        let reply =
            match process_email(&forecast_service, &topo_data_service, &received_email).await {
                Ok(reply) => reply,
                Err(error) => match &error {
                    ProcessEmailError::NoPosition => Reply::from_received(
                        received_email,
                        "No forecast position specified".to_string(),
                        None,
                    ),
                    ProcessEmailError::Unexpected(error) => {
                        tracing::error!("Unexpected error occurred: {:?}", error);
                        Reply::from_received(
                            received_email,
                            "An error occurred while processing your request".to_string(),
                            None,
                        )
                    }
                    ProcessEmailError::Network => return Err(error.into()),
                },
            };
        let reply_bytes = serde_json::to_vec(&reply).wrap_err("Failed to serialize reply")?;
        reply_sender.send(&reply_bytes).await?;

        received.commit()?;
    }
}

/// This function spawns a task to process an incoming email, create a customized forecast that it
/// requested, and dispatch a reply.
#[tracing::instrument(skip_all)]
pub async fn process_emails(
    process_receiver: yaque::Receiver,
    reply_sender: yaque::Sender,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    http_client: reqwest::Client,
    time: &dyn time::Port,
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
        time,
    )
    .await;
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use mockall::predicate::eq;
    use once_cell::sync::Lazy;
    use open_meteo::{Forecast, ForecastParameters, GroundLevel, HourlyVariable};

    use crate::{
        forecast_service,
        gis::Position,
        inreach,
        process::{FormatDetail, FormatForecastOptions, ShortFormatDetail},
        reply::{self, Reply},
        request::{ForecastRequest, ParsedForecastRequest},
        topo_data_service,
    };

    use super::{process_email, WindDirection};

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

    static FORECAST_MT_COOK: Lazy<Forecast> = Lazy::new(|| {
        serde_json::from_str(&std::fs::read_to_string("fixtures/forecast_mt_cook.json").unwrap())
            .unwrap()
    });

    /// Test where the received email is from an inreach, and the user is requesting a forecast for
    /// a location other than where the inreach is located.
    #[tokio::test]
    async fn test_process_email_inreach_parsed_location() {
        let forecast_request = ParsedForecastRequest {
            request: ForecastRequest {
                position: Some(Position::new(-43.513832, 170.33975)),
                format: FormatForecastOptions {
                    detail: FormatDetail::Short(ShortFormatDetail::default()),
                },
            },
            ..ParsedForecastRequest::default()
        };
        let referral_url: url::Url = "https://example.org".parse().unwrap();
        let received_email = &crate::receive::ReceivedKind::Inreach(inreach::email::Received {
            from_name: "Test".to_owned(),
            referral_url: referral_url.clone(),
            position: Position::new(-43.75905, 170.115),
            forecast_request,
        });
        let mut forecast_service = forecast_service::MockPort::new();
        forecast_service
            .expect_obtain_forecast()
            .with(eq(ForecastParameters::builder()
                .latitude(-43.513832)
                .longitude(170.33975)
                .hourly_entry(HourlyVariable::FreezingLevelHeight)
                .hourly_entry(HourlyVariable::WindSpeed(GroundLevel::L10))
                .hourly_entry(HourlyVariable::WindDirection(GroundLevel::L10))
                .hourly_entry(HourlyVariable::WeatherCode)
                .hourly_entry(HourlyVariable::Precipitation)
                .timezone(open_meteo::TimeZone::Auto)
                .build()))
            .return_once(|_| Ok(FORECAST_MT_COOK.clone()));
        let mut topo_data_service = topo_data_service::MockPort::new();

        topo_data_service
            .expect_obtain_elevation()
            .with(eq(open_topo_data::Parameters {
                latitude: -43.513832,
                longitude: 170.33975,
                dataset: open_topo_data::Dataset::Mapzen,
            }))
            .return_once(|_| Ok(2216.0));

        let reply = process_email(&forecast_service, &topo_data_service, received_email)
            .await
            .unwrap();

        let reply: reply::InReach = match reply {
            Reply::InReach(reply) => reply,
            _ => panic!("Unexpected reply: {:?}", reply),
        };

        assert_eq!(referral_url, reply.referral_url);
        insta::assert_snapshot!(reply.message);
    }
}
