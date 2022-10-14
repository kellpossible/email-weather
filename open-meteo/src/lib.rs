use std::collections::{HashMap, HashSet};

use chrono::NaiveDateTime;
use reqwest::{Method, StatusCode};
use serde::{de::Visitor, ser::SerializeMap, Deserialize, Serialize};

/// WMO Weather interpretation code (WW)
#[derive(Debug)]
pub enum WeatherCode {
    /// Code: 0
    ClearSky = 0,
    /// Code: 1
    MainlyClear = 1,
    /// Code: 2
    PartlyCloudy = 2,
    /// Code: 3
    Overcast = 3,
    /// Code: 45
    Fog = 45,
    /// Code: 48
    FogDepositingRime = 48,
    /// Code: 51
    DrizzleLight = 51,
    /// Code: 53
    DrizzleModerate = 53,
    /// Code: 55
    DrizzleDense = 55,
    /// Code: 56
    DrizzleFreezingLight = 56,
    /// Code: 57
    DrizzleFreezingDense = 57,
    /// Code: 61
    RainSlight = 61,
    /// Code: 63
    RainModerate = 63,
    /// Code: 65
    RainHeavy = 65,
    /// Code: 66
    RainFreezingLight = 66,
    /// Code: 67
    RainFreezingHeavy = 67,
    /// Code: 71
    SnowSlight = 71,
    /// Code: 73
    SnowModerate = 73,
    /// Code: 75
    SnowHeavy = 75,
    /// Code: 77
    SnowGrains = 77,
    /// Code: 80
    RainShowersSlight = 80,
    /// Code: 81
    RainShowersModerate = 81,
    /// Code: 82
    RainShowersViolent = 82,
    /// Code: 85
    SnowShowersSlight = 85,
    /// Code: 86
    SnowShowersHeavy = 86,
    /// Code: 95
    ///
    /// Note: Thunderstorm forecast with hail is only available in Central Europe
    ThunderstormSlightOrModerate = 95,
    /// Code: 96
    ///
    /// *Note: Thunderstorm forecast with hail is only available in Central Europe*
    ThunderstormHailSlight = 96,
    /// Code: 99
    ///
    /// *Note: Thunderstorm forecast with hail is only available in Central Europe*
    ThunderstormHailHeavy = 99,
}

impl<'de> Deserialize<'de> for WeatherCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WeatherCodeVisitor;

        impl<'de> Visitor<'de> for WeatherCodeVisitor {
            type Value = WeatherCode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an unsigned integer between 0 and 99")
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v.is_negative() {
                    return Err(E::custom(format!("Cannot parse negative integer `{}`", v)));
                }
                self.visit_u64(v as u64)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(match v {
                    0 => WeatherCode::ClearSky,
                    1 => WeatherCode::MainlyClear,
                    2 => WeatherCode::PartlyCloudy,
                    3 => WeatherCode::Overcast,
                    45 => WeatherCode::Fog,
                    48 => WeatherCode::FogDepositingRime,
                    51 => WeatherCode::DrizzleLight,
                    53 => WeatherCode::DrizzleModerate,
                    55 => WeatherCode::DrizzleDense,
                    56 => WeatherCode::DrizzleFreezingLight,
                    57 => WeatherCode::DrizzleFreezingDense,
                    61 => WeatherCode::RainSlight,
                    63 => WeatherCode::RainModerate,
                    65 => WeatherCode::RainHeavy,
                    66 => WeatherCode::RainFreezingLight,
                    67 => WeatherCode::RainFreezingHeavy,
                    71 => WeatherCode::SnowSlight,
                    73 => WeatherCode::SnowModerate,
                    75 => WeatherCode::SnowHeavy,
                    77 => WeatherCode::SnowGrains,
                    80 => WeatherCode::RainShowersSlight,
                    81 => WeatherCode::RainShowersModerate,
                    82 => WeatherCode::RainShowersViolent,
                    85 => WeatherCode::SnowShowersSlight,
                    86 => WeatherCode::SnowShowersHeavy,
                    95 => WeatherCode::ThunderstormSlightOrModerate,
                    96 => WeatherCode::ThunderstormHailSlight,
                    99 => WeatherCode::ThunderstormHailHeavy,
                    _ => {
                        return Err(E::custom(format!(
                            "Unsupported/invalid weather code: `{}`",
                            v
                        )))
                    }
                })
            }
        }
        deserializer.deserialize_u8(WeatherCodeVisitor)
    }
}

