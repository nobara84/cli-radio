use crate::stations::{Station, validate};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
struct BundledStation {
    name: String,
    url: String,
    #[serde(default)]
    favorite: bool,
}
#[derive(Deserialize)]
struct BundledDatabase {
    stations: Vec<BundledStation>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
}
pub fn bundled() -> Result<Vec<Station>, &'static str> {
    let database: BundledDatabase = toml::from_str(include_str!("../config/default-stations.toml"))
        .map_err(|_| "Invalid bundled station list")?;
    database
        .stations
        .into_iter()
        .map(|s| {
            let mut station = Station::new(&s.name, &s.url)?;
            station.favorite = s.favorite;
            Ok(station)
        })
        .collect()
}
/// Preserve scheme and query (different streams), normalize URL syntax and
/// insignificant trailing slashes, and ignore client-side fragments.
pub fn normalized_url(value: &str) -> Result<String, &'static str> {
    validate("Station", value)?;
    let mut url = url::Url::parse(value.trim()).map_err(|_| "Invalid stream URL")?;
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(if path.is_empty() { "/" } else { &path });
    Ok(url.to_string())
}
pub fn merge(stations: &mut Vec<Station>) -> Result<ImportReport, &'static str> {
    let mut known: HashSet<String> = stations
        .iter()
        .map(|s| normalized_url(&s.url))
        .collect::<Result<_, _>>()?;
    let defaults = bundled()?;
    let mut report = ImportReport {
        imported: 0,
        skipped: 0,
    };
    for station in defaults {
        if known.insert(normalized_url(&station.url)?) {
            stations.push(station);
            report.imported += 1;
        } else {
            report.skipped += 1;
        }
    }
    Ok(report)
}
