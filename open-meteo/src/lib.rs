use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    hash::Hash,
};

pub mod level;

use chrono::NaiveDateTime;
use level::{Level, LevelField, LevelVariable};
use once_cell::sync::Lazy;
use reqwest::{Method, StatusCode};
use serde::{
    de::{IntoDeserializer, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize,
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// WMO Weather interpretation code (WW)
#[derive(EnumIter, Clone, Copy, Debug)]
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

static WEATHER_CODE_VARIANTS: Lazy<Vec<WeatherCode>> = Lazy::new(|| WeatherCode::iter().collect());

impl WeatherCode {
    /// Enumerate all variants of WeatherCode.
    pub fn enumerate() -> &'static [WeatherCode] {
        WEATHER_CODE_VARIANTS.as_slice()
    }

    pub fn code(&self) -> u8 {
        *self as u8
    }
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

impl Display for WeatherCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            WeatherCode::ClearSky => "clear sky",
            WeatherCode::MainlyClear => "mainly clear",
            WeatherCode::PartlyCloudy => "partly cloudy",
            WeatherCode::Overcast => "overcast",
            WeatherCode::Fog => "fog",
            WeatherCode::FogDepositingRime => "fog depositing rime",
            WeatherCode::DrizzleLight => "light drizzle",
            WeatherCode::DrizzleModerate => "moderate drizzle",
            WeatherCode::DrizzleDense => "dense drizzle",
            WeatherCode::DrizzleFreezingLight => "light freezing drizzle",
            WeatherCode::DrizzleFreezingDense => "dense freezing drizzle",
            WeatherCode::RainSlight => "slight rain",
            WeatherCode::RainModerate => "moderate rain",
            WeatherCode::RainHeavy => "heavy rain",
            WeatherCode::RainFreezingLight => "light freezing rain",
            WeatherCode::RainFreezingHeavy => "heavy freezing rain",
            WeatherCode::SnowSlight => "slight snow",
            WeatherCode::SnowModerate => "moderate snow",
            WeatherCode::SnowHeavy => "heavy snow",
            WeatherCode::SnowGrains => "snow grains",
            WeatherCode::RainShowersSlight => "slight rain showers",
            WeatherCode::RainShowersModerate => "moderate rain showers",
            WeatherCode::RainShowersViolent => "violent rain showers",
            WeatherCode::SnowShowersSlight => "slight snow showers",
            WeatherCode::SnowShowersHeavy => "heavy snow showers",
            WeatherCode::ThunderstormSlightOrModerate => "slight or moderate thunderstorm",
            WeatherCode::ThunderstormHailSlight => "slight thunderstorm with hail",
            WeatherCode::ThunderstormHailHeavy => "heavy thunderstorm with hail",
        })
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum HourlyVariable {
    /// This isn't a selectable value, but it can be returned in [Forecast::hourly_units].
    Time,
    /// Requests [Hourly::temperature_2m].
    Temperature2m,
    /// Requests [Hourly::relative_humidity_2m].
    RelativeHumidity2m,
    /// Requests [Hourly::dewpoint_2m].
    Dewpoint2m,
    /// Requests [Hourly::apparent_temperature].
    ApparentTemperature,
    /// Requests [Hourly::pressure_msl].
    PressureMsl,
    /// Requests [Hourly::surface_pressure].
    SurfacePressure,
    /// Requests [Hourly::cloud_cover].
    CloudCover,
    /// Requests [Hourly::cloud_cover_low].
    CloudCoverLow,
    /// Requests [Hourly::cloud_cover_low].
    CloudCoverMid,
    /// Requests [Hourly::cloud_cover_high].
    CloudCoverHigh,
    /// Requests [Hourly::wind_speed].
    WindSpeed(GroundLevel),
    /// Requests [Hourly::wind_direction].
    WindDirection(GroundLevel),
    /// Requests [Hourly::wind_gusts_10m].
    WindGusts10m,
    // TODO: more fields
    /// Requests [Hourly::precipitation].
    Precipitation,
    // TODO: more fields
    /// Requests [Hourly::weather_code].
    WeatherCode,
    /// Requests [Hourly::snow_depth].
    SnowDepth,
    /// Requests [Hourly::freezing_level_height].
    FreezingLevelHeight,
    /// Requests [Hourly::pressure_temperature],
    PressureTemperature(PressureLevel),
    /// Requests [Hourly::pressure_geopotential_height],
    PressureGeopotentialHeight(PressureLevel),
}

