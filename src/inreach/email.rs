//! Parsing emails received from an inreach device.

use eyre::Context;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::{
    gis::Position,
    receive::{self, text_body, ParseReceivedEmail},
    request::{ForecastRequest, ParsedForecastRequest},
};

/// An email received from an inreach device.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct Received {
    /// The name of the person who sent the message.
    /// TODO: remove as part of anonymizing #12
    pub from_name: String,
    /// The url used to send a reply to the message via the inreach web interface.
    pub referral_url: url::Url,
    /// The position of the inreach device at the time that the message was sent.
    pub position: Position,
    /// Weather forecast request.
    pub forecast_request: ParsedForecastRequest,
}

impl receive::Received for Received {
    fn position(&self) -> Option<Position> {
        Some(self.position.clone())
    }

    fn forecast_request(&self) -> &ParsedForecastRequest {
        todo!()
    }
}

static VIEW_LOCATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"View the location or send a reply to (.*)[:]").unwrap());
static MESSAGE_FROM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(.*) sent this message from: Lat (.*) Lon (.*)").unwrap());

#[derive(PartialEq)]
enum ParseState {
    MessageBody,
    ReferralUrl,
    MessageFrom,
    Done,
}

impl ParseReceivedEmail for Received {
    type Err = eyre::Error;

    fn parse_email(message: mail_parser::Message) -> Result<Self, Self::Err> {
        let body = text_body(&message)?;
        Self::parse(body)
    }
}

impl Received {
    fn parse<'a>(body: Cow<'a, str>) -> Result<Self, eyre::Error> {
        let mut from_name: Option<String> = None;
        let mut referral_url: Option<url::Url> = None;
        let mut latitude: Option<f32> = None;
        let mut longitude: Option<f32> = None;
        let mut parse_state = ParseState::MessageBody;
        let mut message_body = String::with_capacity(body.len());

        for line in body.split('\n') {
            match parse_state {
                ParseState::MessageBody => {
                    if let Some(c) = (*VIEW_LOCATION_RE).captures(line.trim()) {
                        let name_match = c.get(1).unwrap();
                        from_name = Some(name_match.as_str().to_string());
                        parse_state = ParseState::ReferralUrl;
                        if message_body.len() > 0 {
                            // Remove last empty newline
                            if message_body.chars().last() == Some('\n') {
                                message_body.remove(
                                    message_body
                                        .char_indices()
                                        .last()
                                        .expect("Expected there to be a last character")
                                        .0,
                                );
                            }
                        }
                    } else {
                        message_body.push_str(line);
                    }
                }
                ParseState::ReferralUrl => {
                    referral_url = Some(
                        line.trim()
                            .parse()
                            .wrap_err("unable to parse referral url")?,
                    );
                    parse_state = ParseState::MessageFrom;
                }
                ParseState::MessageFrom => {
                    if let Some(captures) = (*MESSAGE_FROM_RE).captures(line.trim()) {
                        latitude = Some(
                            captures
                                .get(2)
                                .unwrap()
                                .as_str()
                                .parse()
                                .wrap_err("unable to parse latitude")?,
                        );
                        longitude = Some(
                            captures
                                .get(3)
                                .unwrap()
                                .as_str()
                                .parse()
                                .wrap_err("unable to parse longitude")?,
                        );

                        parse_state = ParseState::Done;
                    }
                }
                ParseState::Done => break,
            }
        }

        if parse_state != ParseState::Done {
            eyre::bail!("Unable to parse email text as a complete inreach message")
        }

        let forecast_request = ParsedForecastRequest::parse(&message_body);

        Ok(Self {
            from_name: from_name.unwrap(),
            referral_url: referral_url.unwrap(),
            position: Position::new(latitude.unwrap(), longitude.unwrap()),
            forecast_request,
        })
    }
}

#[cfg(test)]
mod test {
    use super::Received;

    const TEST_BODY: &'static str = r#"
-37.8245005,145.3032913

View the location or send a reply to Luke Frisken:
https://aus.explore.garmin.com/textmessage/txtmsg?extId=000aa0e6-8e00-2501-000d-3aa730600000&adr=email.weather.service%40gmail.com

Luke Frisken sent this message from: Lat -44.689529 Lon 169.132354

Do not reply directly to this message.

This message was sent to you using the inReach two-way satellite communicator with GPS. To
learn more, visit http://explore.garmin.com/inreach.
    "#;
    #[test]
    fn test_parse_email() {
        let email = Received::parse(TEST_BODY.into()).unwrap();

        insta::assert_json_snapshot!(email, @r###"
        {
          "from_name": "Luke Frisken",
          "referral_url": "https://aus.explore.garmin.com/textmessage/txtmsg?extId=000aa0e6-8e00-2501-000d-3aa730600000&adr=email.weather.service%40gmail.com",
          "position": {
            "latitude": -44.68953,
            "longitude": 169.13235
          },
          "forecast_request": {
            "request": {
              "position": {
                "latitude": -37.8245,
                "longitude": 145.30328
              }
            },
            "errors": []
          }
        }
        "###);
    }
}
