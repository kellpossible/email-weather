//! Parsing emails received from an inreach device.

use eyre::Context;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// An email received from an inreach device.
#[derive(Deserialize, Serialize, Debug)]
pub struct Email {
    /// The name of the person who sent the message.
    pub from_name: String,
    /// The url used to send a reply to the message via the inreach web interface.
    pub referral_url: url::Url,
    /// Latitude in degrees of inreach device when it sent the message.
    pub latitude: f32,
    /// Longitude in degrees of inreach device when it sent the message.
    pub longitude: f32,
}

static VIEW_LOCATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"View the location or send a reply to (.*)[:]").unwrap());
static MESSAGE_FROM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(.*) sent this message from: Lat (.*) Lon (.*)").unwrap());

#[derive(PartialEq)]
enum ParseState {
    ViewLocation,
    ReferralUrl,
    MessageFrom,
    Done,
}

impl FromStr for Email {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut from_name: Option<String> = None;
        let mut referral_url: Option<url::Url> = None;
        let mut latitude: Option<f32> = None;
        let mut longitude: Option<f32> = None;
        let mut parse_state = ParseState::ViewLocation;

        for line in s.split('\n') {
            match parse_state {
                ParseState::ViewLocation => {
                    if let Some(c) = (*VIEW_LOCATION_RE).captures(line.trim()) {
                        let name_match = c.get(1).unwrap();
                        from_name = Some(name_match.as_str().to_string());
                        parse_state = ParseState::ReferralUrl;
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

        Ok(Self {
            from_name: from_name.unwrap(),
            referral_url: referral_url.unwrap(),
            latitude: latitude.unwrap(),
            longitude: longitude.unwrap(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::Email;

    const TEST_BODY: &'static str = r#"
Test

View the location or send a reply to Luke Frisken:
https://aus.explore.garmin.com/textmessage/txtmsg?extId=000aa0e6-8e00-2501-000d-3aa730600000&adr=email.weather.service%40gmail.com

Luke Frisken sent this message from: Lat -44.689529 Lon 169.132354

Do not reply directly to this message.

This message was sent to you using the inReach two-way satellite communicator with GPS. To
learn more, visit http://explore.garmin.com/inreach.
    "#;
    #[test]
    fn test_parse_email() {
        let email: Email = TEST_BODY.parse().unwrap();

        assert_eq!("Luke Frisken", email.from_name);
        assert_eq!(
            "https://aus.explore.garmin.com/textmessage/txtmsg?extId=000aa0e6-8e00-2501-000d-3aa730600000&adr=email.weather.service%40gmail.com",
            email.referral_url.as_str()
        );
        assert_eq!(-44.689529, email.latitude);
        assert_eq!(169.132354, email.longitude);
    }
}
