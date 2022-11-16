use serde::{Deserialize, Serialize};

use crate::{
    email,
    gis::Position,
    process::{FormatDetail, LongFormatStyle},
    receive::{self, from_account, message_id, text_body, ParseReceivedEmail},
    request::{ForecastRequest, ParsedForecastRequest},
};

/// A plain text email that was received.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Received {
    /// Address that this email was received from.
    pub from: email::Account,
    /// Identifier for the received message, will be used to specify the reply.
    pub message_id: Option<String>,
    /// Subject of the received email.
    pub subject: Option<String>,
    /// Requested forecast.
    pub forecast_request: ParsedForecastRequest,
}

impl receive::Received for Received {
    fn position(&self) -> Option<Position> {
        None
    }

    fn forecast_request(&self) -> &ParsedForecastRequest {
        &self.forecast_request
    }
}

impl ParseReceivedEmail for Received {
    type Err = eyre::Error;

    fn parse_email(message: mail_parser::Message) -> Result<Self, Self::Err> {
        let from = from_account(&message)?;
        let message_id = message_id(&message).map(|id| id.to_string());
        let subject = match message.get_header("Subject") {
            Some(subject_header) => match subject_header {
                mail_parser::HeaderValue::Text(text) => Some(text.to_string()),
                mail_parser::HeaderValue::Empty => None,
                _ => {
                    return Err(eyre::eyre!(
                        "Unexpected subject header: {:?}",
                        subject_header
                    ))
                }
            },
            None => None,
        };
        let body = text_body(&message)?.to_string();
        let trimmed_body = trim_body(&body);

        let mut forecast_request = ParsedForecastRequest::parse(trimmed_body);

        // Default to Html style if format detail is long.
        if let FormatDetail::Long(long) = &mut forecast_request.request.format.detail {
            if long.style.is_none() {
                long.style = Some(LongFormatStyle::Html);
            }
        }

        Ok(Self {
            from,
            message_id,
            subject,
            forecast_request,
        })
    }
}

/// Trim the body to only include the request line, removing extra newlines, and quoted replies.
fn trim_body<'a>(body: &'a str) -> &'a str {
    if let Some(first_non_whitespace_i) = body.find(|c: char| !c.is_whitespace()) {
        let request_content_onwards = if first_non_whitespace_i == 0 {
            body
        } else {
            body.split_at(first_non_whitespace_i).1
        };

        // assume that request_content_onwards contains at least one character given the
        // previous offset of -1 from first_non_whitespace_i
        let end_request_i = request_content_onwards
            .find('\n')
            .unwrap_or(request_content_onwards.len());

        request_content_onwards.split_at(end_request_i).0
    } else {
        body
    }
}

#[cfg(test)]
mod test {
    use crate::receive::ParseReceivedEmail;

    use super::{trim_body, Received};

    #[test]
    fn test_trim_body_with_reply() {
        let body = r#"-37.8245005,145.3032913
On Tue, Nov 15, 2022 at 5:55 PM <test.email.weather.service@gmail.com>
wrote:

> An error occurred while processing your request
>"#;
        let trimmed = trim_body(body);

        assert_eq!("-37.8245005,145.3032913", trimmed);
    }

    #[test]
    fn test_trim_body_no_reply() {
        let body = "\n-37.8245005,145.3032913";
        let trimmed = trim_body(body);

        assert_eq!("-37.8245005,145.3032913", trimmed);
    }

    #[test]
    fn test_parse_email() {
        let raw_message = r#"MIME-Version: 1.0
Date: Tue, 15 Nov 2022 17:55:01 +1100
Message-ID: <CAH+3HA1rdRyAyLW+-6zkHLW6UV2Y7bbK2h5Yujq-C6ydX3y1AQ@mail.gmail.com>
Subject: Forecast
From: Luke Frisken <l.frisken@gmail.com>
To: test.email.weather.service@gmail.com
Content-Type: multipart/alternative; boundary="00000000000022f34805ed7cd679"

--00000000000022f34805ed7cd679
Content-Type: text/plain; charset="UTF-8"

-37.8245005,145.3032913

--00000000000022f34805ed7cd679
Content-Type: text/html; charset="UTF-8"

<div dir="ltr">-37.8245005,145.3032913<br></div>

--00000000000022f34805ed7cd679--
"#;

        let message = mail_parser::Message::parse(raw_message.as_bytes()).unwrap();
        let received = Received::parse_email(message).unwrap();

        insta::assert_json_snapshot!(received, @r###"
        {
          "from": "Luke Frisken <l.frisken@gmail.com>",
          "message_id": "CAH+3HA1rdRyAyLW+-6zkHLW6UV2Y7bbK2h5Yujq-C6ydX3y1AQ@mail.gmail.com",
          "subject": "Forecast",
          "forecast_request": {
            "request": {
              "position": {
                "latitude": -37.8245,
                "longitude": 145.30328
              },
              "format": {
                "detail": {
                  "Short": {
                    "length_limit": null
                  }
                }
              }
            },
            "errors": []
          }
        }
        "###);
    }

    #[test]
    fn test_parse_email_reply() {
        let raw_message = r#"MIME-Version: 1.0
Date: Tue, 15 Nov 2022 17:57:11 +1100
References: <637337e8.170a0220.52bc.d228@mx.google.com>
In-Reply-To: <637337e8.170a0220.52bc.d228@mx.google.com>
Message-ID: <CAH+3HA0icQDCrB18R3EP5fr=ug8UNL1t1Q4jy6=o5f3sbmuM5g@mail.gmail.com>
Subject: Re: Forecast
From: Luke Frisken <l.frisken@gmail.com>
To: test.email.weather.service@gmail.com
Content-Type: multipart/alternative; boundary="000000000000e95f8505ed7cdda2"

--000000000000e95f8505ed7cdda2
Content-Type: text/plain; charset="UTF-8"

-37.8245005,145.3032913

On Tue, Nov 15, 2022 at 5:55 PM <test.email.weather.service@gmail.com>
wrote:

> An error occurred while processing your request
>

--000000000000e95f8505ed7cdda2
Content-Type: text/html; charset="UTF-8"
Content-Transfer-Encoding: quoted-printable

<div dir=3D"ltr">-37.8245005,145.3032913</div><br><div class=3D"gmail_quote=
"><div dir=3D"ltr" class=3D"gmail_attr">On Tue, Nov 15, 2022 at 5:55 PM &lt=
;<a href=3D"mailto:test.email.weather.service@gmail.com">test.email.weather=
.service@gmail.com</a>&gt; wrote:<br></div><blockquote class=3D"gmail_quote=
" style=3D"margin:0px 0px 0px 0.8ex;border-left:1px solid rgb(204,204,204);=
padding-left:1ex">An error occurred while processing your request<br>
</blockquote></div>

--000000000000e95f8505ed7cdda2--"#;

        let message = mail_parser::Message::parse(raw_message.as_bytes()).unwrap();
        let received = Received::parse_email(message).unwrap();
        insta::assert_json_snapshot!(received, @r###"
        {
          "from": "Luke Frisken <l.frisken@gmail.com>",
          "message_id": "CAH+3HA0icQDCrB18R3EP5fr=ug8UNL1t1Q4jy6=o5f3sbmuM5g@mail.gmail.com",
          "subject": "Re: Forecast",
          "forecast_request": {
            "request": {
              "position": {
                "latitude": -37.8245,
                "longitude": 145.30328
              },
              "format": {
                "detail": {
                  "Short": {
                    "length_limit": null
                  }
                }
              }
            },
            "errors": []
          }
        }
        "###);
    }
}
