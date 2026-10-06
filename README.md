# cli-radio

Version 0.1.2 fixes proxy port normalization and proxy diagnostics.

A lightweight terminal-based internet radio player for Linux, developed by
**Markus Schneider**. The entire interface runs in your terminal using
Ratatui and Crossterm. mpv plays audio in the background through JSON IPC;
it never opens a separate window. cli-radio is open source under the MIT License.

Fedora and Nobara are the primary development and test platforms. Other Linux
distributions have not yet been fully tested.

## Features

- Select, play, stop and change stations; adjust volume
- Add, edit and delete stations; favorites and name search/filter
- Now Playing with artist/title, codec and bitrate when provided by mpv
- HTTP/HTTPS streams and mpv-supported formats such as MP3, AAC, Ogg/Vorbis,
  Opus and HLS (depending on the installed mpv/FFmpeg codecs)
- Automatic recovery from stream/network failures and mpv crashes, with
  unlimited reconnect attempts and bounded backoff
- Separate handling of intentional stop/change and unexpected disconnects
- Per-session disconnect counter and monotonic process uptime
- HTTP proxy support for playback, including HTTPS CONNECT and NO_PROXY
- XDG configuration, atomic TOML persistence and rotating file logs
- Personal station management and bulk TOML editing
- German UI by default, English available; live switching with `l`
- Optional random station mode with a configurable interval (12 hours by default)

## Requirements and installation

You need Linux, an interactive terminal and mpv with working audio output.
Building from source additionally requires Rust/Cargo. Locally tested tools:
Rust/Cargo 1.98.1, mpv 0.41.0 and FFmpeg 8.1.2. Cargo.lock pins Rust dependencies.

On Fedora/Nobara:

```bash
sudo dnf install rust cargo rustfmt clippy mpv
git clone https://github.com/nobara84/cli-radio.git
cd cli-radio
cargo build --release --locked
./target/release/cli-radio
```

