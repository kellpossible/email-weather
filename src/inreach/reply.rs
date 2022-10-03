use std::{borrow::Cow, collections::HashMap, convert::TryFrom};

use eyre::Context;
use reqwest::Response;
use serde::Serialize;
use uuid::Uuid;

struct Referral {
    ext_id: Uuid,
    adr: String,
}

impl TryFrom<&url::Url> for Referral {
    type Error = eyre::Error;

    fn try_from(url: &url::Url) -> Result<Self, Self::Error> {
        let map: HashMap<Cow<str>, Cow<str>> = url.query_pairs().collect();
        let ext_id: Uuid = map
            .get("extId")
            .ok_or_else(|| eyre::eyre!("extId query parameter is missing"))?
            .parse()
            .wrap_err("unable to parse extId query parameter")?;

        let adr: String = map
            .get("adr")
            .ok_or_else(|| eyre::eyre!("adr query parameter is missing"))
            .and_then(|adr| {
                let decoded = urlencoding::decode(adr).wrap_err("unable to decode adr")?;
                Ok(decoded.into_owned())
            })?;

        Ok(Self { ext_id, adr })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PostFormData<'a> {
    reply_address: &'a str,
    reply_message: &'a str,
    message_id: &'a str,
    guid: Uuid,
}

/// Extract message id from the GET response body
fn extract_message_id(html: &str) -> eyre::Result<String> {
    let document = scraper::Html::parse_document(&html);
    let selector =
        scraper::Selector::parse("#MessageId").expect("Unable to parse MessageId selector");
    let element_ref = document
        .select(&selector)
        .next()
        .ok_or_else(|| eyre::eyre!("Expected a #MessageId element to be present"))?;
    let element = element_ref.value();
    let message_id = element
        .attr("value")
        .ok_or_else(|| eyre::eyre!("#MessageId input is missing `value` attribute"))?;
    Ok(message_id.to_string())
}

pub async fn reply(
    client: &reqwest::Client,
    referral_url: &url::Url,
    message: &str,
) -> eyre::Result<()> {
    dbg!(&referral_url);

    let get_response = client
        .get(referral_url.clone())
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:105.0) Gecko/20100101 Firefox/105.0",
        )
        .send()
        .await
        .and_then(Response::error_for_status)
        .wrap_err("Error while performing GET request")?;

    let cookie = get_response
        .headers()
        .get("set-cookie")
        .ok_or_else(|| eyre::eyre!("Expected Cookie header to be present in GET response"))?
        .clone();

    let get_response_html: String = get_response
        .text()
        .await
        .wrap_err("Unable to decode GET response body")?;
    let message_id: String = extract_message_id(&get_response_html)?;

    if message_id.is_empty() {
        eyre::bail!("Invalid message id received from server");
    }

    let referral: Referral = referral_url
        .try_into()
        .wrap_err("Unable to parse referral url")?;

    let post_body: String = serde_urlencoded::to_string(PostFormData {
        reply_address: &referral.adr,
        reply_message: message,
        message_id: &message_id,
        guid: referral.ext_id,
    })
    .wrap_err("Unable to serialize POST form data")?;

    // println!("headers: {:?}", response.headers());

    // let request_context = response
    //     .headers()
    //     .get("request-context")
    //     .ok_or_else(|| eyre::eyre!("Expected request-context header to be present in response"))?
    //     .to_str().wrap_err("invalid request-context header unable to parse to utf8 string")?;
    //
    // let (key, value) = request_context.split_once('=').ok_or_else(|| eyre::eyre!("unexpected request-context header format: {:?}", request_context))?;

    let mut post_url = referral_url.clone();
    post_url.set_path("TextMessage/TxtMsg");
    post_url.set_query(None);

    let origin = post_url.origin().unicode_serialization();
    dbg!(&origin);
    let host = post_url
        .host_str()
        .ok_or_else(|| eyre::eyre!("Unable to parse host from post url"))?
        .to_string();
    dbg!(&host);
    let content_length = post_body.len();

    dbg!(&post_body);

    let post_response = client
        .post(post_url)
        .body(post_body)
        .header("Referrer-Policy", "strict-origin-when-cross-origin")
        .header("Cookie", cookie)
        .header("Accept", "*/*")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("Content-Length", content_length)
        .header(
            "Content-Type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        )
        .header("Referrer", referral_url.as_str())
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Host", host)
        .header("Origin", origin)
        .header("Pragma", "no-cache")
        .header("Sec-Fetch-Dest", "empty")
        .header("Sec-Fetch-Mode", "cors")
        .header("Sec-Fetch-Site", "same-origin")
        .header("DNT", "1")
        .send()
        .await
        // .and_then(Response::error_for_status)
        .wrap_err("Error while performing POST request")?;

    if !post_response.status().is_success() {
        eyre::bail!(
            "POST response status is not successful, code: {}, response body: {}",
            post_response.status(),
            post_response.text().await.unwrap_or_default()
        );
    }

    println!("POST status: {:?}", post_response.status());
    println!("POST response:\n{}", post_response.text().await?);

    Ok(())
}

