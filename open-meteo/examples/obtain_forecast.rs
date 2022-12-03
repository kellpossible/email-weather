use open_meteo::{
    obtain_forecast, obtain_forecast_json, Forecast, ForecastParameters, GroundLevel,
    HourlyVariable, TimeZone,
};

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();

    let parameters = &ForecastParameters::builder()
        .latitude(-43.75905)
        .longitude(170.115)
        .hourly_entry(HourlyVariable::FreezingLevelHeight)
        .hourly_entry(HourlyVariable::WindSpeed(GroundLevel::L10))
        .hourly_entry(HourlyVariable::WindDirection(GroundLevel::L10))
        .hourly_entry(HourlyVariable::WeatherCode)
        .hourly_entry(HourlyVariable::Precipitation)
        .timezone(TimeZone::Auto)
        .build();

    println!(
        "parameters: {}",
        serde_json::to_string_pretty(&parameters).unwrap()
    );
    let forecast_json: String = obtain_forecast_json(&client, &parameters).await.unwrap();
    println!("{}", forecast_json);
    let forecast: Forecast = obtain_forecast(&client, &parameters).await.unwrap();
    println!("{:#?}", &forecast)
}
