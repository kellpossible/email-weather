use open_meteo::{obtain_forecast, Forecast, ForecastParameters, HourlyVariable, TimeZone};

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();

    let parameters = &ForecastParameters::builder()
        .latitude(-43.5138334)
        .longitude(170.3397567)
        .timezone(TimeZone::Auto)
        .hourly_entry(HourlyVariable::FreezingLevelHeight)
        .build();

    println!(
        "parameters: {}",
        serde_json::to_string_pretty(&parameters).unwrap()
    );
    let forecast: Forecast = obtain_forecast(&client, &parameters).await.unwrap();

    println!("{:#?}", &forecast)
}
