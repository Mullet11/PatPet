//! Satu timer hitung mundur. Sisa waktu dihitung dari waktu target, jadi
//! tidak bergeser dan tetap benar setelah laptop tidur. Waktu dikirim dari luar.

use std::time::{Duration, Instant};

pub const MAX: Duration = Duration::from_secs(24 * 3600 - 1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Running { until: Instant },
    Paused { left: Duration },
}

#[derive(Debug)]
pub struct Countdown {
    state: State,
    /// Total durasi putaran ini; ikut berubah bila waktu ditambah.
    total: Duration,
}

impl Countdown {
    pub fn new() -> Self {
        Self { state: State::Idle, total: Duration::ZERO }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn is_active(&self) -> bool {
        self.state != State::Idle
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, State::Running { .. })
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        match self.state {
            State::Idle => Duration::ZERO,
            State::Running { until } => until.saturating_duration_since(now),
            State::Paused { left } => left,
        }
    }

    /// 0 di awal, 1 saat habis.
    pub fn progress(&self, now: Instant) -> f32 {
        if self.total.is_zero() {
            return 0.0;
        }
        (1.0 - self.remaining(now).as_secs_f32() / self.total.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn start(&mut self, duration: Duration, now: Instant) {
        if self.is_active() || duration.is_zero() {
            return;
        }
        self.total = duration.min(MAX);
        self.state = State::Running { until: now + self.total };
    }

    pub fn pause(&mut self, now: Instant) {
        if self.is_running() {
            self.state = State::Paused { left: self.remaining(now) };
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if let State::Paused { left } = self.state {
            self.state = State::Running { until: now + left };
        }
    }

    pub fn stop(&mut self) {
        self.state = State::Idle;
    }

    /// `true` tepat sekali saat waktu habis.
    pub fn finished(&mut self, now: Instant) -> bool {
        if matches!(self.state, State::Running { until } if now >= until) {
            self.state = State::Idle;
            return true;
        }
        false
    }
}

/// "25:00", atau "1:05:09" bila lebih dari satu jam. Detik dibulatkan ke atas
/// supaya "00:00" hanya tampil saat benar-benar habis.
pub fn format(d: Duration) -> String {
    let s = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn counts_down_and_finishes_once() {
        let t0 = Instant::now();
        let mut c = Countdown::new();
        c.start(secs(60), t0);
        assert_eq!(c.remaining(t0 + secs(20)), secs(40));
        assert!(!c.finished(t0 + secs(59)));
        assert!(c.finished(t0 + secs(60)));
        assert!(!c.finished(t0 + secs(61)));
        assert!(!c.is_active());
    }

    #[test]
    fn pause_freezes_remaining() {
        let t0 = Instant::now();
        let mut c = Countdown::new();
        c.start(secs(60), t0);
        c.pause(t0 + secs(10));
        assert_eq!(c.remaining(t0 + secs(500)), secs(50));
        c.resume(t0 + secs(500));
        assert_eq!(c.remaining(t0 + secs(510)), secs(40));
    }

    #[test]
    fn zero_duration_does_not_start() {
        let mut c = Countdown::new();
        c.start(Duration::ZERO, Instant::now());
        assert!(!c.is_active());
    }

    #[test]
    fn progress_goes_from_zero_to_one() {
        let t0 = Instant::now();
        let mut c = Countdown::new();
        c.start(secs(100), t0);
        assert_eq!(c.progress(t0), 0.0);
        assert!((c.progress(t0 + secs(25)) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn formats_like_a_clock() {
        assert_eq!(format(secs(1500)), "25:00");
        assert_eq!(format(Duration::from_millis(59_001)), "01:00");
        assert_eq!(format(secs(3909)), "1:05:09");
        assert_eq!(format(Duration::ZERO), "00:00");
    }
}
