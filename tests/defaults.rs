use cli_radio::{
    config::{Config, Store},
    defaults::{self, ImportReport},
    stations::Station,
};
use std::{collections::HashSet, fs, process::Command};

fn fixture() -> (Store, tempfile::TempDir) {
    fs::create_dir_all("target/test-data").unwrap();
    let temp = tempfile::tempdir_in("target/test-data").unwrap();
    let root = temp.path().canonicalize().unwrap();
    let store = Store {
        config_dir: root.join("config"),
        data_dir: root.join("data"),
        state_dir: root.join("state"),
        warnings: vec![],
        writable: true,
    };
    for dir in [&store.config_dir, &store.data_dir, &store.state_dir] {
        fs::create_dir_all(dir).unwrap();
    }
    (store, temp)
}
#[test]
fn bundled_nine_stations_parse_exact_names_urls_and_unique_ids() {
    let stations = defaults::bundled().unwrap();
    let expected = [
        (
            "Radio BOB! National",
            "http://streams.radiobob.de/bob-national/mp3-192/mediaplayer",
        ),
        (
            "Radio BOB! Best of Rock",
            "http://streams.radiobob.de/bob-bestofrock/mp3-192/mediaplayer",
        ),
        (
            "Radio BOB! 80s Rock",
            "http://streams.radiobob.de/bob-80srock/mp3-192/mediaplayer",
        ),
        (
            "Radio BOB! 90s Rock",
            "http://streams.radiobob.de/bob-90srock/mp3-192/mediaplayer",
        ),
        (
            "Radio BOB! 2000er Rock",
            "http://streams.radiobob.de/2000er/mp3-192/mediaplayer/",
        ),
        (
            "Radio BOB! Gaming Rock",
            "https://streams.radiobob.de/gamingrock/mp3-192/",
        ),
        (
            "Bayern 1 Oberfranken",
            "https://dispatcher.rndfnk.com/br/br1/franken/mp3/mid",
        ),
        (
            "ANTENNE BAYERN",
            "https://mp3channels.webradio.antenne.de/antenne",
        ),
        (
            "Radio Bamberg",
            "https://webstream.radio-bamberg.de/radio-bamberg.mp3",
        ),
    ];
    assert_eq!(stations.len(), 9);
    for (station, (name, url)) in stations.iter().zip(expected) {
        assert_eq!(station.name, name);
        assert_eq!(station.url, url);
        assert!(!station.favorite);
    }
    assert_eq!(
        stations.iter().map(|s| s.id).collect::<HashSet<_>>().len(),
        9
    );
    assert_ne!(stations[0].id, defaults::bundled().unwrap()[0].id);
}
#[test]
fn first_run_persists_defaults_with_stable_personal_ids() {
    let (mut store, _temp) = fixture();
    let (_, first) = store.load();
    assert_eq!(first.len(), 9);
    assert!(store.data_dir.join("stations.toml").exists());
    assert!(!store.config_dir.join("config.toml").exists());
    let (_, second) = store.load();
    assert_eq!(
        first.iter().map(|s| s.id).collect::<Vec<_>>(),
        second.iter().map(|s| s.id).collect::<Vec<_>>()
    );
}
#[test]
fn existing_personal_database_is_unchanged() {
    let (mut store, _temp) = fixture();
    let mut own = Station::new(
        "My BOB",
        "http://streams.radiobob.de/bob-national/mp3-192/mediaplayer",
    )
    .unwrap();
    own.favorite = true;
    store.save(&Config::default(), &[own.clone()]).unwrap();
    let path = store.data_dir.join("stations.toml");
    let before = fs::read(&path).unwrap();
    let (_, stations) = store.load();
    assert_eq!(stations.len(), 1);
    assert_eq!(stations[0].id, own.id);
    assert!(stations[0].favorite);
    assert_eq!(fs::read(path).unwrap(), before);
}
#[test]
fn deleted_defaults_and_explicitly_empty_database_stay_deleted() {
    let (mut store, _temp) = fixture();
    let (config, mut stations) = store.load();
    let removed = stations.remove(0).id;
    store.save(&config, &stations).unwrap();
    let (_, reloaded) = store.load();
    assert_eq!(reloaded.len(), 8);
    assert!(reloaded.iter().all(|s| s.id != removed));
    store.save(&config, &[]).unwrap();
    assert!(store.load().1.is_empty());
}
#[test]
fn import_adds_only_missing_urls_preserves_existing_favorites_and_config() {
    let (mut store, _temp) = fixture();
    let mut existing = Station::new(
        "My favorite",
        " HTTP://STREAMS.RADIOBOB.DE:80/bob-national/mp3-192/mediaplayer/#ignored ",
    )
    .unwrap();
    existing.favorite = true;
    store
        .save(
            &Config {
                volume: 35,
                last_station: Some(existing.id),
                ..Default::default()
            },
            &[existing.clone()],
        )
        .unwrap();
    let config_before = fs::read(store.config_dir.join("config.toml")).unwrap();
    assert_eq!(
        store.import_defaults().unwrap(),
        ImportReport {
            imported: 8,
            skipped: 1
        }
    );
    let (_, stations) = store.load();
    assert_eq!(stations.len(), 9);
    assert_eq!(stations[0].id, existing.id);
    assert_eq!(stations[0].name, existing.name);
    assert_eq!(stations[0].url, existing.url);
    assert!(stations[0].favorite);
    assert_eq!(
        stations.iter().map(|s| s.id).collect::<HashSet<_>>().len(),
        9
    );
    assert_eq!(
        fs::read(store.config_dir.join("config.toml")).unwrap(),
        config_before
    );
    let before = fs::read(store.data_dir.join("stations.toml")).unwrap();
    assert_eq!(
        store.import_defaults().unwrap(),
        ImportReport {
            imported: 0,
            skipped: 9
        }
    );
    assert_eq!(
        fs::read(store.data_dir.join("stations.toml")).unwrap(),
        before
    );
}
#[test]
fn normalized_urls_preserve_distinct_streams() {
    assert_eq!(
        defaults::normalized_url(" HTTP://EXAMPLE.ORG:80/live/#title ").unwrap(),
        "http://example.org/live"
    );
    assert_ne!(
        defaults::normalized_url("http://example.org/live").unwrap(),
        defaults::normalized_url("https://example.org/live").unwrap()
    );
    assert_ne!(
        defaults::normalized_url("https://example.org/live?channel=1").unwrap(),
        defaults::normalized_url("https://example.org/live?channel=2").unwrap()
    );
    assert_ne!(
        defaults::normalized_url("https://example.org/Live").unwrap(),
        defaults::normalized_url("https://example.org/live").unwrap()
    );
}
#[test]
fn damaged_personal_database_is_not_reseeded_or_imported_over() {
    let (mut store, _temp) = fixture();
    let path = store.data_dir.join("stations.toml");
    fs::write(&path, "stations = [broken").unwrap();
    assert!(store.load().1.is_empty());
    assert!(!store.writable);
    assert!(store.import_defaults().is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "stations = [broken");
}
fn import_command(store: &Store) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cli-radio"));
    // Store::open adds cli-radio to each base; use separate roots for the CLI.
    cmd.arg("--import-defaults")
        .env("XDG_CONFIG_HOME", &store.config_dir)
        .env("XDG_DATA_HOME", &store.data_dir)
        .env("XDG_STATE_HOME", &store.state_dir);
    cmd
}
#[test]
fn cli_import_runs_without_terminal_and_is_idempotent() {
    let (store, _temp) = fixture();
    let first = import_command(&store).output().unwrap();
    assert!(first.status.success());
    assert_eq!(
        String::from_utf8(first.stdout).unwrap(),
        "Imported 9 default stations.\nSkipped 0 existing stations.\n"
    );
    let second = import_command(&store).output().unwrap();
    assert!(second.status.success());
    assert_eq!(
        String::from_utf8(second.stdout).unwrap(),
        "Imported 0 default stations.\nSkipped 9 existing stations.\n"
    );
}
#[test]
fn cli_import_refuses_corruption_without_exposing_raw_values() {
    let (store, _temp) = fixture();
    let dir = store.data_dir.join("cli-radio");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("stations.toml");
    fs::write(&path, "secret = [broken").unwrap();
    let output = import_command(&store).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8(output.stderr).unwrap().contains("secret"));
    assert_eq!(fs::read_to_string(path).unwrap(), "secret = [broken");
}
