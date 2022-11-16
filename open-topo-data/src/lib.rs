use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dataset {
    #[serde(rename = "aster30m")]
    Aster,
    Etopo1,
    #[serde(rename = "eudem25m")]
    EuDem,
    Mapzen,
    #[serde(rename = "ned10m")]
    Ned,
    #[serde(rename = "nzdem8m")]
    NzDem,
    #[serde(rename = "srtm90m")]
    Srtm,
    #[serde(rename = "emod2018")]
    EmodBathymetry,
    #[serde(rename = "gebco2020")]
    GebcoBathymetry,
    #[serde(rename = "bkg200m")]
    Bkg,
    Swisstopo,
}

impl Dataset {
    const VARIANTS: &'static [Self] = &[
        Dataset::Aster,
        Dataset::Etopo1,
        Dataset::EuDem,
        Dataset::Mapzen,
        Dataset::Ned,
        Dataset::NzDem,
        Dataset::Srtm,
        Dataset::EmodBathymetry,
        Dataset::GebcoBathymetry,
        Dataset::Bkg,
        Dataset::Swisstopo,
    ];

    pub fn enumerate() -> &'static [Self] {
        Self::VARIANTS
    }
}

#[derive(Deserialize)]
struct ObtainResults {
    results: Vec<ObtainResult>,
    #[allow(unused)]
    status: String,
}

#[allow(unused)]
#[derive(Deserialize)]
struct ObtainResult {
    elevation: f32,
    location: Location,
    dataset: Dataset,
}

#[allow(unused)]
#[derive(Deserialize)]
struct Location {
    #[serde(rename = "lat")]
    latitude: f32,
    #[serde(rename = "lng")]
    longitude: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error deserializing elevation result")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Error performing http request")]
    Reqwest(#[from] reqwest::Error),
    #[error("No results in response")]
    NoResults,
}

pub struct Parameters {
    pub latitude: f32,
    pub longitude: f32,
    pub dataset: Dataset,
}

pub async fn obtain_elevation(
    client: &reqwest::Client,
    parameters: &Parameters,
) -> Result<f32, Error> {
    let url = format!(
        "https://api.opentopodata.org/v1/{}?locations={},{}",
        serde_json::to_value(&parameters.dataset)?.as_str().unwrap(),
        parameters.latitude,
        parameters.longitude,
    );
    let results: ObtainResults = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(results.results.get(0).ok_or(Error::NoResults)?.elevation)
}

#[cfg(test)]
mod tests {
    use crate::Dataset;

    #[test]
    fn test_serialize_datasets() {
        let expected_datasets = serde_json::json!([
            "aster30m",
            "etopo1",
            "eudem25m",
            "mapzen",
            "ned10m",
            "nzdem8m",
            "srtm90m",
            "emod2018",
            "gebco2020",
            "bkg200m",
            "swisstopo",
        ]);

        assert_eq!(
            expected_datasets,
            serde_json::to_value(Dataset::enumerate()).unwrap()
        )
    }
}
