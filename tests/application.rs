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