#[derive(Debug, Deserialize, Serialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HourlyVariable {
    /// This isn't a selectable value, but it can be returned in [Forecast::hourly_units].
    Time,
    /// Requests [Hourly::temperature_2m].
    #[serde(rename = "temperature_2m")]
    Temperature2m,
    /// Requests [Hourly::relative_humidity_2m].
    #[serde(rename = "relativehumidity_2m")]
    RelativeHumidity2m,
    /// Requests [Hourly::dewpoint_2m].
    #[serde(rename = "dewpoint_2m")]
    Dewpoint2m,
    /// Requests [Hourly::apparent_temperature].
    ApparentTemperature,
    /// Requests [Hourly::pressure_msl].
    PressureMsl,
    /// Requests [Hourly::surface_pressure].
    SurfacePressure,
    /// Requests [Hourly::cloud_cover].
    #[serde(rename = "cloudcover")]
    CloudCover,
    /// Requests [Hourly::cloud_cover_low].
    #[serde(rename = "cloudcover_low")]
    CloudCoverLow,
    /// Requests [Hourly::cloud_cover_low].
    #[serde(rename = "cloudcover_mid")]
    CloudCoverMid,
    /// Requests [Hourly::cloud_cover_high].
    #[serde(rename = "cloudcover_high")]
    CloudCoverHigh,
    /// Requests [Hourly::wind_speed_10m].
    #[serde(rename = "windspeed_10m")]
    WindSpeed10m,
    /// Requests [Hourly::wind_speed_80m].
    #[serde(rename = "windspeed_80m")]
    WindSpeed80m,
    /// Requests [Hourly::wind_speed_120m].
    #[serde(rename = "windspeed_120m")]
    WindSpeed120m,
    /// Requests [Hourly::wind_speed_180m].
    #[serde(rename = "windspeed_180m")]
    WindSpeed180m,
    /// Requests [Hourly::wind_direction_10m].
    #[serde(rename = "winddirection_10m")]
    WindDirection10m,
    /// Requests [Hourly::wind_direction_80m].
    #[serde(rename = "winddirection_80m")]
    WindDirection80m,
    /// Requests [Hourly::wind_direction_120m].
    #[serde(rename = "winddirection_120m")]
    WindDirection120m,
    /// Requests [Hourly::wind_direction_180m].
    #[serde(rename = "winddirection_180m")]
    WindDirection180m,
    /// Requests [Hourly::wind_gusts_10m].
    #[serde(rename = "windgusts_10m")]
    WindGusts10m,
    // TODO: more fields
    /// Requests [Hourly::precipitation].
    Precipitation,
    // TODO: more fields
    /// Requests [Hourly::weather_code].
    #[serde(rename = "weathercode")]
    WeatherCode,
    /// Requests [Hourly::snow_depth].
    #[serde(rename = "snow_depth")]
    SnowDepth,
    /// Requests [Hourly::freezing_level_height].
    #[serde(rename = "freezinglevel_height")]
    FreezingLevelHeight,
}

