use crate::{
    network::Network,
    stations::{Station, validate},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub volume: u8,
    pub last_station: Option<Uuid>,
    pub network: Network,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 85,
            last_station: None,
            network: Network::default(),
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Database {
    stations: Vec<Station>,
}
pub struct Store {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub warnings: Vec<String>,
    pub writable: bool,
}
fn xdg(key: &str, fallback: &str) -> io::Result<PathBuf> {
    if let Some(v) = std::env::var_os(key).filter(|v| !v.is_empty()) {
        let p = PathBuf::from(v);
        if p.is_absolute() {
            return Ok(p.join("cli-radio"));
        }
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(fallback).join("cli-radio"))
        .ok_or_else(|| io::Error::other("HOME or absolute XDG paths required"))
}
impl Store {
    pub fn open() -> io::Result<Self> {
        let s = Self {
            config_dir: xdg("XDG_CONFIG_HOME", ".config")?,
            data_dir: xdg("XDG_DATA_HOME", ".local/share")?,
            state_dir: xdg("XDG_STATE_HOME", ".local/state")?,
            warnings: vec![],
            writable: true,
        };
        for dir in [&s.config_dir, &s.data_dir, &s.state_dir] {
            fs::create_dir_all(dir)?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(s)
    }
    pub fn load(&mut self) -> (Config, Vec<Station>) {
        let path = self.data_dir.join("stations.toml");
        // An existing empty database is intentional, never a reason to reseed.
        let missing =
            matches!(fs::symlink_metadata(&path), Err(e) if e.kind() == io::ErrorKind::NotFound);
        let (config, mut stations) = self.load_existing();
        if missing && self.writable {
            match crate::defaults::bundled() {
                Ok(defaults) => {
                    stations = defaults;
                    match self.write_stations(&stations, false) {
                        Ok(()) => {}
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                            return self.load_existing();
                        }
                        Err(_) => {
                            self.warnings.push("Cannot initialize personal station list; defaults loaded for this session only".into());
                            self.writable = false;
                        }
                    }
                }
                Err(_) => {
                    self.warnings
                        .push("Invalid bundled station list; personal files preserved".into());
                    self.writable = false;
                }
            }
        }
        (config, stations)
    }
    pub fn import_defaults(&mut self) -> io::Result<crate::defaults::ImportReport> {
        // Unlike normal startup, do not first seed: fresh imports report 9 added.
        let missing = matches!(fs::symlink_metadata(self.data_dir.join("stations.toml")), Err(e) if e.kind() == io::ErrorKind::NotFound);
        let (_, mut stations) = self.load_existing();
        if !self.writable {
            return Err(io::Error::other(
                "Import refused: repair configuration/database first; existing files preserved",
            ));
        }
        let report = crate::defaults::merge(&mut stations).map_err(io::Error::other)?;
        if report.imported > 0 {
            self.write_stations(&stations, !missing)?;
        }
        Ok(report)
    }
    fn load_existing(&mut self) -> (Config, Vec<Station>) {
        let config: Config = self.read(&self.config_dir.join("config.toml"));
        let database: Database = self.read(&self.data_dir.join("stations.toml"));
        let mut stations = Vec::new();
        for s in database.stations {
            if validate(&s.name, &s.url).is_err() || stations.iter().any(|v: &Station| v.id == s.id)
            {
                self.warnings
                    .push("Invalid/duplicate station skipped; source files preserved".into());
                self.writable = false;
            } else {
                stations.push(s);
            }
        }
        let mut config = config;
        config.volume = config.volume.min(100);
        (config, stations)
    }
    fn read<T: serde::de::DeserializeOwned + Default>(&mut self, path: &Path) -> T {
        match fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(v) => v,
                Err(_) => {
                    self.warnings.push("Invalid configuration/database; defaults loaded, files preserved. Repair files and restart to enable saving.".into());
                    self.writable = false;
                    T::default()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => T::default(),
            Err(_) => {
                self.warnings
                    .push("Cannot read configuration/database; files preserved".into());
                self.writable = false;
                T::default()
            }
        }
    }
    pub fn save(&self, config: &Config, stations: &[Station]) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::other(
                "Saving disabled until damaged files are repaired and application restarted",
            ));
        }
        let c = toml::to_string_pretty(config)
            .map_err(|_| io::Error::other("Configuration serialization failed"))?;
        atomic(&self.config_dir.join("config.toml"), c.as_bytes())?;
        self.write_stations(stations, true)
    }
    fn write_stations(&self, stations: &[Station], replace: bool) -> io::Result<()> {
        let text = toml::to_string_pretty(&Database {
            stations: stations.to_vec(),
        })
        .map_err(|_| io::Error::other("Station serialization failed"))?;
        atomic_write(
            &self.data_dir.join("stations.toml"),
            text.as_bytes(),
            replace,
        )
    }
}
pub fn atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write(path, bytes, true)
}
fn atomic_write(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if replace {
            fs::rename(&temp, path)?;
        } else {
            // Publish a complete file atomically without replacing a concurrently
            // created personal database. Both paths reside on the same filesystem.
            fs::hard_link(&temp, path)?;
            fs::remove_file(&temp)?;
        }
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_publication_never_overwrites_an_existing_database() {
        std::fs::create_dir_all("target/test-data").unwrap();
        let temp = tempfile::tempdir_in("target/test-data").unwrap();
        let path = temp.path().join("stations.toml");
        atomic(&path, b"stations = []").unwrap();
        let error = atomic_write(&path, b"new defaults", false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&path).unwrap(), "stations = []");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    #[test]
    fn defaults_and_roundtrip() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.volume, 85);
        let text = toml::to_string(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap().volume, 85);
        assert!(toml::from_str::<Config>("volume = 'bad'").is_err());
    }
    #[test]
    fn corruption_preserved_and_atomic_permissions() {
        let dir = {
            std::fs::create_dir_all("target/test-data").unwrap();
            tempfile::tempdir_in("target/test-data").unwrap()
        };
        let path = dir.path().join("config.toml");
        atomic(&path, b"broken = [").unwrap();
        let mut s = Store {
            config_dir: dir.path().into(),
            data_dir: dir.path().into(),
            state_dir: dir.path().into(),
            writable: true,
            warnings: vec![],
        };
        let (c, stations) = s.load();
        assert!(!s.writable);
        assert!(s.save(&c, &stations).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken = [");
        atomic(&path, b"volume = 42").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