Package names are documented in Fedora's official package catalog:
[rust](https://packages.fedoraproject.org/pkgs/rust/rust/),
[cargo](https://packages.fedoraproject.org/pkgs/rust/cargo/),
[rustfmt](https://packages.fedoraproject.org/pkgs/rust/rustfmt/),
[clippy](https://packages.fedoraproject.org/pkgs/rust/clippy/),
[mpv](https://packages.fedoraproject.org/pkgs/mpv/mpv/).
Codec availability depends on the distribution's multimedia repositories and
installed mpv/FFmpeg packages.

Alternatively, install the executable with Cargo from the checkout:

```bash
cargo install --path . --locked
cli-radio
```

Cargo's binary directory (normally `~/.cargo/bin`) must be in PATH. Local
`cargo install --path .` was checked using an isolated installation root.

`cli-radio -h` and `cli-radio --help` show the same help without a terminal,
mpv, or any config/station changes. `--version` prints the version.
Unknown options and positional arguments fail with an error, a `--help` hint
and a nonzero exit code before opening the TUI or accessing configuration.

## Getting started and controls

A fresh installation starts with no stations. Press `a` to add your own.
Once stations exist, select one using the arrow keys and press Enter. The saved last station is
selected on startup; playback starts only when you request it. To add your own
station, press `a`, enter a name, use Tab to enter a **direct HTTP/HTTPS stream URL**,
and press Enter to save. A radio station's ordinary website URL is not a stream.

These shortcuts apply in the main view. While typing a search or form, letters
are text input rather than global shortcuts. Station names and stream metadata
are displayed as provided, independently of the UI language.

| Key | Action |
|---|---|
| Up/Down or k/j | Select station |
| Enter | Play / change to selected station |
| Space | Stop / play selected station |
| + or = / - | Increase / decrease volume by 5, limited to 0–100% |
| f | Toggle favorite; favorites appear first |
| / | Search names, case-insensitively |
| Enter/Esc in search | Leave search, retaining the filter |
| Esc in main view | Clear filter |
| a / e | Add / edit station |
| Tab/Shift+Tab in a form | Change field |
| Enter/Esc in a form | Save / cancel |
| d, then y | Delete station; n/Esc cancels in either language |
| r | Toggle random mode |
| l | Switch German/English and save choice |
| i | Open Info/Diagnostics; i or Esc returns |
| q / Ctrl+C | Quit |

Editing a playing station changes the saved entry; the current stream is retained
until your next explicit start. Deleting the active station stops playback.
The form supports Unicode and Backspace, rather than a full text editor.

The horizontal layout allocates roughly 33% to stations and 67% to Now Playing.
Below 80 columns it stacks the panels; small terminals receive compact details
and shortcuts. Below 38×12 it displays a terminal-size notice.
Footer actions use `|` separators. German shows `l English`; English shows
`l Deutsch`. Very compact footers use `a+`, `e~`, `d-` for add/edit/delete.

## UI language

German (`de`) is the default, including for existing config files without a
language field. Press `l` to immediately switch to English (`en`) and back.
The choice is saved using the existing atomic persistence mechanism and survives
restarts. Invalid language values fall back to German. System language is
not detected. Logs, command output and CLI help remain English.

## Random station mode

Random mode is optional and off by default. Press `r` to toggle it. Configuration:

```toml
random_mode = true
random_interval_hours = 12
```

The interval is a positive whole number of **hours**, edited in config.toml
while cli-radio is closed. Missing or invalid values (including zero, negatives,
floats, strings, booleans and overflow) use the default of 12 hours. For example,
`6` means six hours and `24` means twenty-four hours. Restart after manual changes.

An interval begins on deliberate play, manual station change, automatic random
change, or enabling random while playback is requested. Reconnect of the same
stream **does not reset it**. Stop cancels the countdown; the next deliberate
Play begins a full new interval. ON/OFF and the configured hours are persisted;
the countdown itself is not. Startup still waits for an explicit Play.

When the interval expires during playback, cli-radio randomly selects a different
station by UUID from the entire personal list, ignoring favorites and the search
filter. The filter itself is retained. With only one station it does not switch;
random can stay on and becomes usable when another station is added. If the
interval expires during an outage, the switch waits until playback resumes.
A random switch is intentional: it neither increments the disconnect counter nor
allows late events to reconnect the old station.

Now Playing shows, for example:

```text
Zufall: AN · nächster Wechsel in 11:42:18
Random: ON · next switch in 11:42:18
```

Compact layouts abbreviate the text. With no playback requested, an enabled mode
shows that it is waiting for playback. Timers use monotonic `Instant`; changes to
the system clock do not affect them.

## Files, configuration and bulk editing

| Content | XDG location | Fallback |
|---|---|---|
| Settings | `$XDG_CONFIG_HOME/cli-radio/config.toml` | `~/.config/cli-radio/config.toml` |
| Stations | `$XDG_DATA_HOME/cli-radio/stations.toml` | `~/.local/share/cli-radio/stations.toml` |
| State / private IPC / logs | `$XDG_STATE_HOME/cli-radio/` | `~/.local/state/cli-radio/` |

XDG paths must be absolute; relative or empty values use the HOME fallback.
Directories are mode 0700 and new saved files mode 0600. Writes use a private
temporary file, fsync, atomic publication and parent-directory fsync. Settings
and stations are each atomic, but are not one combined database transaction.

Current config.toml (also see [examples/config.toml](examples/config.toml)):

```toml
volume = 85
language = "de"
random_mode = false
random_interval_hours = 12
# last_station = "UUID of an existing personal station"

[network]
proxy = ""
proxy_username = ""
proxy_password = ""
# no_proxy = "localhost,127.0.0.1,.example.org"
```

Old files without new fields remain compatible. Defaults are German, random off,
and twelve hours. Stop cli-radio before editing settings or stations manually;
simultaneous writers are not coordinated.

For bulk editing, edit the personal stations.toml directly:

```toml
[[stations]]
id = "7d853cab-8f6c-4d0b-9845-3aa02b4a28c1"
name = "Radio Example"
url = "https://example.org/stream.mp3"
favorite = false

[[stations]]
id = "1ad4db97-dd9e-4d86-b6e0-2b5f31d5e3ef"
name = "Another Radio"
url = "https://example.org/another.mp3"
favorite = true
```

These URLs are placeholders. **Every personal station requires a unique UUID**;
UUIDs are not auto-generated for manual personal entries. Use the TUI for automatic
UUID creation. Credentials embedded in station URLs are rejected. Missing config
files use default settings; missing station files start empty. Damaged TOML, invalid stations or duplicate UUIDs generate a TUI
warning and disable saving for that session to preserve originals. Repair the
files and restart. Changes made while saving is disabled remain only in memory.

## Proxy

The proxy applies to mpv's actual audio stream, not only application API requests.
Supported variables: `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` and their lowercase
variants. Precedence:

1. Nonempty `network.proxy` overrides proxy environment variables.
2. HTTPS streams: `https_proxy`/`HTTPS_PROXY`, then `http_proxy`/`HTTP_PROXY`.
   HTTP streams: `http_proxy`/`HTTP_PROXY`. Lowercase wins when both variants exist,
   even when the lowercase value is empty.
3. Otherwise connect directly.

`network.no_proxy` overrides `no_proxy`/`NO_PROXY`. Omit it to inherit; `""`
disables bypass. Matching follows the installed mpv/FFmpeg backend. Concrete
hosts and domain suffixes are recommended; CIDR/complex wildcard behavior is
backend-dependent and not interpreted by cli-radio.

```toml
[network]
proxy = "http://proxy.example.org:8080"
proxy_username = ""
proxy_password = ""
no_proxy = "localhost,127.0.0.1,.internal.example.org"
```

The proxy itself must use `http://`, also for HTTPS streams, which use CONNECT.
TLS certificates are verified. HTTPS-to-proxy, SOCKS, NTLM and Kerberos are not
implemented; special company CA setup is not provided by the app.

cli-radio sets `http_proxy` and `no_proxy` only for its owned mpv child. It disables
personal mpv config and ytdl hooks; external values are never shell commands.
The local mpv 0.41.0 manpage and
[mpv environment documentation](https://mpv.io/manual/stable/#environment-variables)
were checked; loopback tests verify HTTP proxy routing, HTTPS CONNECT and bypass.

Proxy URLs passed to mpv always include an explicit port, including HTTP port 80.
Diagnostics uses the playback resolver for the active or selected station (HTTPS
proxy priority when no station exists), even before playback starts.

Proxy credentials are URL-encoded in the child environment, rather than process
arguments. They are excluded from normal logs, errors and UI; raw mpv output is
discarded. Config credentials are plain text protected by 0600 permissions, and
process environments remain accessible to authorized same-user/root processes.
Do not commit real credentials.

## Info and diagnostics

Press `i` to open Info/Diagnostics; press `i` or `Esc` to return. Opening or
closing the screen does not affect playback, reconnects or the Random timer.
It shows runtime state, actual mpv buffer estimates, mpv version (when reported
by the playback IPC session), reconnect details and the resolved XDG file paths.
Useful troubleshooting commands include the actual log location. Proxy status
is shown without credentials. Small terminals show a clipped runtime summary.

## Buffering and reliability (0.1.1)

cli-radio uses mpv as its playback backend with approximately 10 seconds of
readahead to absorb short network interruptions. Buffering stays in memory;
disk caching is explicitly disabled. The Now Playing panel shows live mpv
buffer duration and forward-buffered bytes when available, and marks active
buffering. Missing information appears as `—`; reported values are estimates.
This is target buffering, not a guarantee
that every interruption of up to 10 seconds will be inaudible: actual behavior
depends on stream, server and network conditions.

mpv runs with `--no-config`, so playback behavior does not depend on the user's
`mpv.conf`. The application defaults are `--cache=yes`, `--cache-on-disk=no`,
`--cache-pause=yes`, `--cache-pause-wait=2` and `--demuxer-readahead-secs=10`.
Buffering is not user-configurable in 0.1.2. Longer stalls use the existing
watchdog and unlimited reconnect/backoff mechanism described below; the
`--network-timeout=15` setting remains unchanged.

## Recovery and session statistics

Unexpected stream end, network/DNS/proxy/server failures, IPC loss or mpv exit
trigger automatic retries of the desired station. The first retry is immediate;
subsequent backoff delays are 1, 2, 5, 10, then at most 30 seconds, without a retry
limit. Actual recovery also depends on mpv/network/IPC timeouts. After at least
60 seconds of successful playback, a later failure resets the backoff counter.
Intentional Stop and application exit disable retries. Station changes replace
the desired stream, and stale events from the previous generation are ignored.

A single asynchronous worker owns mpv and reaps the old child before replacement.
Startup/IPC is cancellable. Observed properties and events provide metadata and
status. A 250-ms watchdog detects process exit, cache pause longer than 30 seconds
or no position progress for 60 seconds; connection startup has a 45-second limit
and mpv network operations a 15-second timeout. The TUI remains responsive.

Disconnects count only accepted unexpected interruptions of established playback.
Initial connection failures and failed retries of the same outage do not increase
the count. Once playback resumes, a later new outage does. Stop, station changes,
random changes, stale events and application exit are excluded. The counter starts
at zero each session and is not persisted.

Uptime measures cli-radio process lifetime with `Instant`, independent of station
changes, reconnects and wall-clock adjustments. It shows days after 24 hours,
for example `2d 08:14:32`.

## Logging and troubleshooting

Logs are written to `$XDG_STATE_HOME/cli-radio/cli-radio.log`, falling back to:

```bash
tail -f ~/.local/state/cli-radio/cli-radio.log
```

They help investigate disconnects, retries, mpv and proxy problems. Entries use
Unix timestamps and numeric generations without URLs, metadata or secrets.
The log rotates at roughly 2 MB with one previous file. Player logs do not disrupt
the terminal UI.

- **mpv missing:** install `mpv`. The UI stays usable and reports the problem on Play.
- **No sound:** check volume, audio device and PipeWire/PulseAudio. The integration
  smoke test uses `--ao=null` and verifies behavior rather than audible output.
- **Repeated connection failures:** check the direct stream URL, DNS, proxy,
  NO_PROXY and certificate trust. Sanitized messages intentionally omit raw errors.
- **Cannot save:** check XDG permissions, free space and TOML validity; restart after repair.
- **IPC failure:** avoid very long XDG_STATE_HOME paths because Unix sockets have
  a short path limit. Normal exit removes private socket directories; SIGKILL or
  power loss may leave stale directories.
- **Terminal cleanup:** RAII and a panic hook restore raw mode, cursor and alternate
  screen on normal exit, errors, Ctrl+C and SIGTERM. SIGKILL/power loss cannot be
  caught; use `reset` if needed.

The network reconnect has been practically tested by interrupting and restoring
connectivity. Multi-day validation, real enterprise proxies and every audio format
on multiple distributions remain outside the completed test coverage.

## Development and tests

```bash
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
python3 tests/smoke.py
git diff --check
```

Tests need no internet streams. Injected monotonic times verify random intervals
without waiting for hours. Parser, CLI side effects, language, persistence,
station management/import logic, reconnect/counter logic and responsive UI have tests.
The Python smoke test uses a loopback server, real mpv with null audio and a PTY;
its files live under target/smoke and user settings are untouched.

Modules separate application state/input/UI, lightweight i18n, random timing,
mpv worker, reconnect state machine, stations, config, proxy and logging.
The packaging-ready man page is maintained in `docs/cli-radio.1` for installation
as `/usr/share/man/man1/cli-radio.1` (or its compressed equivalent). An RPM spec
is not maintained in this repository.

## Currently not implemented

Radio-Browser API, recording, sleep timer, alarm, MPRIS/media keys, station logos,
M3U/PLS/OPML import and Last.fm scrobbling. This is not a committed roadmap.

## Third-party radio streams

cli-radio is an independent open-source internet radio player and is not affiliated
with, endorsed by, or sponsored by any radio station accessible through the application.

Station names, trademarks, stream URLs, broadcasts and related content remain the
property of their respective owners. cli-radio does not host or retransmit radio
content. Playback connects from the user's computer to the configured stream URL.

Availability and permitted use of third-party streams are subject to the terms and
rights of the respective providers. This transparency notice does not grant any
rights or licenses to third-party content.

## Author

Markus Schneider

## License

[MIT License](LICENSE). The software license does not cover third-party broadcasts,
station trademarks or other providers' content.
