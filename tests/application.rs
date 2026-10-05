use cli_radio::{
    app::{App, Metadata, Mode},
    config::{Config, Store},
    input,
    logging::Log,
    player::{Control, Notice},
    reconnect::Reconnect,
    stations::Station,
    ui,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::HashMap;
use tokio::sync::mpsc;

fn fixture() -> (App, mpsc::Receiver<Control>, tempfile::TempDir) {
    let temp = {
        std::fs::create_dir_all("target/test-data").unwrap();
        tempfile::tempdir_in("target/test-data").unwrap()
    };
    let path = temp.path().to_path_buf();
    let store = Store {
        config_dir: path.clone(),
        data_dir: path.clone(),
        state_dir: path.clone(),
        warnings: vec![],
        writable: true,
    };
    let (controls, receiver) = mpsc::channel(32);
    let stations = vec![
        Station::new("One", "https://example.org/one").unwrap(),
        Station::new("Two", "http://example.org/two").unwrap(),
    ];
    let app = App {
        random_timer: cli_radio::random::RandomTimer::default(),
        disconnects: 0,
        session_started: std::time::Instant::now(),
        config: Config::default(),
        stations,
        selected: 0,
        search: String::new(),
        mode: Mode::Normal,
        reconnect: Reconnect::default(),
        active: None,
        metadata: Metadata::default(),
        message: String::new(),
        store,
        log: Log::open(&path),
        controls,
        environment: HashMap::new(),
    };
    (app, receiver, temp)
}
async fn key(app: &mut App, key: KeyCode) {
    assert!(!input::handle(app, KeyEvent::new(key, KeyModifiers::NONE)).await);
}
#[tokio::test]
async fn keyboard_crud_favorites_search_and_persistence() {
    let (mut app, _receiver, _temp) = fixture();
    key(&mut app, KeyCode::Char('f')).await;
    assert!(app.stations[0].favorite);
    key(&mut app, KeyCode::Char('/')).await;
    key(&mut app, KeyCode::Char('T')).await;
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(app.selected_station().unwrap().name, "Two");
    key(&mut app, KeyCode::Char('e')).await;
    key(&mut app, KeyCode::Char('!')).await;
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(app.stations[1].name, "Two!");
    key(&mut app, KeyCode::Char('d')).await;
    key(&mut app, KeyCode::Char('n')).await;
    assert_eq!(app.stations.len(), 2);
    key(&mut app, KeyCode::Char('d')).await;
    key(&mut app, KeyCode::Char('y')).await;
    assert_eq!(app.stations.len(), 1);
    key(&mut app, KeyCode::Esc).await;
    key(&mut app, KeyCode::Char('a')).await;
    app.mode = Mode::Form {
        id: None,
        name: "New".into(),
        url: "https://example.org/new".into(),
        field: true,
    };
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(app.stations.len(), 2);
    key(&mut app, KeyCode::Char('-')).await;
    let (config, stations) = app.store.load();
    assert_eq!(config.volume, 80);
    assert_eq!(stations.len(), 2);
    assert!(stations[0].favorite);
}
#[tokio::test]
async fn switching_and_stop_ignore_old_player_events() {
    let (mut app, mut receiver, _temp) = fixture();
    app.play().await;
    let first = app.reconnect.generation;
    assert!(
        matches!(receiver.recv().await, Some(Control::Play { generation, .. }) if generation == first)
    );
    key(&mut app, KeyCode::Down).await;
    app.play().await;
    assert!(matches!(receiver.recv().await, Some(Control::Play { .. })));
    app.notice(Notice::Lost(first, "old disconnect"));
    assert!(app.reconnect.deadline.is_none());
    app.notice(Notice::Property(
        first,
        "metadata".into(),
        serde_json::json!({"title":"old"}),
    ));
    assert!(app.metadata.title.is_empty());
    app.stop().await;
    assert!(matches!(receiver.recv().await, Some(Control::Stop)));
    app.notice(Notice::Lost(
        app.reconnect.generation - 1,
        "late disconnect",
    ));
    assert!(!app.reconnect.desired);
    assert!(app.reconnect.deadline.is_none());
}
#[test]
fn renders_wide_narrow_small_and_forms_without_panicking() {
    let (mut app, _receiver, _temp) = fixture();
    for (width, height) in [(120, 30), (60, 25), (38, 12), (15, 5)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        app.mode = Mode::Form {
            id: None,
            name: "Station".into(),
            url: "https://example.org/a".into(),
            field: true,
        };
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    }
}

#[tokio::test]
async fn disconnect_counter_counts_outages_not_retry_failures() {
    let (mut app, _receiver, _temp) = fixture();
    app.play().await;
    app.notice(Notice::Playing(app.reconnect.generation));
    app.notice(Notice::Lost(app.reconnect.generation, "network lost"));
    assert_eq!(app.disconnects, 1);
    // Duplicate failure and failed attempts belong to the same outage.
    app.notice(Notice::Lost(app.reconnect.generation, "duplicate"));
    for _ in 0..3 {
        assert!(app.reconnect.tick(app.reconnect.deadline.unwrap()));
        app.notice(Notice::Lost(app.reconnect.generation, "retry failed"));
        assert_eq!(app.disconnects, 1);
    }
    assert!(app.reconnect.tick(app.reconnect.deadline.unwrap()));
    app.notice(Notice::Playing(app.reconnect.generation));
    assert_eq!(app.disconnects, 1);
    app.notice(Notice::Lost(app.reconnect.generation, "another outage"));
    assert_eq!(app.disconnects, 2);
}

#[tokio::test]
async fn disconnect_counter_excludes_stop_switch_exit_and_initial_failure() {
    let (mut app, _receiver, _temp) = fixture();
    app.play().await;
    app.notice(Notice::Lost(
        app.reconnect.generation,
        "initial connection failed",
    ));
    assert_eq!(app.disconnects, 0);
    app.play().await;
    app.notice(Notice::Playing(app.reconnect.generation));
    let previous = app.reconnect.generation;
    app.selected = 1;
    app.play().await;
    app.notice(Notice::Lost(previous, "old station ended"));
    assert_eq!(app.disconnects, 0);
    app.notice(Notice::Playing(app.reconnect.generation));
    let previous = app.reconnect.generation;
    app.stop().await; // Also used during normal application shutdown.
    app.notice(Notice::Lost(previous, "intentional stop"));
    app.notice(Notice::Lost(app.reconnect.generation, "late exit"));
    assert_eq!(app.disconnects, 0);
}

#[test]
fn session_stats_are_visible_and_not_persisted() {
    let (mut app, _receiver, _temp) = fixture();
    app.config.language = cli_radio::i18n::Language::English;
    app.session_started = std::time::Instant::now() - std::time::Duration::from_secs(202472);
    app.disconnects = 7;
    app.save();
    let config = std::fs::read_to_string(app.store.config_dir.join("config.toml")).unwrap();
    assert!(!config.contains("disconnects"));
    assert!(!config.contains("session_started"));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("Disconnects: 7"));
    assert!(text.contains("Uptime: 2d 08:14:"));
    assert!(text.contains("Stopped"));
    assert!(text.contains("Volume"));
    assert!(!text.contains("Größe:"));
    assert!(!text.contains("100 × 30"));
    let (fresh, _receiver, _temp) = fixture();
    assert_eq!(fresh.disconnects, 0);
}

