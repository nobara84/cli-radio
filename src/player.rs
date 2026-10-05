use crate::network::Proxy;
use serde_json::{Value, json};
use std::{io, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::{Child, Command},
    sync::mpsc,
    time::{interval, timeout},
};

pub enum Control {
    Play {
        generation: u64,
        url: String,
        volume: u8,
        proxy: Proxy,
    },
    Stop,
    Volume(u8),
    Quit,
}
pub enum Notice {
    Playing(u64),
    Lost(u64, &'static str),
    Property(u64, String, Value),
}
struct Session {
    child: Child,
    socket: PathBuf,
    dir: PathBuf,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_dir(&self.dir);
    }
}
impl Session {
    async fn close(&mut self) {
        // Request a normal exit first. A hung/crashed backend is killed and
        // reaped after a bounded grace period; only our owned child is touched.
        let _ = timeout(Duration::from_millis(300), async {
            if let Ok(mut stream) = UnixStream::connect(&self.socket).await {
                let _ = stream.write_all(b"{\"command\":[\"quit\"]}\n").await;
            }
        })
        .await;
        if timeout(Duration::from_millis(500), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
        }
    }
}
async fn send(stream: &mut tokio::net::unix::OwnedWriteHalf, value: Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(&value)?;
    bytes.push(b'\n');
    timeout(Duration::from_secs(2), stream.write_all(&bytes))
        .await
        .map_err(|_| io::Error::other("IPC timeout"))?
}
fn spawn(proxy: &Proxy, runtime: &std::path::Path) -> Result<Session, &'static str> {
    use std::os::unix::fs::PermissionsExt;
    let dir = runtime.join(format!(
        "mpv-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    ));
    std::fs::create_dir(&dir).map_err(|_| "Cannot create private IPC directory")?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Cannot protect IPC directory")?;
    let socket = dir.join("ipc");
    let mut cmd = Command::new("mpv");
    cmd.args([
        "--no-config",
        "--no-video",
        "--no-audio-display",
        "--idle=yes",
        "--terminal=no",
        "--input-terminal=no",
        "--input-default-bindings=no",
        "--ytdl=no",
        "--network-timeout=15",
        "--tls-verify=yes",
    ])
    .arg(format!("--input-ipc-server={}", socket.display()))
    .env_remove("HTTP_PROXY")
    .env_remove("HTTPS_PROXY")
    .env_remove("https_proxy")
    .env_remove("ALL_PROXY")
    .env_remove("all_proxy")
    .env_remove("http_proxy")
    .env_remove("NO_PROXY")
    .env("no_proxy", &proxy.bypass)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    if let Some(proxy) = &proxy.url {
        cmd.env("http_proxy", proxy);
    }
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_dir(&dir);
            return Err(if e.kind() == io::ErrorKind::NotFound {
                "mpv is not installed; install the mpv package"
            } else {
                "Cannot start mpv"
            });
        }
    };
    Ok(Session { child, socket, dir })
}
async fn connect(session: &mut Session, url: &str, volume: u8) -> Result<UnixStream, &'static str> {
    let connect = timeout(Duration::from_secs(5), async {
        loop {
            if session.child.try_wait().ok().flatten().is_some() {
                return Err("mpv exited before IPC was ready");
            }
            if let Ok(stream) = UnixStream::connect(&session.socket).await {
                return Ok(stream);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    match connect {
        Ok(Ok(stream)) => {
            // URLs and volume are JSON commands, never shell arguments.
            let (read, mut write) = stream.into_split();
            for (i, name) in [
                "metadata",
                "audio-codec-name",
                "audio-bitrate",
                "paused-for-cache",
                "time-pos",
            ]
            .iter()
            .enumerate()
            {
                if send(
                    &mut write,
                    json!({"command":["observe_property", i + 1, name]}),
                )
                .await
                .is_err()
                {
                    return Err("mpv IPC setup failed");
                }
            }
            if send(
                &mut write,
                json!({"command":["set_property", "volume", volume]}),
            )
            .await
            .is_err()
                || send(
                    &mut write,
                    json!({"command":["loadfile", url, "replace"],"request_id":100}),
                )
                .await
                .is_err()
            {
                return Err("mpv IPC load failed");
            }
            read.reunite(write).map_err(|_| "mpv IPC split error")
        }
        _ => Err("mpv IPC startup failed"),
    }
}
pub async fn run(
    mut controls: mpsc::Receiver<Control>,
    notices: mpsc::Sender<Notice>,
    runtime: PathBuf,
) {
    let mut pending = None;
    loop {
        let control = match pending.take() {
            Some(c) => c,
            None => match controls.recv().await {
                Some(c) => c,
                None => break,
            },
        };
        let Control::Play {
            generation,
            url,
            volume,
            proxy,
        } = control
        else {
            if matches!(control, Control::Quit) {
                break;
            }
            continue;
        };
        let mut session = match spawn(&proxy, &runtime) {
            Ok(session) => session,
            Err(error) => {
                let _ = notices.send(Notice::Lost(generation, error)).await;
                continue;
            }
        };
        // Keep the child outside the cancellable startup future. Always reap it
        // before a replacement can spawn, including rapid stop/station switches.
        let mut updated_volume = None;
        let result = {
            let startup = connect(&mut session, &url, volume);
            tokio::pin!(startup);
            loop {
                tokio::select! {
                    result = &mut startup => break Some(result),
                    control = controls.recv() => match control {
                        Some(Control::Volume(v)) => updated_volume = Some(v),
                        other => { pending = other; break None; }
                    }
                }
            }
        };
        let stream = match result {
            Some(Ok(stream)) => stream,
            other => {
                session.close().await;
                if let Some(Err(error)) = other {
                    let _ = notices.send(Notice::Lost(generation, error)).await;
                }
                if controls.is_closed() && pending.is_none() {
                    break;
                }
                continue;
            }
        };
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        if let Some(v) = updated_volume {
            let _ = send(&mut write, json!({"command":["set_property", "volume", v]})).await;
        }
        let mut watch = interval(Duration::from_millis(250));
        let mut stalled = None;
        let mut last_position = None;
        let mut progressed = tokio::time::Instant::now();
        let mut playing = false;
        let error = loop {
            tokio::select! {
                control = controls.recv() => match control {
                    Some(Control::Volume(v)) => { if send(&mut write, json!({"command":["set_property","volume",v]})).await.is_err() { break Some("mpv IPC volume failed"); } },
                    other => { pending = other; break None; }
                },
                line = lines.next_line() => {
                    let value: Value = match line { Ok(Some(line)) => match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue }, _ => break Some("mpv IPC connection lost") };
                    match value["event"].as_str() {
                        Some("file-loaded") => { playing = true; let _ = notices.send(Notice::Playing(generation)).await; },
                        Some("end-file") if value["reason"] != "redirect" => break Some("Stream ended unexpectedly"),
                        Some("shutdown") => break Some("mpv shut down unexpectedly"),
                        Some("property-change") => {
                            let name = value["name"].as_str().unwrap_or("");
                            if name == "paused-for-cache" { if value["data"].as_bool() == Some(true) { stalled.get_or_insert(tokio::time::Instant::now()); } else { stalled = None; } }
                            if name == "time-pos" && value["data"].as_f64().is_some() {
                                let position = value["data"].as_f64();
                                if position != last_position { progressed = tokio::time::Instant::now(); last_position = position; }
                            }
                            let _ = notices.send(Notice::Property(generation, name.into(), value["data"].clone())).await;
                        }, _ => {}
                    }
                    if value["request_id"] == 100 && value["error"].as_str().is_some_and(|s| s != "success") { break Some("mpv rejected the stream"); }
                },
                _ = watch.tick() => {
                    if session.child.try_wait().ok().flatten().is_some() { break Some("mpv exited unexpectedly"); }
                    if stalled.is_some_and(|t| t.elapsed() > Duration::from_secs(30)) || (playing && last_position.is_some() && progressed.elapsed() > Duration::from_secs(60)) { break Some("Stream stalled; restarting"); }
                }
            }
        };
        session.close().await;
        if let Some(error) = error {
            let _ = notices.send(Notice::Lost(generation, error)).await;
        }
        if controls.is_closed() && pending.is_none() {
            break;
        }
    }
}