static HOURLY_ENUMERATED: Lazy<Vec<HourlyVariable>> = Lazy::new(|| {
    let mut e = vec![
        HourlyVariable::Time,
        HourlyVariable::Temperature2m,
        HourlyVariable::RelativeHumidity2m,
        HourlyVariable::Dewpoint2m,
        HourlyVariable::ApparentTemperature,
        HourlyVariable::PressureMsl,
        HourlyVariable::SurfacePressure,
        HourlyVariable::CloudCover,
        HourlyVariable::CloudCoverLow,
        HourlyVariable::CloudCoverMid,
        HourlyVariable::CloudCoverHigh,
    ];

    e.extend(
        GroundLevel::enumerate()
            .iter()
            .cloned()
            .map(HourlyVariable::WindSpeed),
    );
    e.extend(
        GroundLevel::enumerate()
            .iter()
            .cloned()
            .map(HourlyVariable::WindDirection),
    );

    e.extend_from_slice(&[
        HourlyVariable::WindGusts10m,
        HourlyVariable::Precipitation,
        HourlyVariable::WeatherCode,
        HourlyVariable::SnowDepth,
        HourlyVariable::FreezingLevelHeight,
    ]);

    e.extend(
        PressureLevel::enumerate()
            .iter()
            .cloned()
            .map(HourlyVariable::PressureTemperature),
    );

    e.extend(
        PressureLevel::enumerate()
            .iter()
            .cloned()
            .map(HourlyVariable::PressureGeopotentialHeight),
    );

    e
});

static HOURLY_SERDE_NAMES: Lazy<Vec<&'static str>> = Lazy::new(|| {
    HourlyVariable::enumerate()
        .iter()
        .map(HourlyVariable::serde_name)
        .collect()
});

impl HourlyVariable {
    pub fn enumerate() -> &'static [Self] {
        HOURLY_ENUMERATED.as_slice()
    }

    fn from_serde_name(name: &str) -> Option<Self> {
        Self::enumerate()
            .iter()
            .find(|hv| hv.serde_name() == name)
            .cloned()
    }

    fn serde_name(&self) -> &'static str {
        match self {
            HourlyVariable::Time => "time",
            HourlyVariable::Temperature2m => "temperature_2m",
            HourlyVariable::RelativeHumidity2m => "relativehumidity_2m",
            HourlyVariable::Dewpoint2m => "dewpoint_2m",
            HourlyVariable::ApparentTemperature => "apparent_temperature",
            HourlyVariable::PressureMsl => "pressure_msl",
            HourlyVariable::SurfacePressure => "surface_pressure",
            HourlyVariable::CloudCover => "cloudcover",
            HourlyVariable::CloudCoverLow => "cloud_cover_low",
            HourlyVariable::CloudCoverMid => "cloud_cover_mid",
            HourlyVariable::CloudCoverHigh => "cloud_cover_high",
            HourlyVariable::WindSpeed(level) => WindSpeedField::name(level),
            HourlyVariable::WindDirection(level) => WindDirectionField::name(level),
            HourlyVariable::WindGusts10m => "windgusts_10m",
            HourlyVariable::Precipitation => "precipitation",
            HourlyVariable::WeatherCode => "weathercode",
            HourlyVariable::SnowDepth => "snow_depth",
            HourlyVariable::FreezingLevelHeight => "freezinglevel_height",
            HourlyVariable::PressureTemperature(level) => PressureTemperatureField::name(level),
            HourlyVariable::PressureGeopotentialHeight(level) => {
                PressureGeopotentialHeightField::name(level)
            }
        }
    }

    fn serde_names() -> &'static [&'static str] {
        HOURLY_SERDE_NAMES.as_slice()
    }
}

impl Serialize for HourlyVariable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.serde_name())
    }
}