#[derive(Debug, Deserialize)]
pub struct Hourly {
    /// The times for the values in this struct's fields.
    #[serde(deserialize_with = "naive_times_deserialize")]
    pub time: Vec<chrono::NaiveDateTime>,
    /// Air temperature at 2 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    pub temperature_2m: Option<Vec<f32>>,
    /// Relative humidity at 2 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "relativehumidity_2m")]
    pub relative_humidity_2m: Option<Vec<f32>>,
    /// Dew point temperature at 2 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    pub dewpoint_2m: Option<Vec<f32>>,
    /// Apparent temperature is the perceived feels-like temperature combining wind chill factor,
    /// relative humidity and solar radiation.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    pub apparent_temperature: Option<Vec<f32>>,
    /// Atmospheric air pressure reduced to mean sea level (msl) Typically pressure on mean sea
    /// level is used in meteorology.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `hPa`
    ///
    /// See also: [Hourly::surface_pressure].
    pub pressure_msl: Option<Vec<f32>>,
    /// Atmospheric air pressure reduced to pressure at surface. Surface pressure gets lower with
    /// increasing elevation.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `hPa`
    ///
    /// See also: [Hourly::pressure_msl].
    pub surface_pressure: Option<Vec<f32>>,
    /// Total cloud cover as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover")]
    pub cloud_cover: Option<Vec<f32>>,
    /// Low level cloud and fog cover up to 3 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover_low")]
    pub cloud_cover_low: Option<Vec<f32>>,
    /// Mid level cloud and fog cover 3 to 8 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover_mid")]
    pub cloud_cover_mid: Option<Vec<f32>>,
    /// High level cloud and fog cover from 8 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover_high")]
    pub cloud_cover_high: Option<Vec<f32>>,
    /// Wind speed at 10 meters above ground. Wind speed at 10 meters is the standard level.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `km/h (mph, m/s, knots)`
    #[serde(rename = "windspeed_10m")]
    pub wind_speed_10m: Option<Vec<f32>>,
    /// Wind speed at 80 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `km/h (mph, m/s, knots)`
    #[serde(rename = "windspeed_80m")]
    pub wind_speed_80m: Option<Vec<f32>>,
    /// Wind speed at 120 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `km/h (mph, m/s, knots)`
    #[serde(rename = "windspeed_120m")]
    pub wind_speed_120m: Option<Vec<f32>>,
    /// Wind speed at 180 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `km/h (mph, m/s, knots)`
    #[serde(rename = "windspeed_180m")]
    pub wind_speed_180m: Option<Vec<f32>>,
    /// Wind direction at 10 meters above the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°`
    #[serde(rename = "winddirection_10m")]
    pub wind_direction_10m: Option<Vec<f32>>,
    /// Wind direction at 80 meters above the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°`
    #[serde(rename = "winddirection_80m")]
    pub wind_direction_80m: Option<Vec<f32>>,
    /// Wind direction at 120 meters above the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°`
    #[serde(rename = "winddirection_120m")]
    pub wind_direction_120m: Option<Vec<f32>>,
    /// Wind direction at 180 meters above the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°`
    #[serde(rename = "winddirection_180m")]
    pub wind_direction_180m: Option<Vec<f32>>,
    // TODO: more fields
    /// Total precipitation (rain, showers, snow) sum of the preceding hour.
    ///
    /// + Valid time: `Preceding hour sum`
    /// + Unit: `mm (inch)`
    pub precipitation: Option<Vec<f32>>,
    // TODO: more fields
    /// Weather condition.
    ///
    /// + Valid time: `Instant`
    #[serde(rename = "weathercode")]
    pub weather_code: Option<Vec<WeatherCode>>,
    /// Snow depth on the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `meters`
    pub snow_depth: Option<Vec<f32>>,
    /// Altitude above sea level of the 0°C level.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `meters`
    #[serde(rename = "freezinglevel_height")]
    pub freezing_level_height: Option<Vec<f32>>,
}

/// Deserialize date time in ISO8601 format without seconds or timezone.
fn naive_time_deserialize<'de, D>(deserializer: D) -> Result<chrono::NaiveDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StrVisitor;
    impl<'de> serde::de::Visitor<'de> for StrVisitor {
        type Value = NaiveDateTime;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                formatter,
                "An ISO8601 date without the seconds or the timezone: e.g. 2022-08-02T10:42"
            )
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M")
                .map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_str(StrVisitor)
}

/// Deserialize sequence of date time in ISO8601 format without seconds or timezone.
fn naive_times_deserialize<'de, D>(deserializer: D) -> Result<Vec<chrono::NaiveDateTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DateTime(chrono::NaiveDateTime);

    impl<'de> Deserialize<'de> for DateTime {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            naive_time_deserialize(deserializer).map(DateTime)
        }
    }

    struct SeqVisitor;
    impl<'de> serde::de::Visitor<'de> for SeqVisitor {
        type Value = Vec<chrono::NaiveDateTime>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "Expecting a sequence of values")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut v = Vec::new();
            while let Some(element) = seq.next_element::<DateTime>()? {
                v.push(element.0)
            }

            Ok(v)
        }
    }
    deserializer.deserialize_seq(SeqVisitor)
}

