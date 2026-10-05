use crate::{
    random::RandomTimer,
    reconnect::{Phase, Reconnect},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Language {
    #[default]
    #[serde(rename = "de")]
    German,
    #[serde(rename = "en")]
    English,
}
impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Ok(if value.as_str() == Some("en") {
            Self::English
        } else {
            Self::German
        })
    }
}
impl Language {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::German => Self::English,
            Self::English => Self::German,
        };
    }
    pub fn text(self, english: &str) -> &str {
        if self == Self::English {
            return english;
        }
        TRANSLATIONS
            .iter()
            .find(|(key, _)| *key == english)
            .map(|(_, value)| *value)
            .unwrap_or(english)
    }
    pub fn message(self, text: &str) -> String {
        if self == Self::English {
            return text.into();
        }
        // Longest messages first also handles combined startup warnings.
        let mut result = text.to_owned();
        for (english, german) in TRANSLATIONS {
            result = result.replace(english, german);
        }
        result
    }

    pub fn status(self, reconnect: &Reconnect, now: Instant) -> String {
        match reconnect.phase {
            Phase::Playing => format!("● {}", self.text("Playing")),
            Phase::Stopped => self.text("Stopped").into(),
            Phase::Connecting => self.text("Connecting…").into(),
            Phase::Disconnected => self.text("Connection lost").into(),
            Phase::Reconnecting => format!(
                "{} {}s ({} {})",
                self.text("Reconnecting in"),
                reconnect
                    .deadline
                    .map(|d| d.saturating_duration_since(now).as_secs())
                    .unwrap_or(0),
                self.text("attempt"),
                reconnect.attempts
            ),
        }
    }
    pub fn random_status(
        self,
        enabled: bool,
        timer: &RandomTimer,
        now: Instant,
        hours: u64,
        compact: bool,
    ) -> String {
        let label = format!(
            "{}: {}",
            self.text("Random"),
            self.text(if enabled { "ON" } else { "OFF" })
        );
        if !enabled {
            return label;
        }
        match timer.remaining(now, hours) {
            Some(time) => {
                if compact {
                    format!("{label} | {}", clock(time))
                } else {
                    format!("{label} · {} {}", self.text("next switch in"), clock(time))
                }
            }
            None => {
                if compact {
                    label
                } else {
                    format!("{label} · {}", self.text("waiting for playback"))
                }
            }
        }
    }
    pub fn footer(self, width: u16) -> Vec<&'static str> {
        match (self, width < 80) {
            (Self::German, true) => vec![
                "↑↓/jk | Enter Start | Space Stopp",
                "+/- Laut. | f Fav. | / Suche | q Ende",
                "a+ | e~ | d- | r Zufall | l English",
            ],
            (Self::English, true) => vec![
                "↑↓/jk | Enter Play | Space Stop",
                "+/- Vol | f Fav | / Find | q Quit",
                "a+ | e~ | d- | r Random | l Deutsch",
            ],
            (Self::German, false) => vec![
                "↑↓/jk Auswahl | Enter Abspielen | Space Stopp/Start | +/- Lautstärke",
                "f Favorit | / Suche | a Neu | e Bearbeiten | d Löschen",
                "r Zufall | l English | q Beenden",
            ],
            (Self::English, false) => vec![
                "↑↓/jk Select | Enter Play | Space Stop/Play | +/- Volume",
                "f Favorite | / Search | a Add | e Edit | d Delete",
                "r Random | l Deutsch | q Quit",
            ],
        }
    }
}
pub fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