#[test]
fn responsive_ui_preserves_status_stats_and_metadata() {
    let (mut app, _receiver, _temp) = fixture();
    app.config.language = cli_radio::i18n::Language::English;
    app.active = Some(app.stations[0].clone());
    app.metadata = Metadata {
        artist: "Test Artist".into(),
        title: "Test Title".into(),
        codec: "mp3".into(),
        bitrate: "192 kbps".into(),
    };
    for (width, height) in [(151, 51), (100, 28), (80, 20), (60, 25), (38, 12)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("Stopped"),
            "missing status at {width}x{height}"
        );
        assert!(
            text.contains("Disconnects: 0"),
            "missing counter at {width}x{height}"
        );
        assert!(
            text.contains("Uptime:"),
            "missing uptime at {width}x{height}"
        );
        assert!(text.contains("Test Artist"));
        assert!(text.contains("Test Title"));
        if height >= 20 {
            assert!(text.contains("mp3"));
            assert!(text.contains("192 kbps"));
            assert!(text.contains('█'));
        }
    }
}

#[test]
fn backward_compatible_language_and_interval_configuration() {
    use cli_radio::i18n::Language;
    let config: Config = toml::from_str("volume = 42").unwrap();
    assert_eq!(config.language, Language::German);
    assert!(!config.random_mode);
    assert_eq!(config.random_interval_hours, 12);
    for (value, expected) in [
        ("de", Language::German),
        ("en", Language::English),
        ("unknown", Language::German),
    ] {
        let config: Config = toml::from_str(&format!("language = '{value}'\nvolume = 42")).unwrap();
        assert_eq!(config.language, expected);
        assert_eq!(config.volume, 42);
    }
    for (value, expected) in [
        ("6", 6),
        ("24", 24),
        ("0", 12),
        ("-1", 12),
        ("'invalid'", 12),
        ("true", 12),
        ("1.5", 12),
        ("9223372036854775807", 12),
        ("[]", 12),
    ] {
        let config: Config =
            toml::from_str(&format!("random_interval_hours = {value}\nvolume = 42")).unwrap();
        assert_eq!(config.random_interval_hours, expected);
        assert_eq!(config.volume, 42);
    }
}
#[tokio::test]
async fn language_and_random_keys_toggle_and_persist_settings() {
    use cli_radio::i18n::Language;
    let (mut app, _receiver, _temp) = fixture();
    app.config.volume = 35;
    app.config.random_interval_hours = 6;
    key(&mut app, KeyCode::Char('l')).await;
    assert_eq!(app.config.language, Language::English);
    assert_eq!(app.store.load().0.language, Language::English);
    key(&mut app, KeyCode::Char('l')).await;
    assert_eq!(app.store.load().0.language, Language::German);
    key(&mut app, KeyCode::Char('r')).await;
    let config = app.store.load().0;
    assert!(config.random_mode);
    assert_eq!(config.random_interval_hours, 6);
    assert_eq!(config.volume, 35);
    key(&mut app, KeyCode::Char('r')).await;
    assert!(!app.store.load().0.random_mode);
    let text = std::fs::read_to_string(app.store.config_dir.join("config.toml")).unwrap();
    assert!(!text.contains("started"));
    assert!(!text.contains("countdown"));
    let (new_app, _receiver, _temp) = fixture();
    assert!(
        new_app
            .random_timer
            .remaining(std::time::Instant::now(), 12)
            .is_none()
    );
}
#[test]
fn translated_ui_preserves_station_names_and_stream_metadata() {
    use cli_radio::i18n::Language;
    let (mut app, _receiver, _temp) = fixture();
    let now = std::time::Instant::now();
    app.active = Some(Station::new("Radio BOB!", "https://example.org/live").unwrap());
    app.metadata.artist = "BAP".into();
    app.metadata.title = "Verdamp Lang Her".into();
    app.config.random_mode = true;
    app.random_timer.start(now);
    for (lang, station_label, playing_label, random_label, switch_label) in [
        (
            Language::German,
            "Sender",
            "Aktuelle Wiedergabe",
            "Zufall: AN",
            "l English",
        ),
        (
            Language::English,
            "Stations",
            "Now Playing",
            "Random: ON",
            "l Deutsch",
        ),
    ] {
        app.config.language = lang;
        for (width, height) in [(151, 51), (100, 28), (80, 20), (60, 25), (38, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(text.contains(station_label));
            assert!(text.contains(playing_label));
            assert!(text.contains("Radio BOB!"));
            if height > 12 {
                assert!(text.contains("BAP"));
                assert!(text.contains("Verdamp Lang Her"));
            }
            assert!(
                text.contains(random_label),
                "{width}x{height}: {random_label}"
            );
            assert!(text.contains(switch_label));
            assert!(text.contains('|'));
        }
    }
}
#[test]
fn random_countdown_uses_interval_and_translates_both_languages() {
    use cli_radio::i18n::Language;
    let mut timer = cli_radio::random::RandomTimer::default();
    let now = std::time::Instant::now();
    timer.start(now);
    let after = now + std::time::Duration::from_secs(60 * 60);
    assert_eq!(
        Language::German.random_status(true, &timer, after, 6, false),
        "Zufall: AN · nächster Wechsel in 05:00:00"
    );
    assert_eq!(
        Language::English.random_status(true, &timer, after, 24, false),
        "Random: ON · next switch in 23:00:00"
    );
    assert_eq!(
        Language::German.random_status(false, &timer, after, 12, false),
        "Zufall: AUS"
    );
    assert_eq!(
        Language::English.random_status(false, &timer, after, 12, false),
        "Random: OFF"
    );
}
#[tokio::test]
async fn random_switch_obeys_configured_intervals_and_excludes_active_station() {
    use std::time::{Duration, Instant};
    for hours in [6, 12, 24] {
        let (mut app, mut receiver, _temp) = fixture();
        let now = Instant::now();
        app.config.random_mode = true;
        app.config.random_interval_hours = hours;
        app.play_at(now).await;
        let old_generation = app.reconnect.generation;
        let old_id = app.active.as_ref().unwrap().id;
        assert!(matches!(receiver.recv().await, Some(Control::Play { .. })));
        app.notice(Notice::Playing(old_generation));
        let deadline = now + Duration::from_secs(hours * 3600);
        assert!(!app.random_tick(deadline - Duration::from_secs(1)).await);
        assert!(app.random_tick(deadline).await);
        assert_ne!(app.active.as_ref().unwrap().id, old_id);
        assert!(
            matches!(receiver.recv().await, Some(Control::Play { generation, .. }) if generation == app.reconnect.generation)
        );
        assert_eq!(
            app.random_timer.remaining(deadline, hours),
            Some(Duration::from_secs(hours * 3600))
        );
        app.notice(Notice::Lost(old_generation, "intentional old stream end"));
        app.notice(Notice::Playing(old_generation));
        assert_eq!(app.disconnects, 0);
        assert!(app.reconnect.deadline.is_none());
        assert!(!app.random_tick(deadline).await);
    }
}
#[tokio::test]
async fn random_off_and_single_station_do_not_switch_and_later_addition_works() {
    use std::time::{Duration, Instant};
    let (mut app, _receiver, _temp) = fixture();
    let now = Instant::now();
    app.play_at(now).await;
    app.notice(Notice::Playing(app.reconnect.generation));
    assert!(!app.random_tick(now + Duration::from_secs(86400)).await);
    let other = app.stations.pop().unwrap();
    app.config.random_mode = true;
    assert!(!app.random_tick(now + Duration::from_secs(86400)).await);
    assert!(app.config.random_mode);
    app.stations.push(other);
    assert!(app.random_tick(now + Duration::from_secs(86400)).await);
}
#[tokio::test]
async fn reconnect_preserves_random_timer_and_defers_due_switch_until_playing() {
    use std::time::{Duration, Instant};
    let (mut app, _receiver, _temp) = fixture();
    let now = Instant::now();
    app.config.random_mode = true;
    app.play_at(now).await;
    app.notice(Notice::Playing(app.reconnect.generation));
    app.notice(Notice::Lost(app.reconnect.generation, "outage"));
    assert_eq!(app.disconnects, 1);
    assert!(app.reconnect.tick(app.reconnect.deadline.unwrap()));
    app.launch().await; // Same stream's real retry path must not reset timer.
    let due = now + Duration::from_secs(12 * 3600);
    assert_eq!(
        app.random_timer
            .remaining(now + Duration::from_secs(8 * 3600), 12),
        Some(Duration::from_secs(4 * 3600))
    );
    assert!(!app.random_tick(due).await);
    app.notice(Notice::Playing(app.reconnect.generation));
    assert!(app.random_tick(due).await);
    assert_eq!(app.disconnects, 1);
}
#[tokio::test]
async fn manual_switch_stop_and_restart_reset_interval_without_overdue_switch() {
    use std::time::{Duration, Instant};
    let (mut app, _receiver, _temp) = fixture();
    let now = Instant::now();
    app.config.random_mode = true;
    app.config.random_interval_hours = 6;
    app.play_at(now).await;
    app.notice(Notice::Playing(app.reconnect.generation));
    let later = now + Duration::from_secs(3 * 3600);
    app.selected = 1;
    app.play_at(later).await;
    assert_eq!(
        app.random_timer.remaining(later, 6),
        Some(Duration::from_secs(6 * 3600))
    );
    app.stop().await;
    assert!(app.random_timer.remaining(later, 6).is_none());
    assert!(!app.random_tick(now + Duration::from_secs(24 * 3600)).await);
    let restart = now + Duration::from_secs(30 * 3600);
    app.play_at(restart).await;
    app.notice(Notice::Playing(app.reconnect.generation));
    assert!(!app.random_tick(restart).await);
    assert_eq!(
        app.random_timer.remaining(restart, 6),
        Some(Duration::from_secs(6 * 3600))
    );
}
#[tokio::test]
async fn enabling_random_during_playback_starts_fresh_interval() {
    use std::time::{Duration, Instant};
    let (mut app, _receiver, _temp) = fixture();
    let now = Instant::now();
    app.play_at(now).await;
    app.notice(Notice::Playing(app.reconnect.generation));
    let later = now + Duration::from_secs(24 * 3600);
    app.toggle_random(later);
    assert!(app.config.random_mode);
    assert!(!app.random_tick(later).await);
    assert_eq!(
        app.random_timer.remaining(later, 12),
        Some(Duration::from_secs(12 * 3600))
    );
}

#[test]
fn empty_station_list_is_localized_in_both_languages() {
    use cli_radio::i18n::Language;
    let (mut app, _receiver, _temp) = fixture();
    app.stations.clear();
    for (language, empty, hint) in [
        (
            Language::German,
            "Noch keine Sender vorhanden",
            "Drücke a, um einen Sender hinzuzufügen.",
        ),
        (
            Language::English,
            "No stations yet",
            "Press a to add a station.",
        ),
    ] {
        app.config.language = language;
        let mut terminal = Terminal::new(TestBackend::new(151, 30)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains(empty));
        assert!(text.contains(hint));
    }
}

#[test]
fn malformed_language_value_retains_other_valid_settings() {
    use cli_radio::i18n::Language;
    for value in ["42", "true", "[]", "{}"] {
        let config: Config = toml::from_str(&format!(
            "language = {value}\nvolume = 42\nrandom_interval_hours = 6"
        ))
        .unwrap();
        assert_eq!(config.language, Language::German);
        assert_eq!(config.volume, 42);
        assert_eq!(config.random_interval_hours, 6);
    }
}
#[test]
fn footer_and_technical_error_translation_are_centralized() {
    use cli_radio::i18n::Language;
    for language in [Language::German, Language::English] {
        for width in [38, 60, 80, 151] {
            let footer = language.footer(width);
            assert!(footer.iter().all(|line| line.contains('|')
                && !line.contains('·')
                && line.chars().count() <= width as usize));
        }
    }
    let warning = "Invalid/duplicate station skipped; source files preserved; Cannot read configuration/database; files preserved";
    let translated = Language::German.message(warning);
    assert!(translated.contains("Ungültiger/doppelter Sender"));
    assert!(translated.contains("nicht lesbar"));
    assert!(!translated.contains("source files"));
    assert!(!translated.contains("Cannot read"));
    assert_eq!(Language::English.message(warning), warning);
}