#[derive(Serialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DailyWeatherVariable {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureUnit {
    Celcius,
    Farenheit,
}

impl Default for TemperatureUnit {
    fn default() -> Self {
        Self::Celcius
    }
}

#[derive(Debug, Serialize)]
#[serde(rename = "snake_case")]
pub enum WindspeedUnit {
    Kmh,
    Ms,
    Mph,
    Kn,
}

impl Default for WindspeedUnit {
    fn default() -> Self {
        Self::Kmh
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecipitationUnit {
    Mm,
    Inch,
}

impl Default for PrecipitationUnit {
    fn default() -> Self {
        Self::Mm
    }
}

#[derive(Debug, Serialize)]
#[serde(rename = "snake_case")]
pub enum TimeFormat {
    Iso8601,
    /// UNIX epoch time in seconds.
    ///
    /// Please note that all timestamp are in GMT+0! For daily values with unix timestamps, please
    /// apply [Forecast::utc_offset_seconds] again to get the correct date.
    Unixtime,
}

impl Default for TimeFormat {
    fn default() -> Self {
        Self::Iso8601
    }
}

#[derive(Debug)]
pub enum TimeZone {
    /// The position coordinates will be automatically resolved to the local time zone.
    Auto,
    /// All timestamps are returned as local-time and data is returned starting at 00:00
    /// local-time.
    Tz(chrono_tz::Tz),
}

impl Default for TimeZone {
    fn default() -> Self {
        Self::Tz(chrono_tz::Tz::GMT)
    }
}

impl Serialize for TimeZone {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            TimeZone::Auto => serializer.serialize_str("auto"),
            TimeZone::Tz(timezone) => timezone.serialize(serializer),
        }
    }
}

#[derive(Debug, buildstructor::Builder)]
pub struct ForecastParameters {
    /// Geographical WGS84 latitude of the location.
    pub latitude: f32,
    /// Geographical WGS84 longitude of the location.
    pub longitude: f32,
    /// A set of hourly weather variables which should be returned.
    pub hourly: HashSet<HourlyVariable>,
    /// A set of daily weather variable aggregations which should be returned.
    pub daily: HashSet<HourlyVariable>,
    /// Include current weather conditions in the JSON output.
    pub current_weather: Option<bool>,
    /// What unit to return temperatures in.
    pub temperature_unit: Option<TemperatureUnit>,
    /// What unit to return wind speeds in.
    pub windspeed_unit: Option<WindspeedUnit>,
    /// What unit to return precipitation amounts in.
    pub precipitation_unit: Option<PrecipitationUnit>,
    pub time_format: Option<TimeFormat>,
    pub timezone: Option<TimeZone>,
    /// If set, yesterday or the day before yesterday data are also returned.
    pub past_days: Option<u8>,
    /// The time interval to get weather data. Must be specified in conjunction with
    /// [ForecastParameters::end_date].
    pub start_date: Option<chrono::NaiveDate>,
    /// The time interval to get weather data. Must be specified in conjunction with
    /// [ForecastParameters::start_date].
    pub end_date: Option<chrono::NaiveDate>,
}

impl Serialize for ForecastParameters {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("latitude", &self.latitude)?;
        map.serialize_entry("longitude", &self.longitude)?;
        for hv in &self.hourly {
            map.serialize_entry("hourly", &hv)?;
        }
        for dv in &self.daily {
            map.serialize_entry("daily", &dv)?;
        }
        self.time_format
            .as_ref()
            .map(|v| map.serialize_entry("timeformat", v))
            .transpose()?;
        self.timezone
            .as_ref()
            .map(|v| map.serialize_entry("timezone", v))
            .transpose()?;
        self.past_days
            .map(|v| map.serialize_entry("past_days", &v))
            .transpose()?;
        self.start_date
            .map(|v| map.serialize_entry("start_date", &v))
            .transpose()?;
        self.end_date
            .map(|v| map.serialize_entry("end_date", &v))
            .transpose()?;
        map.end()
    }
}

