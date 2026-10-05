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
