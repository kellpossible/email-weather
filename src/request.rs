//! Parser for weather forecast requests.
//! See [`ForecastRequest`].

use std::str::FromStr;

use chumsky::{
    prelude::Simple,
    primitive::{choice, end, just},
    recovery::skip_until,
    text::{self, TextParser},
    Parser,
};
use color_eyre::Help;
use serde::{Deserialize, Serialize};

use crate::{
    gis::Position,
    process::{
        FormatDetail, FormatForecastOptions, LongFormatDetail, LongFormatStyle, ShortFormatDetail,
    },
};

/// A request for a weather forecast.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ForecastRequest {
    /// Requested forecast position.
    pub position: Option<Position>,
    /// Options for formatting the output message.
    pub format: FormatForecastOptions,
}

impl ForecastRequest {
    /// Parse request from a string.
    pub fn parse(request_string: &str) -> (Self, Vec<Simple<char>>) {
        let (request, errors) = request_parser().parse_recovery(request_string.to_uppercase());
        (request.unwrap_or_default(), errors)
    }
}

/// A parsed [`ForecastRequest`], with parsing errors stored alongside.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ParsedForecastRequest {
    /// The parsed request.
    pub request: ForecastRequest,
    /// Errors encountered while parsing the request.
    pub errors: Vec<String>,
}

impl ParsedForecastRequest {
    /// Parse request from a string.
    pub fn parse(request_string: &str) -> Self {
        let (request, errors) = ForecastRequest::parse(request_string);
        let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();

        if !errors.is_empty() {
            let error = errors
                .iter()
                .enumerate()
                .map(|(i, e)| format!("Error {}: {}", i, e))
                .collect::<Vec<String>>()
                .join("\n");
            tracing::warn!(
                "Errors while parsing request string {:?}:\n{}",
                request_string,
                error
            )
        }

        Self { request, errors }
    }
}

fn request_parser() -> impl Parser<char, ForecastRequest, Error = Simple<char>> {
    #[derive(Debug)]
    enum Expr {
        Position(Position),
        Format(FormatForecastOptions),
        Invalid,
    }

    fn fold_expr(mut request: ForecastRequest, expr: Expr) -> ForecastRequest {
        match expr {
            Expr::Position(position) => request.position = Some(position),
            Expr::Format(f) => request.format = f,
            Expr::Invalid => {}
        };
        request
    }

    let pos = position_parser()
        .map(Expr::Position)
        .recover_with(skip_until([' '], |_| Expr::Invalid));
    let fmt = format_parser()
        .map(Expr::Format)
        .recover_with(skip_until([' '], |_| Expr::Invalid));

    pos.or_not()
        .map(|expr_option| expr_option.into_iter().collect::<Vec<Expr>>())
        .then_ignore(just(' ').or_not())
        .chain(fmt.or_not())
        .map(|exprs| (ForecastRequest::default(), exprs))
        .foldl(fold_expr)
        .padded()
        .then_ignore(end().recover_with(skip_until([' '], |_| ())))
        .labelled("request")
}

/// Parses a long message format specification.
///
/// For example:
/// + `L` - Long with no specified style.
/// + `LH` - Long with [`LongFormatStyle::Html`] style.
/// + `LP` - Long with [`LongFormatStyle::PlainText`] style.
fn long_format_parser() -> impl Parser<char, LongFormatDetail, Error = Simple<char>> {
    let html_style = just('H').map(|_| LongFormatStyle::Html);
    let plain_style = just('P').map(|_| LongFormatStyle::PlainText);

    just('L')
        .ignore_then(choice((html_style, plain_style)).or_not())
        .map(|style| LongFormatDetail { style })
}

/// Parses a short message format specification.
///
/// For example:
/// + `S` - Short with no specified length limit.
/// + `S100` - Short with a length limit of 100.
fn short_format_parser() -> impl Parser<char, ShortFormatDetail, Error = Simple<char>> {
    let length_limit = text::int(10).try_map(|s: String, span| {
        s.parse::<usize>()
            .map_err(|e| Simple::custom(span, e.to_string()))
    });
    just('S')
        .ignore_then(length_limit.or_not())
        .map(|limit_option| {
            let mut short = ShortFormatDetail::default();
            short.length_limit = limit_option;
            short
        })
}