impl<'de> Deserialize<'de> for HourlyVariable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HourlyVariableVisitor;

        impl<'de> Visitor<'de> for HourlyVariableVisitor {
            type Value = HourlyVariable;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Expecting one of: ")?;
                let names = HourlyVariable::enumerate()
                    .iter()
                    .map(HourlyVariable::serde_name)
                    .collect::<Vec<&str>>()
                    .join(", ");

                formatter.write_str(&names)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                HourlyVariable::enumerate()
                    .iter()
                    .find(|hv| hv.serde_name() == v)
                    .ok_or_else(|| {
                        E::custom(format!(
                            "{} does not match any valid HourlyVariable field names",
                            v
                        ))
                    })
                    .cloned()
            }
        }
        deserializer.deserialize_str(HourlyVariableVisitor)
    }
}

#[derive(EnumIter, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroundLevel {
    /// 10m above the ground.
    L10 = 10,
    /// 80m above the ground.
    L80 = 80,
    /// 120m above the ground.
    L120 = 120,
    /// 180m above the ground.
    L180 = 180,
}

impl GroundLevel {
    /// Height above the ground in meters.
    pub fn height(&self) -> f32 {
        match self {
            GroundLevel::L10 => 10.0,
            GroundLevel::L80 => 80.0,
            GroundLevel::L120 => 120.0,
            GroundLevel::L180 => 180.0,
        }
    }
}

static GROUND_LEVEL_VARIANTS: Lazy<Vec<GroundLevel>> = Lazy::new(|| GroundLevel::iter().collect());

impl Level for GroundLevel {
    fn enumerate() -> &'static [Self] {
        GROUND_LEVEL_VARIANTS.as_slice()
    }
}

/// Field definition for [`WindDirection`].
pub struct WindDirectionField;

static WIND_DIRECTION_NAMES: Lazy<HashMap<GroundLevel, String>> = Lazy::new(|| {
    GroundLevel::enumerate()
        .iter()
        .cloned()
        .map(|level| (level, format!("winddirection_{}m", level as u32)))
        .collect()
});

impl LevelField<GroundLevel> for WindDirectionField {
    fn name(level: &GroundLevel) -> &'static str {
        WIND_DIRECTION_NAMES.get(level).unwrap()
    }
}

/// Direction of the wind in degrees.
pub type WindDirection = LevelVariable<GroundLevel, WindDirectionField, Vec<f32>>;

/// Field definition for [`WindSpeed`].
pub struct WindSpeedField;

static WIND_SPEED_NAMES: Lazy<HashMap<GroundLevel, String>> = Lazy::new(|| {
    GroundLevel::enumerate()
        .iter()
        .cloned()
        .map(|level| (level, format!("windspeed_{}m", level as u32)))
        .collect()
});

impl LevelField<GroundLevel> for WindSpeedField {
    fn name(level: &GroundLevel) -> &'static str {
        WIND_SPEED_NAMES.get(level).unwrap()
    }
}

/// Speed of the wind.
pub type WindSpeed = LevelVariable<GroundLevel, WindSpeedField, Vec<f32>>;

#[derive(EnumIter, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PressureLevel {
    L1000 = 1000,
    L975 = 975,
    L950 = 950,
    L925 = 925,
    L900 = 900,
    L850 = 850,
    L800 = 800,
    L700 = 700,
    L600 = 600,
    L500 = 500,
    L400 = 400,
    L300 = 300,
    L250 = 250,
    L200 = 200,
    L150 = 150,
    L100 = 100,
    L70 = 70,
    L50 = 50,
    L30 = 30,
}

impl PressureLevel {
    /// Return pressure in `hPa`.
    pub fn pressure(&self) -> f32 {
        match self {
            Self::L1000 => 1000.0,
            Self::L975 => 975.0,
            Self::L950 => 950.0,
            Self::L925 => 925.0,
            Self::L900 => 900.0,
            Self::L850 => 850.0,
            Self::L800 => 800.0,
            Self::L700 => 700.0,
            Self::L600 => 600.0,
            Self::L500 => 500.0,
            Self::L400 => 400.0,
            Self::L300 => 300.0,
            Self::L250 => 250.0,
            Self::L200 => 200.0,
            Self::L150 => 150.0,
            Self::L100 => 100.0,
            Self::L70 => 70.0,
            Self::L50 => 50.0,
            Self::L30 => 30.0,
        }
    }
}

static PRESSURE_LEVEL_VARIANTS: Lazy<Vec<PressureLevel>> =
    Lazy::new(|| PressureLevel::iter().collect());

