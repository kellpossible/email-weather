#[tokio::main]
async fn main() {
    let http_client = reqwest::Client::new();

    let terrain_elevation = open_topo_data::obtain_elevation(
        &http_client,
        &open_topo_data::Parameters {
            latitude: -43.513832,
            longitude: 170.33975,
            dataset: open_topo_data::Dataset::Mapzen,
        },
    )
    .await
    .unwrap();

    dbg!(terrain_elevation);
}
