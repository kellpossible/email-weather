use std::collections::{HashSet, HashMap};

use reqwest::{Method, StatusCode};
use serde::{de::Visitor, Deserialize, Serialize, ser::SerializeMap};

/// WMO Weather interpretation code (WW)
#[derive(Debug)]
pub enum WeatherCode {
    /// Code: 0
    ClearSky,
    /// Code: 1
    MainlyClear,
    /// Code: 2
    PartlyCloudy,
    /// Code: 3
    Overcast,
    /// Code: 45
    Fog,
    /// Code: 48
    FogDepositingRime,
    /// Code: 51
    DrizzleLight,
    /// Code: 53
    DrizzleModerate,
    /// Code: 55
    DrizzleDense,
    /// Code: 56
    DrizzleFreezingLight,
    /// Code: 57
    DrizzleFreezingDense,
    /// Code: 61
    RainSlight,
    /// Code: 63
    RainModerate,
    /// Code: 65
    RainHeavy,
    /// Code: 66
    RainFreezingLight,
    /// Code: 67
    RainFreezingHeavy,
    /// Code: 71
    SnowSlight,
    /// Code: 73
    SnowModerate,
    /// Code: 75
    SnowHeavy,
    /// Code: 77
    SnowGrains,
    /// Code: 80
    RainShowersSlight,
    /// Code: 81
    RainShowersModerate,
    /// Code: 82
    RainShowersViolent,
    /// Code: 85
    SnowShowersSlight,
    /// Code: 86
    SnowShowersHeavy,
    /// Code: 95
    ///
    /// Note: Thunderstorm forecast with hail is only available in Central Europe
    ThunderstormSlightOrModerate,
    /// Code: 96
    ///
    /// *Note: Thunderstorm forecast with hail is only available in Central Europe*
    ThunderstormHailSlight,
    /// Code: 99
    ///
    /// *Note: Thunderstorm forecast with hail is only available in Central Europe*
    ThunderstormHailHeavy,
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

            fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
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
                    _ => return Err(E::custom(format!("Invalid weather code: {}", v))),
                })
            }
        }
        deserializer.deserialize_u8(WeatherCodeVisitor)
    }
}

#[derive(Serialize, Hash, PartialEq, Eq)]
#[serde(rename = "snake_case")]
pub enum HourlyVariable {
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
    time: Vec<chrono::NaiveDateTime>,
    /// Air temperature at 2 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    temperature_2m: Option<Vec<f32>>,
    /// Relative humidity at 2 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "relativehumidity_2m")]
    relative_humidity_2m: Option<Vec<f32>>,
    /// Dew point temperature at 2 meters above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    dewpoint_2m: Option<Vec<f32>>,
    /// Apparent temperature is the perceived feels-like temperature combining wind chill factor,
    /// relative humidity and solar radiation.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    apparent_temperature: Option<Vec<f32>>,
    /// Atmospheric air pressure reduced to mean sea level (msl) Typically pressure on mean sea
    /// level is used in meteorology.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `hPa`
    ///
    /// See also: [Hourly::surface_pressure].
    pressure_msl: Option<Vec<f32>>,
    /// Atmospheric air pressure reduced to pressure at surface. Surface pressure gets lower with
    /// increasing elevation.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `hPa`
    ///
    /// See also: [Hourly::pressure_msl].
    surface_pressure: Option<Vec<f32>>,
    /// Total cloud cover as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover")]
    cloud_cover: Option<Vec<f32>>,
    /// Low level cloud and fog cover up to 3 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover_low")]
    cloud_cover_low: Option<Vec<f32>>,
    /// Mid level cloud and fog cover 3 to 8 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover_mid")]
    cloud_cover_mid: Option<Vec<f32>>,
    /// High level cloud and fog cover from 8 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    #[serde(rename = "cloudcover_high")]
    cloud_cover_high: Option<Vec<f32>>,

    // TODO: more fields
    /// Weather condition.
    ///
    /// + Valid time: `Instant`
    #[serde(rename = "weathercode")]
    weather_code: Option<Vec<WeatherCode>>,
    /// Snow depth on the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `meters`
    snow_depth: Option<Vec<f32>>,
    /// Altitude above sea level of the 0°C level.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `meters`
    #[serde(rename = "freezinglevel_height")]
    freezing_level_height: Vec<f32>,
}

#[derive(Serialize, Hash, PartialEq, Eq)]
#[serde(rename = "snake_case")]
pub enum DailyWeatherVariable {}

#[derive(Serialize)]
#[serde(rename = "snake_case")]
pub enum TemperatureUnit {
    Celcius,
    Farenheit,
}

impl Default for TemperatureUnit {
    fn default() -> Self {
        Self::Celcius
    }
}

#[derive(Serialize)]
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

#[derive(Serialize)]
#[serde(rename = "snake_case")]
pub enum PrecipitationUnit {
    Mm,
    Inch,
}

impl Default for PrecipitationUnit {
    fn default() -> Self {
        Self::Mm
    }
}

#[derive(Serialize)]
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

#[derive(buildstructor::Builder)]
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
        S: serde::Serializer {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("latitude", &self.latitude)?;
        map.serialize_entry("longitude", &self.longitude)?;
        self.time_format.as_ref().map(|v| map.serialize_entry("timeformat", v)).transpose()?;
        self.timezone.as_ref().map(|v| map.serialize_entry("timezone", v)).transpose()?;
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
    pub utc_offset_seconds: u64,
    /// Timezone identifier.
    pub timezone: chrono_tz::Tz,
    /// Timezone abbreviation.
    pub timezone_abbreviation: String,
    /// Hourly forecast data.
    pub hourly: Option<Hourly>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Error while performing request")]
    Reqwest(#[from] reqwest::Error),
    #[error("Response status unsuccessful, code: {code}, reason: {reason}")]
    ResponseStatusNotSuccessful { code: StatusCode, reason: String },
}

#[derive(Deserialize)]
struct ErrorMessage {
    reason: String,
}

pub async fn obtain_forecast(
    client: &reqwest::Client,
    parameters: &ForecastParameters,
) -> Result<Forecast, Error> {
    let response = client
        .request(Method::GET, "https://api.open-meteo.com/v1/forecast")
        .query(parameters)
        .send()
        .await?;

    if response.status().is_success() {
        let forecast = response.json::<Forecast>().await?;
        Ok(forecast)
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
}