impl Level for PressureLevel {
    fn enumerate() -> &'static [Self] {
        PRESSURE_LEVEL_VARIANTS.as_slice()
    }
}

pub type PressureTemperature = LevelVariable<PressureLevel, PressureTemperatureField, Vec<f32>>;

#[derive(Debug)]
pub struct PressureTemperatureField;

static PRESSURE_TEMPERATURE_FIELD_NAMES: Lazy<HashMap<PressureLevel, String>> = Lazy::new(|| {
    PressureLevel::enumerate()
        .iter()
        .cloned()
        .map(|level| (level, format!("temperature_{}hPa", level as u32)))
        .collect()
});

impl LevelField<PressureLevel> for PressureTemperatureField {
    fn name(level: &PressureLevel) -> &'static str {
        PRESSURE_TEMPERATURE_FIELD_NAMES.get(level).unwrap()
    }
}

pub type PressureGeopotentialHeight =
    LevelVariable<PressureLevel, PressureGeopotentialHeightField, Vec<f32>>;

pub struct PressureGeopotentialHeightField;

static PRESSURE_GEOPOTENTIAL_HEIGHT_FIELD_NAMES: Lazy<HashMap<PressureLevel, String>> =
    Lazy::new(|| {
        PressureLevel::enumerate()
            .iter()
            .cloned()
            .map(|level| (level, format!("geopotential_height_{}hPa", level as u32)))
            .collect()
    });

impl LevelField<PressureLevel> for PressureGeopotentialHeightField {
    fn name(level: &PressureLevel) -> &'static str {
        PRESSURE_GEOPOTENTIAL_HEIGHT_FIELD_NAMES.get(level).unwrap()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Hourly {
    /// The times for the values in this struct's fields.
    // -   #[serde(deserialize_with = "naive_times_deserialize")]
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
    // #[serde(rename = "relativehumidity_2m")]
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
    pub cloud_cover: Option<Vec<f32>>,
    /// Low level cloud and fog cover up to 3 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    pub cloud_cover_low: Option<Vec<f32>>,
    /// Mid level cloud and fog cover 3 to 8 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    pub cloud_cover_mid: Option<Vec<f32>>,
    /// High level cloud and fog cover from 8 km altitude as an area fraction.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `%`
    pub cloud_cover_high: Option<Vec<f32>>,
    /// Wind speed at different heights above ground. Wind speed at 10 meters is the standard level.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `km/h (mph, m/s, knots)`
    pub wind_speed: WindSpeed,
    /// Wind direction at different heights above the ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°`
    pub wind_direction: WindDirection,
    /// Wind gust speed at 10m above the ground as a maximum of the preceding hour.
    ///
    /// + Valid time: `Preceding hour mean`
    /// + Unit: `km/h`
    pub wind_gusts_10m: Option<Vec<f32>>,
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
    pub freezing_level_height: Option<Vec<f32>>,
    /// Air temperature at the specified pressure level. Air temperatures decrease linearly with
    /// pressure.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `°C (°F)`
    pub pressure_temperature: PressureTemperature,
    /// Geopotential height at the specified pressure level. This can be used to get the correct
    /// altitude in meter above sea level of each pressure level. Be carefull not to mistake it
    /// with altitude above ground.
    ///
    /// + Valid time: `Instant`
    /// + Unit: `meter`
    pub pressure_geopotential_height: PressureGeopotentialHeight,
}

impl<'de> Deserialize<'de> for Hourly {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Deserialize date time in ISO8601 format without seconds or timezone.
        struct TimeDeserialize(NaiveDateTime);

        impl<'de> Deserialize<'de> for TimeDeserialize {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct StrVisitor;
                impl<'de> serde::de::Visitor<'de> for StrVisitor {
                    type Value = TimeDeserialize;

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
                            .map(TimeDeserialize)
                    }
                }

