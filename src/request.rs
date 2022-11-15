//! Parser for weather forecast requests.
//! See [`ForecastRequest`].

use std::str::FromStr;

use chumsky::{
    prelude::Simple,
    primitive::{end, filter},
    text::{self, TextParser},
    Parser,
};
use color_eyre::Help;
use serde::{Deserialize, Serialize};

use crate::gis::Position;

/// A request for a weather forecast.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ForecastRequest {
    /// Requested forecast position.
    pub position: Option<Position>,
}

impl ForecastRequest {
    /// Parse request from a string.
    pub fn parse(request_string: &str) -> (Self, Vec<Simple<char>>) {
        let (request, errors) = request_parser().parse_recovery(request_string);
        (request.unwrap_or_default(), errors)
    }
}

/// A parsed [`ForecastRequest`], with parsing errors stored alongside.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ParsedForecastRequest {
    pub request: ForecastRequest,
    pub errors: Vec<String>,
}

impl ParsedForecastRequest {
    /// Parse request from a string.
    pub fn parse(request_string: &str) -> Self {
        tracing::debug!("Parsing request string: {}", request_string);
        let (request, errors) = ForecastRequest::parse(request_string);
        let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();

        Self { request, errors }
    }
}

fn request_parser() -> impl Parser<char, ForecastRequest, Error = Simple<char>> {
    position_parser()
        .padded()
        .or_not()
        .map(|position| {
            let mut request = ForecastRequest::default();
            request.position = position;
            request
        })
        .then_ignore(end())
}

/// Parses 32bit floating point numbers:
///
/// e.g:
/// + `1.01234`
/// + `-0.345`
/// + `1246`
/// + `-12345`
fn f32_parser() -> impl Parser<char, f32, Error = Simple<char>> {
    // Left of the decimal point.
    let left = text::int::<char, Simple<char>>(10).try_map(|s, span| {
        s.parse::<f32>()
            .map_err(|e| Simple::custom(span, format!("{}", e)))
    });

    // Right of the decimal point.
    let right = text::int::<char, Simple<char>>(10).try_map(|s, span| {
        let z = s.len();
        s.parse::<f32>()
            .map(|n| n * 10.0f32.powi(-1 * z as i32))
            .map_err(|e| Simple::custom(span, format!("{}", e)))
    });

    // Negative `-` sign.
    let neg = filter(|c: &char| c == &'-').or_not().map(|o| o.is_some());

    neg.then(
        left.then(
            filter(|c: &char| c == &'.')
                .ignored()
                .then(right)
                .or_not()
                .map(|o| o.map(|(_ignored, right)| right)),
        )
        .map(|(left, right)| right.map(|right| right + left).unwrap_or(left)),
    )
    .map(|(neg, num)| if neg { -num } else { num })
}

fn position_parser() -> impl Parser<char, Position, Error = Simple<char>> {
    f32_parser()
        .try_map(|latitude, span| {
            if latitude > 90.0 || latitude < -90.0 {
                return Err(Simple::custom(
                    span,
                    format!(
                        "Invalid latitude {}. It needs to be in the range [-90.0, 90.0]",
                        latitude
                    ),
                ));
            } else {
                Ok(latitude)
            }
        })
        .then_ignore(filter(|c: &char| c == &',').padded())
        .then(f32_parser().try_map(|longitude, span| {
            if longitude > 180.0 || longitude < -180.0 {
                return Err(Simple::custom(
                    span,
                    format!(
                        "Invalid longitude {}. It needs to be in the range [-180.0, 180.0]",
                        longitude
                    ),
                ));
            } else {
                Ok(longitude)
            }
        }))
        .map(|(latitude, longitude)| Position::new(latitude, longitude))
}

/// Convert parsing errors to an eyre formatted error.
pub fn errors_to_eyre(errors: Vec<Simple<char>>) -> eyre::Error {
    let mut errors_formatted = String::new();
    for (i, error) in errors.into_iter().enumerate() {
        errors_formatted.push_str(&format!("Error {}: {:#}, ", i, error))
    }
    eyre::eyre!("Error parsing Position from string. {}", errors_formatted)
}

impl FromStr for Position {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        position_parser()
            .then_ignore(end())
            .parse(s)
            .map_err(|errors| {
                errors_to_eyre(errors)
                    .suggestion("Expected a latitude,longitude in degrees like: `-24.0,45.0`")
            })
    }
}

#[cfg(test)]
mod test {
    use chumsky::Parser;

    use crate::{gis::Position, request::ParsedForecastRequest};

    use super::{f32_parser, position_parser, ForecastRequest};

    #[test]
    fn test_parse_f32_positive_no_fraction() {
        let f = f32_parser().parse("12345").unwrap();
        assert_eq!(12345.0f32, f)
    }

    #[test]
    fn test_parse_f32_negative_no_fraction() {
        let f = f32_parser().parse("-12345").unwrap();
        assert_eq!(-12345.0f32, f)
    }

    #[test]
    fn test_parse_f32_positive() {
        let f = f32_parser().parse("12345.23").unwrap();
        assert_eq!(12345.23f32, f)
    }

    #[test]
    fn test_parse_f32_negative() {
        let f = f32_parser().parse("-12345.23").unwrap();
        assert_eq!(-12345.23f32, f)
    }

    #[test]
    fn test_parse_position_success() {
        let p = position_parser().parse("42.245,-100.1").unwrap();
        assert_eq!(Position::new(42.245, -100.1), p);
        let p = position_parser().parse("42.245, -100.1").unwrap();
        assert_eq!(Position::new(42.245, -100.1), p);
        let p = position_parser().parse("42.245 ,-100.1").unwrap();
        assert_eq!(Position::new(42.245, -100.1), p);
        let p = position_parser().parse("42.245 , -100.1").unwrap();
        assert_eq!(Position::new(42.245, -100.1), p);
        let p = position_parser().parse("42,100").unwrap();
        assert_eq!(Position::new(42.0, 100.0), p);
    }

    #[test]
    fn test_parse_position_out_of_bounds() {
        assert!(position_parser().parse("100.0,40.0").is_err());
        assert!(position_parser().parse("-100.0,40.0").is_err());
        assert!(position_parser().parse("40.0,200.0").is_err());
        assert!(position_parser().parse("40.0,-200.0").is_err());
    }

    #[test]
    fn test_parse_request() {
        let (request, errors) = ForecastRequest::parse("45,-24");
        assert!(errors.is_empty());
        assert_eq!(Some(Position::new(45.0, -24.0)), request.position);
        let parsed = ParsedForecastRequest::parse("-37.8245005,145.3032913");
        assert!(parsed.errors.is_empty());
        assert_eq!(
            Some(Position::new(-37.8245005, 145.3032913)),
            parsed.request.position
        );
    }

    #[test]
    fn test_parse_empty_request() {
        let (request, errors) = ForecastRequest::parse("");
        assert!(errors.is_empty());
        assert!(request.position.is_none());
    }

    #[test]
    fn test_parse_request_errors() {
        let (request, errors) = ForecastRequest::parse("100.0,40");
        assert!(request.position.is_none());
        assert_eq!(1, errors.len());

        let (request, errors) = ForecastRequest::parse("12l3kjlkdfsh,lskjdfsl");
        assert!(request.position.is_none());
        assert_eq!(1, errors.len());
    }
}