const TRANSLATIONS: &[(&str, &str)] = &[
    (
        "Invalid configuration/database; defaults loaded, files preserved. Repair files and restart to enable saving.",
        "Ungültige Konfiguration/Senderdatei; Standardwerte geladen, Dateien erhalten. Zum Speichern Dateien reparieren und neu starten.",
    ),
    (
        "Cannot initialize personal station list; defaults loaded for this session only",
        "Persönliche Senderliste konnte nicht angelegt werden; Standardsender nur für diese Sitzung geladen",
    ),
    (
        "Cannot save settings. Check file permissions or repair invalid TOML and restart.",
        "Speichern fehlgeschlagen. Dateirechte prüfen oder TOML reparieren und neu starten.",
    ),
    (
        "CLI Radio\nTerminal too small (minimum 38×12).\nq: Quit / Ctrl+C: Quit",
        "CLI Radio\nTerminal zu klein (mindestens 38×12).\nq / Ctrl+C: Beenden",
    ),
    (
        "Invalid/duplicate station skipped; source files preserved",
        "Ungültiger/doppelter Sender übersprungen; Originaldateien bleiben erhalten",
    ),
    (
        "Name must be nonempty and contain no control characters",
        "Name darf nicht leer sein oder Steuerzeichen enthalten",
    ),
    (
        "Invalid bundled station list; personal files preserved",
        "Ungültige Standardsenderliste; persönliche Dateien bleiben erhalten",
    ),
    (
        "mpv requires an http:// proxy URL (also for HTTPS streams)",
        "mpv benötigt eine http://-Proxy-URL (auch für HTTPS-Streams)",
    ),
    (
        "mpv is not installed; install the mpv package",
        "mpv ist nicht installiert; bitte das Paket mpv installieren",
    ),
    (
        "No station selected. Press a to add a station.",
        "Kein Sender ausgewählt. Mit a einen Sender hinzufügen.",
    ),
    (
        "Cannot read configuration/database; files preserved",
        "Konfiguration/Senderdatei nicht lesbar; Dateien bleiben erhalten",
    ),
    (
        "Credentials in station URLs are not supported",
        "Zugangsdaten in Sender-URLs werden nicht unterstützt",
    ),
    (
        "Tab: switch field | Enter: Save | Esc: Cancel",
        "Tab: Feld wechseln | Enter: Speichern | Esc: Abbrechen",
    ),
    (
        "Delete selected station?\ny: Delete | n/Esc: Cancel",
        "Ausgewählten Sender löschen?\ny: Löschen | n/Esc: Abbrechen",
    ),
    (
        "Invalid proxy configuration (details hidden)",
        "Ungültige Proxy-Konfiguration (Details verborgen)",
    ),
    (
        "Cannot create private IPC directory",
        "Privates IPC-Verzeichnis konnte nicht erstellt werden",
    ),
    ("Error", "Fehler"),
    ("No stations yet", "Noch keine Sender vorhanden"),
    (
        "Press a to add a station.",
        "Drücke a, um einen Sender hinzuzufügen.",
    ),
    (
        "Station saved. Press Enter to play.",
        "Sender gespeichert. Mit Enter abspielen.",
    ),
    (
        "Select a station · Enter to play",
        "Sender wählen · Enter zum Abspielen",
    ),
    (
        "Use an HTTP or HTTPS stream URL",
        "Eine HTTP- oder HTTPS-Stream-URL verwenden",
    ),
    (
        "mpv exited before IPC was ready",
        "mpv wurde vor dem IPC-Aufbau beendet",
    ),
    (
        "Cannot protect IPC directory",
        "IPC-Verzeichnis konnte nicht geschützt werden",
    ),
    ("mpv shut down unexpectedly", "mpv unerwartet beendet"),
    (
        "Stream stalled; restarting",
        "Stream reagiert nicht; Neustart",
    ),
    ("Invalid proxy credentials", "Ungültige Proxy-Zugangsdaten"),
    ("Stream ended unexpectedly", "Stream unerwartet beendet"),
    ("Player task unavailable", "Wiedergabe-Task nicht verfügbar"),
    ("mpv IPC connection lost", "mpv-IPC-Verbindung verloren"),
    ("mpv rejected the stream", "mpv hat den Stream abgelehnt"),
    ("mpv exited unexpectedly", "mpv unerwartet beendet"),
    ("Invalid proxy username", "Ungültiger Proxy-Benutzername"),
    ("mpv IPC startup failed", "mpv-IPC-Aufbau fehlgeschlagen"),
    (
        "mpv IPC volume failed",
        "Lautstärke konnte über mpv-IPC nicht geändert werden",
    ),
    ("waiting for playback", "wartet auf Wiedergabe"),
    ("mpv IPC setup failed", "mpv-IPC-Einrichtung fehlgeschlagen"),
    (
        "mpv IPC load failed",
        "Stream konnte über mpv-IPC nicht geladen werden",
    ),
    (
        "mpv IPC split error",
        "Fehler beim Aufteilen der mpv-IPC-Verbindung",
    ),
    ("Invalid stream URL", "Ungültige Stream-URL"),
    ("Cannot start mpv", "mpv konnte nicht gestartet werden"),
    ("Connection lost", "Verbindung verloren"),
    ("Reconnecting in", "Wiederverbinden in"),
    ("next switch in", "nächster Wechsel in"),
    ("Edit Station", "Sender bearbeiten"),
    ("Now Playing", "Aktuelle Wiedergabe"),
    ("Disconnects", "Verbindungsabbrüche"),
    ("Connecting…", "Verbinden…"),
    ("Add Station", "Sender hinzufügen"),
    ("Stations", "Sender"),
    ("Playing", "Wiedergabe"),
    ("Stopped", "Gestoppt"),
    ("attempt", "Versuch"),
    ("Confirm", "Bestätigen"),
    ("Search", "Suche"),
    ("Artist", "Interpret"),
    ("Volume", "Lautstärke"),
    ("Uptime", "Laufzeit"),
    ("Random", "Zufall"),
    ("Title", "Titel"),
    ("Drops", "Abbr."),
    ("Name", "Name"),
    ("OFF", "AUS"),
    ("Add", "Hinzufügen"),
    ("Up", "Laufz."),
    ("ON", "AN"),
];