                deserializer.deserialize_str(StrVisitor)
            }
        }
        struct HourlyVisitor;

        impl<'de> Visitor<'de> for HourlyVisitor {
            type Value = Hourly;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Expecting one of: ")?;
                let expecting_names = HourlyVariable::serde_names().to_vec().join(", ");
                formatter.write_str(&expecting_names)
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut hourly = Hourly::default();
                let mut wind_speed_fields: HashMap<String, Vec<f32>> = HashMap::new();
                let mut wind_direction_fields: HashMap<String, Vec<f32>> = HashMap::new();
                let mut pressure_temperature_fields: HashMap<String, Vec<f32>> = HashMap::new();
                let mut pressure_geopotential_height_fields: HashMap<String, Vec<f32>> =
                    HashMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    if let Some(hv) = HourlyVariable::from_serde_name(&key) {
                        match hv {
                            HourlyVariable::Time => {
                                hourly.time = map
                                    .next_value::<Vec<TimeDeserialize>>()?
                                    .into_iter()
                                    .map(|t| t.0)
                                    .collect()
                            }
                            HourlyVariable::Temperature2m => {
                                hourly.temperature_2m = map.next_value()?;
                            }
                            HourlyVariable::RelativeHumidity2m => {
                                hourly.relative_humidity_2m = map.next_value()?;
                            }
                            HourlyVariable::Dewpoint2m => {
                                hourly.dewpoint_2m = map.next_value()?;
                            }
                            HourlyVariable::ApparentTemperature => {
                                hourly.apparent_temperature = map.next_value()?;
                            }
                            HourlyVariable::PressureMsl => {
                                hourly.pressure_msl = map.next_value()?;
                            }
                            HourlyVariable::SurfacePressure => {
                                hourly.surface_pressure = map.next_value()?;
                            }
                            HourlyVariable::CloudCover => {
                                hourly.cloud_cover = map.next_value()?;
                            }
                            HourlyVariable::CloudCoverLow => {
                                hourly.cloud_cover_low = map.next_value()?;
                            }
                            HourlyVariable::CloudCoverMid => {
                                hourly.cloud_cover_mid = map.next_value()?;
                            }
                            HourlyVariable::CloudCoverHigh => {
                                hourly.cloud_cover_high = map.next_value()?;
                            }
                            HourlyVariable::WindSpeed(_) => {
                                wind_speed_fields.insert(key.to_owned(), map.next_value()?);
                            }
                            HourlyVariable::WindDirection(_) => {
                                wind_direction_fields.insert(key.to_owned(), map.next_value()?);
                            }
                            HourlyVariable::WindGusts10m => {
                                hourly.wind_gusts_10m = map.next_value()?;
                            }
                            HourlyVariable::Precipitation => {
                                hourly.precipitation = map.next_value()?;
                            }
                            HourlyVariable::WeatherCode => {
                                hourly.weather_code = map.next_value()?;
                            }
                            HourlyVariable::SnowDepth => {
                                hourly.snow_depth = map.next_value()?;
                            }
                            HourlyVariable::FreezingLevelHeight => {
                                hourly.freezing_level_height = map.next_value()?;
                            }
                            HourlyVariable::PressureTemperature(_) => {
                                pressure_temperature_fields
                                    .insert(key.to_owned(), map.next_value()?);
                            }
                            HourlyVariable::PressureGeopotentialHeight(_) => {
                                pressure_geopotential_height_fields
                                    .insert(key.to_owned(), map.next_value()?);
                            }
                        }
                    } else {
                        return Err(serde::de::Error::unknown_field(
                            &key,
                            HourlyVariable::serde_names(),
                        ));
                    }
                }

                hourly.wind_speed = WindSpeed::deserialize(wind_speed_fields.into_deserializer())?;
                hourly.wind_direction =
                    WindDirection::deserialize(wind_direction_fields.into_deserializer())?;
                hourly.pressure_temperature = PressureTemperature::deserialize(
                    pressure_temperature_fields.into_deserializer(),
                )?;
                hourly.pressure_geopotential_height = PressureGeopotentialHeight::deserialize(
                    pressure_geopotential_height_fields.into_deserializer(),
                )?;

                Ok(hourly)
            }
        }
        deserializer.deserialize_any(HourlyVisitor)
    }
}

#[derive(Serialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DailyWeatherVariable {}

#[derive(Debug, PartialEq, Eq, Serialize)]
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

#[derive(Debug, PartialEq, Eq, Serialize)]
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

#[derive(Debug, PartialEq, Eq, Serialize)]
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

