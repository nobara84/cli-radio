use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Stopped,
    Connecting,
    Playing,
    Disconnected,
    Reconnecting,
}
pub struct Reconnect {
    pub phase: Phase,
    pub desired: bool,
    pub generation: u64,
    pub attempts: u32,
    pub deadline: Option<Instant>,
    started: Instant,
    stable: Option<Instant>,
}
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 5,
        4 => 10,
        _ => 30,
    })
}
impl Default for Reconnect {
    fn default() -> Self {
        Self {
            phase: Phase::Stopped,
            desired: false,
            generation: 0,
            attempts: 0,
            deadline: None,
            started: Instant::now(),
            stable: None,
        }
    }
}
impl Reconnect {
    pub fn play(&mut self, now: Instant) {
        self.generation += 1;
        self.desired = true;
        self.attempts = 0;
        self.begin(now);
    }
    fn begin(&mut self, now: Instant) {
        self.phase = Phase::Connecting;
        self.started = now;
        self.deadline = None;
        self.stable = None;
    }
    pub fn stop(&mut self) {
        self.generation += 1;
        self.desired = false;
        self.phase = Phase::Stopped;
        self.deadline = None;
        self.stable = None;
    }
    pub fn playing(&mut self, generation: u64, now: Instant) {
        if self.desired && self.generation == generation {
            self.phase = Phase::Playing;
            self.stable = Some(now);
        }
    }
    pub fn lost(&mut self, generation: u64, now: Instant) -> bool {
        if !self.desired || generation != self.generation || self.deadline.is_some() {
            return false;
        }
        if self
            .stable
            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(60))
        {
            self.attempts = 0;
        }
        self.stable = None;
        self.phase = Phase::Disconnected;
        self.deadline = Some(now + backoff(self.attempts));
        self.attempts = self.attempts.saturating_add(1);
        true
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.phase == Phase::Connecting
            && now.duration_since(self.started) >= Duration::from_secs(45)
        {
            self.lost(self.generation, now);
        }
        if let Some(deadline) = self.deadline {
            self.phase = Phase::Reconnecting;
            if now >= deadline {
                self.generation += 1;
                self.begin(now);
                return true;
            }
        }
        false
    }
    pub fn status(&self, now: Instant) -> String {
        match self.phase {
            Phase::Stopped => "Stopped".into(),
            Phase::Connecting => "Connecting…".into(),
            Phase::Playing => "Playing".into(),
            _ => format!(
                "Reconnecting in {}s (attempt {})",
                self.deadline
                    .map(|d| d.saturating_duration_since(now).as_secs())
                    .unwrap_or(0),
                self.attempts
            ),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stop_and_stale_events_never_restart() {
        let mut s = Reconnect::default();
        let now = Instant::now();
        s.play(now);
        let g = s.generation;
        s.stop();
        assert!(!s.lost(g, now));
        assert!(!s.tick(now + Duration::from_secs(100)));
        s.play(now);
        s.play(now);
        assert!(!s.lost(g + 2, now));
    }
    #[test]
    fn retry_forever_and_reset_after_stability() {
        let mut s = Reconnect::default();
        let mut now = Instant::now();
        s.play(now);
        for i in 0..100 {
            assert!(s.lost(s.generation, now));
            let delay = backoff(i);
            assert!(delay.as_secs() <= 30);
            now += delay;
            assert!(s.tick(now));
        }
        s.playing(s.generation, now);
        now += Duration::from_secs(61);
        s.lost(s.generation, now);
        assert_eq!(s.attempts, 1);
        assert_eq!(s.deadline, Some(now));
    }
    #[test]
    fn delays_and_connect_timeout() {
        assert_eq!(
            (0..7).map(|i| backoff(i).as_secs()).collect::<Vec<_>>(),
            vec![0, 1, 2, 5, 10, 30, 30]
        );
        let mut s = Reconnect::default();
        let now = Instant::now();
        s.play(now);
        assert!(s.tick(now + Duration::from_secs(46)));
    }
}
