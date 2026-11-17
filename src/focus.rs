//! Focus sessions: pick a length, and the cat keeps you company until the time is up. Any
//! length works as well as the classic 25 minutes, and stopping early costs nothing: the minutes
//! already done still count. Saved in data.json (Unix seconds), so a session survives a restart.

use serde::{Deserialize, Serialize};

/// Lengths offered (minutes).
pub const LENGTHS: [u32; 4] = [15, 25, 45, 60];
const DEFAULT_LENGTH: u32 = 25;
/// Experience: 1 for every 5 minutes of focus, at most this much a day.
const MIN_PER_XP: i64 = 5;
pub const XP_PER_DAY: u32 = 12;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Focus {
    /// when the running session started and ends (Unix seconds)
    pub started: i64,
    pub until: Option<i64>,
    /// minutes picked for the next session (0 = not picked yet)
    pub length: u32,
    /// the day (days since 1970) `xp_today` counts for
    pub day: i64,
    pub xp_today: u32,
}

impl Focus {
    /// Minutes the next session lasts.
    pub fn minutes(&self) -> u32 {
        if LENGTHS.contains(&self.length) {
            self.length
        } else {
            DEFAULT_LENGTH
        }
    }

    pub fn running(&self) -> bool {
        self.until.is_some()
    }

    /// Seconds left of the running session.
    pub fn left(&self, now: i64) -> Option<i64> {
        self.until.map(|u| (u - now).max(0))
    }

    pub fn start(&mut self, now: i64) {
        self.started = now;
        self.until = Some(now + i64::from(self.minutes()) * 60);
    }

    /// Stop the running session now; returns the experience it earned.
    pub fn stop(&mut self, now: i64, day: i64) -> u32 {
        match self.until.take() {
            Some(until) => self.earn(now.min(until) - self.started, day),
            None => 0,
        }
    }

    /// Call now and then: `Some(experience)` once, when the running session is over.
    pub fn tick(&mut self, now: i64, day: i64) -> Option<u32> {
        match self.until {
            Some(until) if now >= until => Some(self.stop(now, day)),
            _ => None,
        }
    }

    fn earn(&mut self, secs: i64, day: i64) -> u32 {
        if self.day != day {
            self.day = day;
            self.xp_today = 0;
        }
        let xp = (secs.max(0) / 60 / MIN_PER_XP) as u32;
        let xp = xp.min(XP_PER_DAY - self.xp_today.min(XP_PER_DAY));
        self.xp_today += xp;
        xp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_800_000_000;

    #[test]
    fn a_session_runs_its_length_and_earns_experience() {
        let mut f = Focus { length: 25, ..Default::default() };
        f.start(T);
        assert_eq!(f.left(T + 60), Some(24 * 60));
        assert_eq!(f.tick(T + 24 * 60, 1), None);
        assert_eq!(f.tick(T + 25 * 60, 1), Some(5));
        assert!(!f.running());
        assert_eq!(f.tick(T + 26 * 60, 1), None);
    }

    #[test]
    fn stopping_early_keeps_the_minutes_done() {
        let mut f = Focus { length: 45, ..Default::default() };
        f.start(T);
        assert_eq!(f.stop(T + 17 * 60, 1), 3);
        assert_eq!(f.stop(T + 18 * 60, 1), 0); // nothing running any more
    }

    #[test]
    fn experience_is_capped_per_day() {
        let mut f = Focus { length: 60, ..Default::default() };
        f.start(T);
        assert_eq!(f.tick(T + 3600, 1), Some(12));
        f.start(T + 3600);
        assert_eq!(f.tick(T + 7200, 1), Some(0));
        f.start(T + 7200);
        assert_eq!(f.tick(T + 3 * 3600, 2), Some(12)); // a new day
    }

    #[test]
    fn a_session_that_ended_while_patpet_was_closed_counts_once() {
        let mut f = Focus { length: 15, ..Default::default() };
        f.start(T);
        assert_eq!(f.tick(T + 5 * 3600, 1), Some(3)); // only its own 15 minutes
    }

    #[test]
    fn unknown_lengths_fall_back_to_25_minutes() {
        assert_eq!(Focus { length: 7, ..Default::default() }.minutes(), 25);
        assert_eq!(Focus::default().minutes(), 25);
    }
}
