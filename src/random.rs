use std::time::{Duration, Instant};
pub const DEFAULT_HOURS: u64 = 12;
pub fn interval(hours: u64) -> Duration {
    Duration::from_secs(
        hours
            .checked_mul(3600)
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_HOURS * 3600),
    )
}
#[derive(Default)]
pub struct RandomTimer {
    started: Option<Instant>,
}
impl RandomTimer {
    pub fn start(&mut self, now: Instant) {
        self.started = Some(now);
    }
    pub fn stop(&mut self) {
        self.started = None;
    }
    pub fn remaining(&self, now: Instant, hours: u64) -> Option<Duration> {
        self.started
            .map(|start| interval(hours).saturating_sub(now.saturating_duration_since(start)))
    }
    pub fn due(&self, now: Instant, hours: u64) -> bool {
        self.remaining(now, hours) == Some(Duration::ZERO)
    }
}

pub fn default_hours() -> u64 {
    DEFAULT_HOURS
}
pub fn deserialize_hours<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    // A TOML value lets invalid strings, booleans, floats and negative integers
    // fall back without discarding unrelated valid configuration fields.
    let value = <toml::Value as serde::Deserialize>::deserialize(deserializer)?;
    Ok(value
        .as_integer()
        .and_then(|v| u64::try_from(v).ok())
        .filter(|v| *v > 0 && v.checked_mul(3600).is_some())
        .unwrap_or(DEFAULT_HOURS))
}
