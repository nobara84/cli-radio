use cli_radio::{
    app::{App, Metadata, Mode},
    config::Store,
    input,
    logging::Log,
    player::{self, Control},
    reconnect::Reconnect,
    terminal, ui,
};
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
#[tokio::main]
async fn main() -> io::Result<()> {
    let session_started = Instant::now();
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "cli-radio — terminal web radio\nRun without arguments to open the TUI.\n--import-defaults: add missing bundled stations without opening the TUI.\nEnter Play · Space Stop/Play · a Add · e Edit · d Delete\nf Favorite · / Search · +/- Volume · q/Ctrl+C Quit"
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("cli-radio {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut store = Store::open()?;
    if args.iter().any(|a| a == "--import-defaults") {
        match store.import_defaults() {
            Ok(report) => {
                println!(
                    "Imported {} default stations.\nSkipped {} existing stations.",
                    report.imported, report.skipped
                );
                return Ok(());
            }
            Err(_) => {
                eprintln!(
                    "Default station import failed. Check XDG permissions and repair invalid TOML; existing files are preserved when validation fails."
                );
                std::process::exit(1);
            }
        }
    }
    let (config, stations) = store.load();
    let mut log = Log::open(&store.state_dir);
    log.event("application start", 0);
    if !store.warnings.is_empty() {
        log.event("configuration warning", 0);
    }
    let message = store.warnings.join("; ");
    let runtime = store.state_dir.clone();
    let (controls, receiver) = mpsc::channel(32);
    let (notices, mut events) = mpsc::channel(256);
    let selected = config
        .last_station
        .and_then(|id| {
            cli_radio::stations::filtered(&stations, "")
                .iter()
                .position(|&i| stations[i].id == id)
        })
        .unwrap_or(0);
    let mut app = App {
        disconnects: 0,
        session_started,
        config,
        stations,
        selected,
        search: String::new(),
        mode: Mode::Normal,
        reconnect: Reconnect::default(),
        active: None,
        metadata: Metadata::default(),
        message,
        store,
        log,
        controls,
        environment: std::env::vars()
            .filter(|(k, _)| {
                matches!(
                    k.as_str(),
                    "HTTP_PROXY"
                        | "HTTPS_PROXY"
                        | "NO_PROXY"
                        | "http_proxy"
                        | "https_proxy"
                        | "no_proxy"
                )
            })
            .collect(),
    };
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        terminal::restore();
        old_hook(info);
    }));
    let _guard = terminal::Guard::enter()?;
    let mut terminal = terminal::terminal()?;
    let player = tokio::spawn(player::run(receiver, notices, runtime));
    let mut keys = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let result = loop {
        if let Err(error) = terminal.draw(|frame| ui::draw(frame, &app)) {
            break Err(error);
        }
        tokio::select! {
            event = keys.next() => match event { Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => { if input::handle(&mut app, key).await { break Ok(()); } }, Some(Err(_)) | None => break Ok(()), _ => {} },
            event = events.recv() => { if let Some(event) = event { app.notice(event); } },
            _ = tick.tick() => { if app.reconnect.tick(Instant::now()) { app.log.event("reconnect attempt", app.reconnect.generation); app.launch().await; } },
            _ = tokio::signal::ctrl_c() => break Ok(()),
            _ = terminate.recv() => break Ok(()),
        }
    };
    app.stop().await;
    let _ = app.controls.send(Control::Quit).await;
    // Joining ensures our only mpv child is killed and reaped before leaving.
    let _ = player.await;
    app.save();
    app.log.event("application exit", app.reconnect.generation);
    result
}
