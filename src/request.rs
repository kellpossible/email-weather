//! Parser for request strings

use chumsky::{prelude::Simple, primitive::filter, text, Parser};

use crate::gis::Position;

#[derive(Default)]
pub struct Request {
    /// Requested position.
    pub position: Option<Position>,
}

/// Valid Expressions
#[derive(Debug)]
enum Expr {
    Position(Position),
}

fn f32_parser() -> impl Parser<char, f32, Error = Simple<char>> {
    let left = text::int::<char, Simple<char>>(10).try_map(|s, span| {
        s.parse::<f32>()
            .map_err(|e| Simple::custom(span, format!("{}", e)))
    });

    let right = text::int::<char, Simple<char>>(10).try_map(|s, span| {
        let z = s.len();
        s.parse::<f32>()
            .map(|n| n * 10.0f32.powi(-1 * z as i32))
            .map_err(|e| Simple::custom(span, format!("{}", e)))
    });

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

#[cfg(test)]
mod test {
    use chumsky::Parser;

    use super::f32_parser;

    #[test]
    fn parse_f32_positive_no_fraction() {
        let f = f32_parser().parse("12345").unwrap();
        assert_eq!(12345.0f32, f)
    }

    #[test]
    fn parse_f32_negative_no_fraction() {
        let f = f32_parser().parse("-12345").unwrap();
        assert_eq!(-12345.0f32, f)
    }

    #[test]
    fn parse_f32_positive() {
        let f = f32_parser().parse("12345.23").unwrap();
        assert_eq!(12345.23f32, f)
    }

    #[test]
    fn parse_f32_negative() {
        let f = f32_parser().parse("-12345.23").unwrap();
        assert_eq!(-12345.23f32, f)
    }
}
