#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Run,
    Help,
    Version,
    // Maintainer-only import; intentionally absent from public HELP.
    BootstrapStations,
}
pub fn parse(args: &[String]) -> Result<Action, &'static str> {
    // Validate every argument before honoring help, so unknown options never hide.
    for arg in args {
        if !matches!(
            arg.as_str(),
            "-h" | "--help" | "--version" | "--bootstrap-stations"
        ) {
            return Err("Unknown option or argument. Use cli-radio --help for usage.");
        }
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Action::Help);
    }
    if args.iter().any(|a| a == "--version") {
        return Ok(Action::Version);
    }
    if args.iter().any(|a| a == "--bootstrap-stations") {
        return Ok(Action::BootstrapStations);
    }
    Ok(Action::Run)
}
pub const HELP: &str = "cli-radio - terminal internet radio player

USAGE:
    cli-radio [OPTIONS]

OPTIONS:
    -h, --help        Show this help
    --version        Show the program version

INTERACTIVE CONTROLS:
    Up/Down, j/k     Select a station
    Enter            Play / change station
    Space            Stop / play selected station
    +, = / -         Increase / decrease volume
    f                Toggle favorite
    /                Search station names
    a / e / d        Add / edit / delete station
    r                Toggle random station mode (configurable interval; default 12 hours)
    l                Switch UI language between German and English
    q / Ctrl+C       Quit
    Search: Enter/Esc leave search; Esc in main view clears filter
    Form: Tab/Shift+Tab change field; Enter save; Esc cancel
    Delete: y confirms; n/Esc cancels (same keys in both languages)

FILES (XDG overrides supported):
    Config:   ~/.config/cli-radio/config.toml
    Stations: ~/.local/share/cli-radio/stations.toml
    Log:      ~/.local/state/cli-radio/cli-radio.log
    XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_STATE_HOME must be absolute paths.

BULK STATION EDITING:
    Stations can be edited directly in stations.toml; each needs a unique UUID.
    Quit cli-radio before editing the file manually.

LANGUAGE:
    Default UI language: German. Press l to switch to English and back.
    The language choice is saved in config.toml.

RANDOM MODE:
    Optional; off by default. Select a different personal station after the
    configured random_interval_hours (default 12; positive whole hours).
    Edit config.toml while cli-radio is closed; invalid intervals use 12 hours.
    Reconnect of the same stream preserves the timer. Stop cancels the countdown;
    deliberate play/change or enabling during playback starts a fresh interval.
    ON/OFF is saved; the countdown is not. One station: no automatic switch.

ENVIRONMENT / PROXY:
    HTTP_PROXY, HTTPS_PROXY, NO_PROXY and http_proxy, https_proxy, no_proxy.
    Explicit network.proxy overrides proxy environment variables; lowercase wins.
    HTTP proxies also carry HTTPS streams via CONNECT. No SOCKS/NTLM/Kerberos.

AUTHOR:
    Markus Schneider
";
