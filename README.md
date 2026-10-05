# cli-radio

Ein Linux-Webradio ausschließlich fürs Terminal, geschrieben in Rust mit
Ratatui/Crossterm. mpv übernimmt Audio und Streaming; es öffnet kein Fenster.
Primäre Zielplattformen sind Fedora und Nobara. Lizenz: MIT.

## Funktionen

- Interaktive, anpassbare TUI; Sender auswählen, starten, stoppen und wechseln
- Sender hinzufügen, bearbeiten, mit Bestätigung löschen, suchen und favorisieren
- Lautstärke, letzter ausgewählter Sender, Favoriten und Einstellungen dauerhaft speichern
- Artist/Titel, Codec und Bitrate anzeigen, soweit mpv sie bereitstellt
- HTTP/HTTPS und durch mpv/FFmpeg unterstützte MP3-, AAC-, Ogg/Vorbis-, Opus- und HLS-Streams
- Automatischer Reconnect bei Stream-Ende, Netzwerkproblemen, IPC-Ausfall und mpv-Absturz
- HTTP-Proxy für den eigentlichen Stream, einschließlich HTTPS CONNECT
- Private IPC-Sockets, atomare TOML-Dateien und begrenzte Dateilogs

Die Senderliste startet bewusst leer. Es gibt keine Online-Sendersuche oder
vorinstallierten URLs. Aufnahme, Timer, Wecker, MPRIS und Playlist-Import sind
nicht Bestandteil dieses MVP. Ein beim Start gespeicherter Sender wird ausgewählt,
aber erst mit Enter gestartet.

## Voraussetzungen und Installation auf Fedora/Nobara

Benötigt werden ein normales interaktives Terminal, Linux, Rust/Cargo und mpv
mit funktionierender Audioausgabe. Getestete lokale Umgebung: Rust/Cargo 1.98.1,
mpv 0.41.0 und FFmpeg 8.1.2. Abhängigkeiten sind in Cargo.lock festgehalten.
Codec-Unterstützung hängt von der installierten mpv/FFmpeg-Ausgabe ab.

```bash
sudo dnf install rust cargo rustfmt clippy mpv
cd /home/sysop/Projekte/cli-radio
cargo build --release --locked
./target/release/cli-radio
```

