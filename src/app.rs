use crate::{
    config::{Config, Store},
    logging::Log,
    player::{Control, Notice},
    reconnect::{Phase, Reconnect},
    stations::{Station, filtered, validate},
};
use rand::seq::IteratorRandom;
use std::{collections::HashMap, time::Instant};
use tokio::sync::mpsc;
use uuid::Uuid;
#[derive(Default)]
pub struct Metadata {
    pub artist: String,
    pub title: String,
    pub codec: String,
    pub bitrate: String,
}
pub enum Mode {
    Normal,
    Info,
    Search,
    Form {
        id: Option<Uuid>,
        name: String,
        url: String,
        field: bool,
    },
    Delete(Uuid),
}
pub struct App {
    pub random_timer: crate::random::RandomTimer,
    pub disconnects: u64,
    pub session_started: Instant,
    pub config: Config,
    pub stations: Vec<Station>,
    pub selected: usize,
    pub search: String,
    pub mode: Mode,
    pub reconnect: Reconnect,
    pub active: Option<Station>,
    pub metadata: Metadata,
    pub cache: crate::cache::Cache,
    pub mpv_version: Option<String>,
    pub message: String,
    pub store: Store,
    pub log: Log,
    pub controls: mpsc::Sender<Control>,
    pub environment: HashMap<String, String>,
}
impl App {
    pub fn visible(&self) -> Vec<usize> {
        filtered(&self.stations, &self.search)
    }
    pub fn selected_station(&self) -> Option<&Station> {
        self.visible()
            .get(self.selected)
            .map(|&i| &self.stations[i])
    }
    pub fn save(&mut self) {
        if self.store.save(&self.config, &self.stations).is_err() {
            self.message =
                "Cannot save settings. Check file permissions or repair invalid TOML and restart."
                    .into();
            self.log
                .event("persistence failed", self.reconnect.generation);
        }
    }
    pub async fn play(&mut self) {
        self.play_at(Instant::now()).await;
    }
    pub async fn play_at(&mut self, now: Instant) {
        if let Some(station) = self.selected_station().cloned() {
            self.start_station(station, now).await;
        } else {
            self.message = "No station selected. Press a to add a station.".into();
        }
    }
    async fn start_station(&mut self, station: Station, now: Instant) {
        self.config.last_station = Some(station.id);
        self.active = Some(station);
        self.random_timer.start(now);
        self.reconnect.play(now);
        self.metadata = Metadata::default();
        self.cache = crate::cache::Cache::default();
        self.log
            .event("station selected", self.reconnect.generation);
        self.launch().await;
        self.save();
    }
    pub fn toggle_random(&mut self, now: Instant) {
        self.config.random_mode = !self.config.random_mode;
        if self.config.random_mode && self.reconnect.desired {
            self.random_timer.start(now);
        }
        self.save();
    }
    pub async fn random_tick(&mut self, now: Instant) -> bool {
        if !self.config.random_mode
            || !self.reconnect.desired
            || self.reconnect.phase != Phase::Playing
            || !self
                .random_timer
                .due(now, self.config.random_interval_hours)
        {
            return false;
        }
        let Some(active) = &self.active else {
            return false;
        };
        let station = self
            .stations
            .iter()
            .filter(|s| s.id != active.id)
            .choose(&mut rand::rng())
            .cloned();
        if let Some(station) = station {
            if let Some(index) = self
                .visible()
                .iter()
                .position(|&i| self.stations[i].id == station.id)
            {
                self.selected = index;
            }
            self.log.event(
                "intentional random station switch",
                self.reconnect.generation,
            );
            self.start_station(station, now).await;
            return true;
        }
        false
    }
    pub async fn launch(&mut self) {
        let Some(station) = &self.active else {
            return;
        };
        match self.config.network.resolve(&station.url, &self.environment) {
            Ok(proxy) => {
                self.metadata = Metadata::default();
                self.cache = crate::cache::Cache::default();
                if self
                    .controls
                    .send(Control::Play {
                        generation: self.reconnect.generation,
                        url: station.url.clone(),
                        volume: self.config.volume,
                        proxy,
                    })
                    .await
                    .is_err()
                {
                    self.message = "Player task unavailable".into();
                    self.reconnect.stop();
                } else {
                    self.log
                        .event("mpv start/restart requested", self.reconnect.generation);
                }
            }
            Err(error) => {
                self.message = error.into();
                self.reconnect
                    .lost(self.reconnect.generation, Instant::now());
                self.log
                    .event("proxy configuration error", self.reconnect.generation);
            }
        }
    }
    pub async fn stop(&mut self) {
        self.random_timer.stop();
        self.reconnect.stop();
        self.metadata = Metadata::default();
        self.cache = crate::cache::Cache::default();
        let _ = self.controls.send(Control::Stop).await;
        self.log
            .event("intentional stop", self.reconnect.generation);
    }
    pub fn notice(&mut self, notice: Notice) {
        match notice {
            Notice::Playing(g) if g == self.reconnect.generation && self.reconnect.desired => {
                self.reconnect.playing(g, Instant::now());
                self.message.clear();
                self.log.event("playing", g);
            }
            Notice::Lost(g, error) => {
                let was_playing = self.reconnect.phase == Phase::Playing;
                if self.reconnect.lost(g, Instant::now()) {
                    if was_playing {
                        self.disconnects = self.disconnects.saturating_add(1);
                    }
                    self.cache = crate::cache::Cache::default();
                    self.message = error.into();
                    self.log.event("unexpected disconnect", g);
                }
            }
            Notice::Property(g, name, data)
                if g == self.reconnect.generation && self.reconnect.desired =>
            {
                match name.as_str() {
                    "mpv-version" => {
                        self.mpv_version =
                            data.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
                    }
                    "demuxer-cache-state" => self.cache.update(&data),
                    "paused-for-cache" => {
                        let paused = data.as_bool().unwrap_or(false);
                        if paused != self.cache.paused {
                            self.log.event(
                                if paused {
                                    "cache buffering started"
                                } else {
                                    "cache buffering ended"
                                },
                                g,
                            );
                        }
                        self.cache.paused = paused;
                    }
                    "metadata" => {
                        self.metadata.artist.clear();
                        self.metadata.title.clear();
                        if let Some(map) = data.as_object() {
                            for (key, val) in map {
                                let value = val.as_str().unwrap_or("").to_string();
                                match key.to_lowercase().as_str() {
                                    "artist" => self.metadata.artist = value,
                                    "title" | "icy-title" => self.metadata.title = value,
                                    _ => {}
                                }
                            }
                        }
                    }
                    "audio-codec-name" => self.metadata.codec = data.as_str().unwrap_or("").into(),
                    "audio-bitrate" => {
                        self.metadata.bitrate = data
                            .as_f64()
                            .map(|n| format!("{:.0} kbps", n / 1000.0))
                            .unwrap_or_default()
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    pub fn form_commit(&mut self) {
        if let Mode::Form { id, name, url, .. } = &self.mode {
            if let Err(e) = validate(name, url) {
                self.message = e.into();
                return;
            }
            if let Some(id) = id {
                if let Some(s) = self.stations.iter_mut().find(|s| s.id == *id) {
                    s.name = name.trim().into();
                    s.url = url.trim().into();
                }
            } else if let Ok(s) = Station::new(name, url) {
                self.stations.push(s);
            }
            self.mode = Mode::Normal;
            self.message = "Station saved. Press Enter to play.".into();
            self.save();
        }
    }
}
