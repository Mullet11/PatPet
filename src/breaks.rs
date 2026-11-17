//! Break reminders: after a long stretch of work the cat suggests a short break, and a break
//! really taken (mouse and keyboard left alone for a while) is good for the cat too. Skipping one
//! costs nothing: the cat is never sad about it. Pure: the time and how long the user has been
//! idle are passed in (seconds), about once a second.

/// The "time for a break?" notice stays this long; letting it go means "later".
pub const ASK_SECS: u64 = 40;
/// A break lasts this long.
pub const REST_SECS: u64 = 5 * 60;
/// Mouse and keyboard left alone this long during a break: it counts.
const RESTED_IDLE: u64 = 3 * 60;
/// Touched within this: the user is working.
const ACTIVE_IDLE: u64 = 60;
/// Away this long without being asked: that was a break too (the count starts again, quietly).
const NATURAL_BREAK: u64 = 5 * 60;
/// "Later" asks again after this much more work, at most MAX_LATER times in a row.
const LATER: u64 = 15 * 60;
const MAX_LATER: u8 = 2;
/// Longer gaps between two calls (the PC slept) are not counted as work.
const MAX_STEP: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Working,
    /// the notice is up
    Asked { at: u64 },
    /// on a break since `since`; `rested`: left alone long enough already
    Resting { since: u64, rested: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// time to suggest a break
    Due,
    /// the suggestion went by without a tap (counts as "later")
    Ignored,
    /// a break was taken: reward the cat
    Taken,
    /// a break was started but the user kept working: it ends quietly
    Ended,
}

#[derive(Debug)]
pub struct Breaks {
    /// seconds of work since the last break
    pub worked: u64,
    later: u8,
    phase: Phase,
    last: Option<u64>,
}

impl Default for Breaks {
    fn default() -> Self {
        Breaks { worked: 0, later: 0, phase: Phase::Working, last: None }
    }
}