/// Parses a message format specification.
///
/// For example:
/// + `MS` - [`FormatDetail::Short`] message format detail. See [`short_format_parser()`] for more
///   variations.
/// + `ML` - [`FormatDetail::Long`] message format. See [`long_format_parser()`] for more
///   variations.
fn format_parser() -> impl Parser<char, FormatForecastOptions, Error = Simple<char>> {
    enum Expr {
        FormatDetail(FormatDetail),
    }

    fn fold_expr(mut options: FormatForecastOptions, expr: Expr) -> FormatForecastOptions {
        match expr {
            Expr::FormatDetail(detail) => options.detail = detail,
        };
        options
    }

    let format_ident = just('M');

    let short = short_format_parser().map(FormatDetail::Short);
    let long = long_format_parser().map(FormatDetail::Long);

    format_ident
        .ignore_then(choice((short, long)).map(Expr::FormatDetail).or_not())
        .map(|exprs| (FormatForecastOptions::default(), exprs))
        .foldl(fold_expr)
        .labelled("format")
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
    let left = text::digits::<char, Simple<char>>(10);

    // Right of the decimal point.
    let right = text::digits::<char, Simple<char>>(10);

    just('-')
        .or_not()
        .chain::<char, _, _>(left)
        .chain::<char, _, _>(just('.').chain(right).or_not().flatten())
        .collect::<String>()
        .from_str()
        .unwrapped()
        .labelled("number")
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
            }

            Ok(latitude)
        })
        .then_ignore(just(',').padded())
        .then(f32_parser().try_map(|longitude, span| {
            if longitude > 180.0 || longitude < -180.0 {
                return Err(Simple::custom(
                    span,
                    format!(
                        "Invalid longitude {}. It needs to be in the range [-180.0, 180.0]",
                        longitude
                    ),
                ));
            }

            Ok(longitude)
        }))
        .map(|(latitude, longitude)| Position::new(latitude, longitude))
        .labelled("position")
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
    use chumsky::{prelude::Simple, Parser};

    use crate::{
        gis::Position,
        process::{FormatDetail, FormatForecastOptions, LongFormatDetail, ShortFormatDetail},
        request::{format_parser, ParsedForecastRequest},
    };

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
        let p = position_parser().parse("53.035,158.654").unwrap();
        assert_eq!(Position::new(53.035, 158.654), p);
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
        assert_eq!(Vec::<Simple<char>>::new(), errors);
        assert_eq!(Some(Position::new(45.0, -24.0)), request.position);

        let (request, errors) = ForecastRequest::parse("45,-24 ML");
        assert_eq!(Vec::<Simple<char>>::new(), errors);
        assert_eq!(Some(Position::new(45.0, -24.0)), request.position);
        assert!(matches!(request.format.detail, FormatDetail::Long(_)));

        let parsed = ParsedForecastRequest::parse("-37.8245005,145.3032913");
        assert_eq!(Vec::<String>::new(), parsed.errors);
        assert_eq!(
            Some(Position::new(-37.8245005, 145.3032913)),
            parsed.request.position
        );
    }

    #[test]
    fn test_parse_empty_request() {
        let (request, errors) = ForecastRequest::parse("");
        assert_eq!(Vec::<Simple<char>>::new(), errors);
        assert!(request.position.is_none());

        let (request, errors) = ForecastRequest::parse(" ");
        assert_eq!(Vec::<Simple<char>>::new(), errors);
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

    #[test]
    fn test_parse_request_recover_position() {
        let (request, errors) = ForecastRequest::parse("-37.8245005,145.3032913 XXXXXX ");
        assert!(!errors.is_empty());
        assert_eq!(
            Some(Position::new(-37.8245005, 145.3032913)),
            request.position
        );

        let (request, errors) = ForecastRequest::parse("-37.8245005,145.3032913 XXXXXX");
        assert!(!errors.is_empty());
        assert_eq!(
            Some(Position::new(-37.8245005, 145.3032913)),
            request.position
        );
    }

    #[test]
    fn test_parse_request_recover_position_format() {
        let (request, errors) = ForecastRequest::parse("-37.8245005,145.3032913 ML LKJDFLSKDJF");
        assert!(!errors.is_empty());
        assert_eq!(
            Some(Position::new(-37.8245005, 145.3032913)),
            request.position
        );
        assert!(matches!(request.format.detail, FormatDetail::Long(_)));

        let (request, errors) = ForecastRequest::parse("-37.8245005,145.3032913 ML LKJDFLSKDJF ");
        assert!(!errors.is_empty());
        assert_eq!(
            Some(Position::new(-37.8245005, 145.3032913)),
            request.position
        );
        assert!(matches!(request.format.detail, FormatDetail::Long(_)));
    }

    #[test]
    fn test_parse_format_short_success() {
        let expected_format_options = FormatForecastOptions {
            detail: FormatDetail::Short(ShortFormatDetail::default()),
            ..FormatForecastOptions::default()
        };
        let format_options = format_parser().parse("MS").unwrap();
        assert_eq!(expected_format_options, format_options);
    }

    #[test]
    fn test_parse_format_long_success() {
        let expected_format_options = FormatForecastOptions {
            detail: FormatDetail::Long(LongFormatDetail::default()),
            ..FormatForecastOptions::default()
        };
        let format_options = format_parser().parse("ML").unwrap();
        assert_eq!(expected_format_options, format_options);
    }

    #[test]
    fn test_parse_format_short_limit_success() {
        let expected_format_options = FormatForecastOptions {
            detail: FormatDetail::Short(crate::process::ShortFormatDetail {
                length_limit: Some(1000),
            }),
            ..FormatForecastOptions::default()
        };
        let format_options = format_parser().parse("MS1000").unwrap();
        assert_eq!(expected_format_options, format_options);
    }
}