#[cfg(test)]
pub mod test {
    use std::convert::TryFrom;

    use url::Url;
    use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

    use super::reply;
    use super::{extract_message_id, Referral};

    const GET_RESPONSE_BODY: &'static str = r#"
    <html>
        <body>
            <input data-val="true" data-val-number="The field MessageId must be a number." data-val-required="The MessageId field is required." id="MessageId" name="MessageId" type="hidden" value="66270435">
        </body>
    </html>
    "#;

    #[test]
    fn test_extract_message_id() {
        let message_id = extract_message_id(GET_RESPONSE_BODY).unwrap();
        assert_eq!("66270435", message_id);
    }

    #[test]
    fn test_parse_referral_url() {
        let url: Url = "https://aus.explore.garmin.com/textmessage/txtmsg?extId=08daa4e6-8eda-25c1-000d-3aa730600000&adr=email.weather.service%40gmail.com".parse().unwrap();

        let referral = Referral::try_from(&url).unwrap();
        assert_eq!(
            uuid::uuid!("08daa4e6-8eda-25c1-000d-3aa730600000"),
            referral.ext_id
        );
        assert_eq!("email.weather.service@gmail.com", referral.adr);
    }

    #[tokio::test]
    async fn test_reply() {
        let mock_server = MockServer::start().await;

        let mut referral_url: Url = mock_server.uri().parse().unwrap();
        referral_url.set_path("textmessage/txtmsg");
        referral_url.set_query(Some(
            "extId=08daa4e6-8eda-25c1-000d-3aa730600000&adr=email.weather.service%40gmail.com",
        ));

        Mock::given(matchers::method("GET"))
            .and(matchers::path("/textmessage/txtmsg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .insert_header("content-encoding", "br")
                    .insert_header("server", "cloudflare")
                    .insert_header("set-cookie", "BrowsingMode=Desktop; path=/")
                    .insert_header("vary", "Accept-Encoding")
                    .insert_header("access-control-expose-headers", "Request-Context")
                    .insert_header("cache-control", "private")
                    .insert_header("x-frame-options", "DENY")
                    .insert_header("cf-ray", "75427c6cb8f3a835-SYD")
                    .set_body_string(GET_RESPONSE_BODY),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let success_body: String =
            serde_json::to_string(&serde_json::json!({"Success": true})).unwrap();

        Mock::given(matchers::method("POST"))
            .and(matchers::path("/TextMessage/TxtMsg"))
            .and(matchers::body_string(
                "ReplyAddress=email.weather.service%40gmail.com&\
                    ReplyMessage=Unit+Test+message%2C+from+Luke&\
                    MessageId=66270435&\
                    Guid=08daa4e6-8eda-25c1-000d-3aa730600000",
            ))
            .and(matchers::header("Referrer", referral_url.as_str()))
            .and(matchers::header(
                "Host",
                referral_url.host().unwrap().to_string().as_str(),
            ))
            .and(matchers::header(
                "Origin",
                referral_url.origin().unicode_serialization().as_str(),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json; charset=utf-8")
                    .insert_header("content-encoding", "br")
                    .set_body_string(success_body),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        reply(&client, &referral_url, "Unit Test message, from Luke")
            .await
            .unwrap();
    }
}