#[derive(Debug, PartialEq, Eq, Serialize)]
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

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, buildstructor::Builder)]
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
    pub time: chrono::NaiveDateTime,
    pub temperature: f32,
    #[serde(rename = "weathercode")]
    pub weather_code: WeatherCode,
    #[serde(rename = "windspeed")]
    pub wind_speed: f32,
    #[serde(rename = "winddirection")]
    pub wind_direction: u16,
}

#[derive(Debug, Clone, Deserialize)]
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

pub async fn obtain_forecast_json(
    client: &reqwest::Client,
    parameters: &ForecastParameters,
) -> Result<String, Error> {
    let query = serde_urlencoded::to_string(parameters)?;
    let url = format!("https://api.open-meteo.com/v1/forecast?{}", query);
    tracing::trace!("GET {}", url);

    let response = client.request(Method::GET, url).send().await?;

    if response.status().is_success() {
        response.text().await.map_err(Error::from)
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

pub async fn obtain_forecast(
    client: &reqwest::Client,
    parameters: &ForecastParameters,
) -> Result<Forecast, Error> {
    obtain_forecast_json(client, parameters)
        .await
        .and_then(|json| Ok(serde_json::from_str(&json)?))
}

#[cfg(test)]
mod test {
    use chrono::NaiveDate;
    use chrono_tz::Tz;
    use serde_json::json;

    use crate::{Forecast, GroundLevel, HourlyVariable};

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
        let forecast_json = json!({
          "elevation": 1050.0,
          "generationtime_ms": 0.5849599838256836,
          "hourly": {
            "windspeed_10m": [
                0.0,
                10.0,
            ],
            "windspeed_80m": [
                10.0,
                20.0,
            ],
            "winddirection_10m": [
                200.0,
                300.0,
            ],
            "winddirection_80m": [
                210.0,
                310.0,
            ],
            "freezinglevel_height": [
              2000.0,
              1980.0,
            ],
            "time": [
              "2022-10-04T00:00",
              "2022-10-04T01:00",
            ],
            "temperature_1000hPa": [
                10.0,
                12.0,
            ],
          },
          "hourly_units": {
            "freezinglevel_height": "m",
            "time": "iso8601"
          },
          "latitude": -43.375,
          "longitude": 170.25,
          "timezone": "Pacific/Auckland",
          "timezone_abbreviation": "NZDT",
          "utc_offset_seconds": 46800,
        });

        let forecast: Forecast = serde_json::from_value(forecast_json).unwrap();
        assert_eq!(1050.0, forecast.elevation);
        assert_eq!(0.5849599838256836, forecast.generation_time_ms);

        let hourly = forecast.hourly.unwrap();

        assert_eq!(
            vec![
                NaiveDate::from_ymd(2022, 10, 4).and_hms(0, 0, 0),
                NaiveDate::from_ymd(2022, 10, 4).and_hms(1, 0, 0)
            ],
            hourly.time
        );
        assert_eq!(
            &vec![0.0, 10.0],
            hourly.wind_speed.value(&GroundLevel::L10).unwrap()
        );
        assert_eq!(
            &vec![10.0, 20.0],
            hourly.wind_speed.value(&GroundLevel::L80).unwrap()
        );
        assert_eq!(
            &vec![200.0, 300.0],
            hourly.wind_direction.value(&GroundLevel::L10).unwrap()
        );
        assert_eq!(
            &vec![210.0, 310.0],
            hourly.wind_direction.value(&GroundLevel::L80).unwrap()
        );
        assert_eq!(vec![2000.0, 1980.0], hourly.freezing_level_height.unwrap());
        let expected_hourly_units = vec![
            (HourlyVariable::FreezingLevelHeight, "m"),
            (HourlyVariable::Time, "iso8601"),
        ]
        .into_iter()
        .map(|(hv, unit)| (hv, unit.to_owned()))
        .collect();
        assert_eq!(Some(expected_hourly_units), forecast.hourly_units);
        assert_eq!(-43.375, forecast.latitude);
        assert_eq!(170.25, forecast.longitude);
        assert_eq!(Tz::Pacific__Auckland, forecast.timezone);
        assert_eq!("NZDT", forecast.timezone_abbreviation);
        assert_eq!(46800, forecast.utc_offset_seconds);
    }
}
