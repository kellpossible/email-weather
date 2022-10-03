use email_weather;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let referral_url: url::Url = "https://aus.explore.garmin.com/textmessage/txtmsg?extId=08daa4e6-8eda-25c1-000d-3aa730600000&adr=email.weather.service%40gmail.com".parse().unwrap();

    let client = reqwest::Client::new();

    let message = "Test From Rust";

    email_weather::inreach::reply::reply(&client, &referral_url, message).await?;
    
    Ok(())
}
