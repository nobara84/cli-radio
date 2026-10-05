use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Station {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub favorite: bool,
}
impl Station {
    pub fn new(name: &str, url: &str) -> Result<Self, &'static str> {
        validate(name, url)?;
        Ok(Self {
            id: Uuid::new_v4(),
            name: name.trim().into(),
            url: url.trim().into(),
            favorite: false,
        })
    }
}
pub fn validate(name: &str, value: &str) -> Result<(), &'static str> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err("Name must be nonempty and contain no control characters");
    }
    let url = url::Url::parse(value.trim()).map_err(|_| "Invalid stream URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Use an HTTP or HTTPS stream URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Credentials in station URLs are not supported");
    }
    Ok(())
}
pub fn filtered(stations: &[Station], search: &str) -> Vec<usize> {
    let search = search.to_lowercase();
    let mut result: Vec<_> = stations
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.to_lowercase().contains(&search))
        .map(|(i, _)| i)
        .collect();
    result.sort_by_key(|&i| !stations[i].favorite);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn station_validation_and_filter() {
        assert!(Station::new("", "https://example.com").is_err());
        assert!(Station::new("A", "file:///etc/passwd").is_err());
        assert!(Station::new("A", "https://user:secret@example.com").is_err());
        let mut stations = vec![
            Station::new("Rock", "https://example.com/a").unwrap(),
            Station::new("ROCK 2", "http://example.com/b").unwrap(),
        ];
        stations[1].favorite = true;
        assert_eq!(filtered(&stations, "rock"), vec![1, 0]);
        stations.remove(1);
        assert_eq!(filtered(&stations, "2"), Vec::<usize>::new());
    }
}