Die Paketnamen sind über die offiziellen Fedora-Paketlisten überprüft:
[rust](https://packages.fedoraproject.org/pkgs/rust/rust/),
[cargo](https://packages.fedoraproject.org/pkgs/rust/cargo/),
[rustfmt](https://packages.fedoraproject.org/pkgs/rust/rustfmt/),
[clippy](https://packages.fedoraproject.org/pkgs/rust/clippy/),
[mpv](https://packages.fedoraproject.org/pkgs/mpv/mpv/).
Auf Nobara wurde auch das passende Clippy-Paket aus dem vorhandenen Nobara-Repository
heruntergeladen und für die Projektprüfung lokal ausgeführt. Je nach
Fedora-Ausgabe/Repository-Auswahl kann die vollständige Codec-Unterstützung
zusätzliche, distributionsspezifische Multimedia-Pakete erfordern.

Optional das fertige Binary installieren:

```bash
install -Dm755 target/release/cli-radio "$HOME/.local/bin/cli-radio"
cli-radio
```

Dafür muss `~/.local/bin` im PATH liegen. `cli-radio --help` und `--version`
funktionieren auch ohne interaktives Terminal.

## Bedienung und erster Test

1. Anwendung starten, `a` drücken und einen Sendernamen eingeben.
2. Mit Tab ins URL-Feld wechseln und die direkte HTTP/HTTPS-Stream-URL des
   gewünschten Anbieters eingeben. Eine normale Website-Adresse ist kein Stream.
3. Mit Enter speichern und erneut Enter drücken, um den Sender zu starten.
4. Mit `+`/`-` Lautstärke einstellen, mit Space stoppen/erneut starten.
5. Zum manuellen Reconnect-Test während der Wiedergabe kurz die Netzwerkverbindung
   unterbrechen und wiederherstellen. Der Status zeigt die Wiederholungsversuche.
6. Mit `q` beenden. Beim nächsten Start sind Sender und Lautstärke gespeichert.

| Taste | Aktion |
|---|---|
| ↑/↓ oder k/j | Navigation |
| Enter | Ausgewählten Sender starten/wechseln |
| Space | Stop / ausgewählten Sender starten |
| + / - | Lautstärke ±5, begrenzt auf 0–100 % |
| f | Favorit umschalten; Favoriten erscheinen zuerst |
| / | Namen filtern, unabhängig von Groß-/Kleinschreibung |
| Enter/Esc in Suche | Suche verlassen; Filter bleibt erhalten |
| Esc im Hauptbild | Filter löschen |
| a / e | Sender hinzufügen / bearbeiten |
| Tab/Shift+Tab im Formular | Zwischen Name und URL wechseln |
| Enter/Esc im Formular | Speichern / abbrechen |
| d, anschließend y | Sender löschen; n/Esc bricht ab |
| q / Ctrl+C | Beenden |

Beim Bearbeiten bleibt die bereits laufende Senderkopie bis zum nächsten Enter
aktiv. Das Löschen des laufenden Senders stoppt ihn. Formulare unterstützen
Unicode-Eingabe und Backspace; einen vollständigen Texteditor gibt es noch nicht.
Die Oberfläche wechselt unter 80 Spalten zu einer vertikalen Aufteilung; unter
38×12 Zeichen erscheint ein Hinweis zur Terminalgröße.

## Konfiguration und XDG

| Inhalt | Pfad | Fallback |
|---|---|---|
| Einstellungen | `$XDG_CONFIG_HOME/cli-radio/config.toml` | `~/.config/cli-radio/config.toml` |
| Sender | `$XDG_DATA_HOME/cli-radio/stations.toml` | `~/.local/share/cli-radio/stations.toml` |
| Logs / private IPC-Verzeichnisse | `$XDG_STATE_HOME/cli-radio/` | `~/.local/state/cli-radio/` |

XDG-Werte müssen absolute Pfade sein; leere oder relative Werte verwenden den
HOME-Fallback. Verzeichnisse erhalten Modus 0700, neu gespeicherte Dateien 0600.
Es wird in eine eindeutige temporäre Datei geschrieben, synchronisiert, atomar
umbenannt und das Elternverzeichnis synchronisiert. Die zwei TOML-Dateien werden
jeweils atomar gespeichert, bilden aber keine gemeinsame Datenbanktransaktion.
UUIDs bleiben bei Bearbeitung unverändert.

Beispiel `config.toml` (siehe auch [examples/config.toml](examples/config.toml)):

```toml
volume = 85
# last_station = "UUID eines gespeicherten Senders"

[network]
proxy = ""
proxy_username = ""
proxy_password = ""
# no_proxy = "localhost,127.0.0.1,.example.org"
```

Beispiel `stations.toml`:

```toml
[[stations]]
id = "7d853cab-8f6c-4d0b-9845-3aa02b4a28c1"
name = "Mein Radio"
url = "https://radio.example.org/live"
favorite = true
```

Die Beispiel-URL ist ein Platzhalter. URLs müssen HTTP/HTTPS sein;
eingebettete Benutzername/Passwort-Paare werden abgewiesen. Einstellungen lassen
sich bei beendeter Anwendung bearbeiten; Proxy-Änderungen erfordern einen Neustart.
Fehlende Dateien laden Defaults. Bei fehlerhaftem TOML, ungültigen Sendern oder
doppelten IDs zeigt die TUI eine Warnung und deaktiviert Speichern für diese Sitzung,
damit Originaldateien erhalten bleiben. Dateien reparieren und neu starten.

## Proxy

Priorität:

1. Nichtleeres `network.proxy` in config.toml.
2. Für HTTPS: `https_proxy` / `HTTPS_PROXY`, danach `http_proxy` / `HTTP_PROXY`.
   Für HTTP: `http_proxy` / `HTTP_PROXY`. Klein geschriebene Variablen haben
   bei gleichzeitig gesetzten Varianten Vorrang, auch wenn sie leer sind.
3. Ohne Einstellung direkte Verbindung.

`network.no_proxy` hat Vorrang vor `no_proxy` / `NO_PROXY`. Weglassen bedeutet
Vererbung, eine explizite leere Zeichenkette deaktiviert den Bypass. Die Liste
wird an mpv/FFmpeg übergeben; Domain-/Host-Matching folgt dem installierten Backend.
Verwende konkrete Hosts und Domain-Suffixe. CIDR und komplexe Wildcards sind
versionsabhängig und werden nicht von cli-radio selbst interpretiert.

```toml
[network]
proxy = "http://proxy.example.org:8080"
proxy_username = ""
proxy_password = ""
no_proxy = "localhost,127.0.0.1,.internal.example.org"
```

Auch für HTTPS-Zielstreams muss die Proxy-Adresse `http://` verwenden; HTTPS zum
Proxy selbst und SOCKS werden im MVP nicht unterstützt. HTTPS-Streams verwenden
CONNECT, TLS-Zertifikate werden geprüft. NTLM/Kerberos-Unternehmensanmeldung und
besondere Firmen-CA-Einrichtung sind nicht implementiert.

cli-radio setzt für den eigenen mpv-Kindprozess das von mpv dokumentierte
`http_proxy` sowie `no_proxy`. Damit gelten die Einstellungen für die Audioquelle,
nicht nur für API-Abfragen. Die eigene mpv-Konfiguration und ytdl-Hooks sind
abgeschaltet. Keine URL oder Zugangsdaten werden über eine Shell ausgewertet.
Die mpv-Option `--http-proxy` wird bewusst nicht verwendet: die lokale Dokumentation
nennt Einschränkungen für HTTPS. Grundlage:
[mpv-Handbuch, Environment variables](https://mpv.io/manual/stable/#environment-variables),
plus lokal geprüftes mpv-0.41.0-Manpage. Der lokale Smoke-Test bestätigt HTTP-Proxy-
Routing und HTTPS CONNECT mit dem installierten FFmpeg-Backend.

Benutzername/Passwort werden URL-kodiert und über die Kindprozessumgebung übertragen.
Sie erscheinen nicht in Prozessargumenten, TUI oder normalen Logs. Raw-mpv-Ausgaben
werden verworfen und Fehler durch feste, bereinigte Meldungen ersetzt. Die
Konfigurationsdatei enthält Zugangsdaten im Klartext mit Modus 0600; die Umgebung
bleibt für berechtigte Prozesse desselben Benutzers bzw. root einsehbar.
Das ist keine Secret-Vault-Lösung. Keine echten Zugangsdaten ins Repository legen.

## Reconnect und Dauerbetrieb

Die State Machine verwendet `Stopped`, `Connecting`, `Playing`, `Disconnected`
und `Reconnecting`. Der gewünschte Wiedergabezustand ist vom beobachteten
Playerzustand getrennt. Jede neue Wiedergabe/Stop-Aktion erhält eine Generation;
alte Events nach einem Senderwechsel oder Stop werden verworfen.

Nach unerwartetem Ende startet der erste Versuch sofort, danach folgen 1, 2, 5,
10 und höchstens 30 Sekunden Abstand. Es gibt keine maximale Versuchszahl.
Nach mindestens 60 Sekunden zusammenhängender erfolgreicher Wiedergabe setzt ein
anschließender Abbruch den Fehlerzähler zurück. Ein dauerhaft ausfallender Sender
wird weiter versucht. Absichtlicher Stop und Programmende deaktivieren Reconnect.

mpv-Events und beobachtete Properties liefern Metadaten und Status. Ein
250-ms-Watchdog überwacht den eigenen Prozess und Stillstand: über 30 Sekunden
Cache-Pause oder 60 Sekunden ohne gemeldeten Positionsfortschritt lösen einen
Neustart aus. Connecting hat ein 45-Sekunden-Limit; mpv-Netzwerkzugriffe ein
15-Sekunden-Timeout. Der Worker beendet und reapet den vorherigen Prozess vor
einem Ersatzstart. Abgebrochener IPC-Aufbau bleibt ebenfalls abbrechbar.
Die TUI wartet nicht auf Netzwerkzugriffe oder Reconnect-Timer.

## Logging und Troubleshooting

`cli-radio.log` liegt im State-Verzeichnis, verwendet Unix-Zeitstempel und
numerische Wiedergabe-Generationen. Start, Senderwechsel, Stop, Reconnect,
Playerfehler und Speicherfehler werden ohne URLs, Metadaten oder Secrets protokolliert.
Bei ungefähr 2 MB wird nach `cli-radio.previous.log` rotiert; es bleibt eine
vorherige Datei. Während der TUI gelangen keine Playerlogs auf stdout/stderr.

- **mpv fehlt:** `sudo dnf install mpv`. Die TUI bleibt bedienbar und zeigt die
  Installationsmeldung beim Start eines Senders.
- **Kein Ton:** Lautstärke, Audioausgabe und PipeWire/PulseAudio prüfen. Einen
  vertrauenswürdigen Stream separat mit mpv testen. Den lokalen Integrationstest
  nicht als Audio-Hörtest verstehen: er nutzt bewusst `--ao=null`.
- **Dauerhaft Connecting/Reconnecting:** direkte Stream-URL, DNS, Proxy-Erreichbarkeit,
  NO_PROXY und CA-Vertrauen prüfen. Generische Fehlertexte schützen Zugangsdaten.
- **Ungültige Konfiguration:** TOML-Dateien sichern und reparieren; bis zum Neustart
  werden Änderungen nur im Arbeitsspeicher gehalten.
- **Keine Speicherung:** Schreibrechte und freien Speicher der XDG-Verzeichnisse prüfen.
- **IPC-Fehler:** Linux-Unix-Sockets haben eine kurze Pfadlängengrenze. Sehr lange
  XDG_STATE_HOME-Pfade vermeiden. Normales Beenden entfernt private Socket-Verzeichnisse;
  nach SIGKILL/Stromausfall können verwaiste Verzeichnisse verbleiben.
- **Terminal:** RAII und Panic-Hook stellen Raw Mode, Cursor und Alternate Screen
  bei normalem Exit, Fehler, Ctrl+C und SIGTERM wieder her. SIGKILL/Stromausfall
  kann kein Programm abfangen; dann gegebenenfalls `reset` im Terminal ausführen.

Ein mehrtägiger Realnetz-Dauertest sowie Tests mit realen Unternehmensproxies,
allen Audioformaten und verschiedenen Distributionen stehen noch aus.

## Entwicklung und Tests

```bash
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --locked
python3 tests/smoke.py
git diff --check
```

Unit-/Anwendungstests prüfen Parser, atomare Speicherung, beschädigte Dateien,
Sender-CRUD, Suche/Favoriten, Proxy-Priorität, Backoff, dauerhaftes Wiederholen,
Stop/Senderwechsel und TUI bei verschiedenen Größen. Sie brauchen kein Internet.
Der optionale Python-Smoke-Test braucht Python 3 und installiertes mpv; er öffnet
nur einen Loopback-HTTP-Server, verwendet echte mpv-Prozesse mit Null-Audio und
prüft Crash/EOF-Reconnect, Stop, Proxy und PTY-Terminal-Cleanup. Testdaten liegen
nur unter `target/smoke/`; Benutzereinstellungen bleiben unberührt.

Module unter `src/`: `app` (Zustand), `input`, `ui`, `terminal` (RAII), `player`
(asynchroner mpv-Worker mit JSON IPC), `reconnect` (testbare State Machine),
`stations`, `config`, `network` und `logging`. `main` verbindet Inputstream,
Playerkanal, Tick und Betriebssystemsignale über Tokio `select!`. RPM-Packaging
kann später das einzelne Binary und Dokumentation installieren; ein RPM-Spec ist
noch nicht enthalten.