impl Breaks {
    #[cfg(test)]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn resting(&self) -> bool {
        matches!(self.phase, Phase::Resting { .. })
    }

    /// Seconds left of the break being taken.
    pub fn rest_left(&self, now: u64) -> Option<u64> {
        match self.phase {
            Phase::Resting { since, .. } => Some(REST_SECS.saturating_sub(now.saturating_sub(since))),
            _ => None,
        }
    }

    /// `now`: seconds on any steady clock; `idle`: seconds since the last mouse or keyboard
    /// input; `busy`: not a good moment to ask (full screen, another notice up); `after`:
    /// seconds of work before a break is suggested.
    pub fn tick(&mut self, now: u64, idle: u64, busy: bool, after: u64) -> Option<Event> {
        let dt = self.last.map_or(0, |l| now.saturating_sub(l)).min(MAX_STEP);
        self.last = Some(now);
        match self.phase {
            Phase::Working => {
                if idle >= NATURAL_BREAK {
                    self.reset();
                    return None;
                }
                if idle < ACTIVE_IDLE {
                    self.worked += dt;
                }
                if self.worked >= after && !busy {
                    self.phase = Phase::Asked { at: now };
                    return Some(Event::Due);
                }
                None
            }
            Phase::Asked { at } => {
                if now.saturating_sub(at) >= ASK_SECS {
                    self.later();
                    return Some(Event::Ignored);
                }
                None
            }
            Phase::Resting { since, rested } => {
                let rested = rested || idle >= RESTED_IDLE;
                self.phase = Phase::Resting { since, rested };
                if now.saturating_sub(since) < REST_SECS {
                    return None;
                }
                self.reset();
                Some(if rested { Event::Taken } else { Event::Ended })
            }
        }
    }

    /// The user tapped the suggestion: the break starts now.
    pub fn start(&mut self, now: u64) {
        self.phase = Phase::Resting { since: now, rested: false };
    }

    /// Not now: ask again after a little more work, or let it go after a few times.
    pub fn later(&mut self) {
        self.later += 1;
        if self.later > MAX_LATER {
            self.reset();
        } else {
            self.worked = self.worked.saturating_sub(LATER);
            self.phase = Phase::Working;
        }
    }

    fn reset(&mut self) {
        self.worked = 0;
        self.later = 0;
        self.phase = Phase::Working;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFTER: u64 = 50 * 60;

    /// Runs `secs` seconds, one call a second, with the given idle time; returns the events.
    fn run(b: &mut Breaks, t: &mut u64, secs: u64, idle: impl Fn(u64) -> u64, busy: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        let start = *t;
        for _ in 0..secs {
            *t += 1;
            if let Some(e) = b.tick(*t, idle(*t - start), busy, AFTER) {
                ev.push(e);
            }
        }
        ev
    }

    #[test]
    fn asks_after_fifty_minutes_of_work() {
        let (mut b, mut t) = (Breaks::default(), 0);
        assert!(run(&mut b, &mut t, AFTER - 2, |_| 0, false).is_empty());
        assert_eq!(run(&mut b, &mut t, 3, |_| 0, false), vec![Event::Due]);
    }

    #[test]
    fn reading_without_touching_still_counts_as_work() {
        let (mut b, mut t) = (Breaks::default(), 0);
        // idle up to 59 s at a time (reading), then a touch
        assert_eq!(run(&mut b, &mut t, AFTER + 1, |s| s % 60, false), vec![Event::Due]);
    }

    #[test]
    fn being_away_on_its_own_starts_the_count_again() {
        let (mut b, mut t) = (Breaks::default(), 0);
        run(&mut b, &mut t, 40 * 60, |_| 0, false);
        run(&mut b, &mut t, NATURAL_BREAK + 1, |s| s, false);
        assert_eq!(b.worked, 0);
        assert!(run(&mut b, &mut t, 40 * 60, |_| 0, false).is_empty());
    }

    #[test]
    fn waits_while_busy() {
        let (mut b, mut t) = (Breaks::default(), 0);
        assert!(run(&mut b, &mut t, AFTER + 600, |_| 0, true).is_empty());
        assert_eq!(run(&mut b, &mut t, 1, |_| 0, false), vec![Event::Due]);
    }

    #[test]
    fn an_ignored_notice_asks_again_later_then_lets_go() {
        let (mut b, mut t) = (Breaks::default(), 0);
        run(&mut b, &mut t, AFTER + 1, |_| 0, false);
        assert_eq!(b.phase(), Phase::Asked { at: t });
        assert_eq!(run(&mut b, &mut t, ASK_SECS, |_| 0, false), vec![Event::Ignored]);
        // asked again 15 minutes of work later, twice, then not until the next 50 minutes
        assert_eq!(run(&mut b, &mut t, LATER + 1, |_| 0, false), vec![Event::Due]);
        run(&mut b, &mut t, ASK_SECS, |_| 0, false);
        assert_eq!(run(&mut b, &mut t, LATER + 1, |_| 0, false), vec![Event::Due]);
        run(&mut b, &mut t, ASK_SECS, |_| 0, false);
        assert!(run(&mut b, &mut t, AFTER - 60, |_| 0, false).is_empty());
    }

    #[test]
    fn a_break_taken_is_rewarded() {
        let (mut b, mut t) = (Breaks::default(), 0);
        run(&mut b, &mut t, AFTER, |_| 0, false);
        b.start(t);
        assert_eq!(b.rest_left(t), Some(REST_SECS));
        // a minute to get up, then left alone
        let ev = run(&mut b, &mut t, REST_SECS, |s| s.saturating_sub(60), false);
        assert_eq!(ev, vec![Event::Taken]);
        assert_eq!((b.phase(), b.worked), (Phase::Working, 0));
    }

    #[test]
    fn a_break_spent_working_ends_quietly() {
        let (mut b, mut t) = (Breaks::default(), 0);
        run(&mut b, &mut t, AFTER, |_| 0, false);
        b.start(t);
        assert_eq!(run(&mut b, &mut t, REST_SECS, |s| s % 30, false), vec![Event::Ended]);
        assert_eq!(b.worked, 0);
    }

    #[test]
    fn sleep_is_not_work() {
        let (mut b, mut t) = (Breaks::default(), 0);
        run(&mut b, &mut t, 60, |_| 0, false);
        // the PC slept for two hours: the next call comes much later, with no input since resume
        t += 2 * 3600;
        assert_eq!(b.tick(t, 0, false, AFTER), None);
        assert!(b.worked <= 60 + MAX_STEP);
    }
}