#[derive(Debug, Deserialize)]
pub struct CurrentWeather {
    time: chrono::NaiveDateTime,
    temperature: f32,
    #[serde(rename = "weathercode")]
    weather_code: WeatherCode,
    #[serde(rename = "windspeed")]
    wind_speed: f32,
    #[serde(rename = "winddirection")]
    wind_direction: u16,
}

#[derive(Debug, Deserialize)]
pub struct Forecast {
    /// Geographical WGS84 latitude of the center of the weather grid-cell which was used to
    /// generate this forecast. This coordinate might be up to 5 km away..
    pub latitude: f32,
    /// Geographical WGS84 longitude of the center of the weather grid-cell which was used to
    /// generate this forecast. This coordinate might be up to 5 km away..
    pub longitude: f32,
    /// The elevation in meters of the selected weather grid-cell. In mountain terrain it might
    /// differ from the location you would expect.
    pub elevation: f32,
    /// Generation time of the weather forecast in milliseconds. This is mainly used for
    /// performance monitoring and improvements.
    #[serde(rename = "generationtime_ms")]
    pub generation_time_ms: f32,
    /// Applied timezone offset from the [ForecastParameter::time_format] parameter.
    pub utc_offset_seconds: i64,
    /// Timezone identifier.
    pub timezone: chrono_tz::Tz,
    /// Timezone abbreviation.
    pub timezone_abbreviation: String,
    /// Hourly forecast data.
    pub hourly: Option<Hourly>,
    /// For each selected weather variable, the unit will be listed here.
    pub hourly_units: Option<HashMap<HourlyVariable, String>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Error while performing request")]
    Reqwest(#[from] reqwest::Error),
    #[error("Response status unsuccessful, code: {code}, reason: {reason}")]
    ResponseStatusNotSuccessful { code: StatusCode, reason: String },
    #[error("Error while parsing json")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Error while seriazizing url query parameters")]
    SerdeUrlencoded(#[from] serde_urlencoded::ser::Error),
}

#[derive(Deserialize)]
struct ErrorMessage {
    reason: String,
}

pub async fn obtain_forecast(
    client: &reqwest::Client,
    parameters: &ForecastParameters,
) -> Result<Forecast, Error> {
    let query = serde_urlencoded::to_string(&parameters)?;
    let url = format!("https://api.open-meteo.com/v1/forecast?{}", query);
    tracing::trace!("GET {}", url);

    let response = client.request(Method::GET, url).send().await?;

    if response.status().is_success() {
        response.json().await.map_err(Error::from)
    } else {
        Err(Error::ResponseStatusNotSuccessful {
            code: response.status(),
            reason: response
                .json::<ErrorMessage>()
                .await
                .map(|message| message.reason)
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod test {
    use chrono_tz::Tz;
    use serde_json::json;

    use crate::Forecast;

    use super::TimeZone;

    #[test]
    fn timezone_serialize() {
        let timezone_auto = serde_json::to_value(&TimeZone::Auto).unwrap();
        assert_eq!(json!("auto"), timezone_auto);

        let timezone_default = serde_json::to_value(&TimeZone::default()).unwrap();
        assert_eq!(json!("GMT"), timezone_default);

        let timezone_auckland = serde_json::to_value(&TimeZone::Tz(Tz::Pacific__Auckland)).unwrap();
        assert_eq!(json!("Pacific/Auckland"), timezone_auckland);
    }

    #[test]
    fn forecast_deserialize() {
        let forecast_json = r#"{
  "elevation": 0.0,
  "generationtime_ms": 0.5849599838256836,
  "hourly": {
    "freezinglevel_height": [
      2000.0,
      1980.0
    ],
    "time": [
      "2022-10-04T00:00",
      "2022-10-04T01:00"
    ]
  },
  "hourly_units": {
    "freezinglevel_height": "m",
    "time": "iso8601"
  },
  "latitude": -43.375,
  "longitude": 170.25,
  "timezone": "Pacific/Auckland",
  "timezone_abbreviation": "NZDT",
  "utc_offset_seconds": 46800
}"#;

        let forecast: Forecast = serde_json::from_str(forecast_json).unwrap();
    }
}
