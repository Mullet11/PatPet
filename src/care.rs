//! Looking after the cat: four needs that run down in real time, experience and levels, and
//! the rooms that unlock on the way. Saved in data.json; time is passed in (Unix seconds), so
//! this stays pure and easy to test.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Food,
    Fun,
    Energy,
    Clean,
}

/// Below this a need shows on the cat (and may bring a notice).
pub const LOW: f32 = 25.0;
/// Actions only earn experience while the need is not already almost full.
const NEARLY_FULL: f32 = 90.0;
/// How much each need drops per hour (energy: while awake).
const FOOD_PER_H: f32 = 12.5;
const FUN_PER_H: f32 = 10.0;
const CLEAN_PER_H: f32 = 4.0;
const ENERGY_PER_H: f32 = 6.0;
/// Energy regained per hour of sleep.
const REST_PER_H: f32 = 25.0;
/// While PatPet is closed (or the PC sleeps) food, fun and cleanliness drop this much slower.
const AWAY_RATE: f32 = 1.0;
/// A longer gap (PatPet closed for days) counts as this long, so the cat is sad, not empty.
const MAX_GAP_S: i64 = 3 * 86400;

/// Rooms in unlock order: (file stem, name, level that unlocks it).
pub const ROOMS: [(&str, &str, u32); 6] = [
    ("box", "Cardboard box", 1),
    ("bedroom", "Bedroom", 3),
    ("living", "Living room", 5),
    ("garden", "Garden", 8),
    ("treehouse", "Treehouse", 12),
    ("castle", "Castle", 16),
];

/// Experience earned per action.
pub const XP_FEED: u32 = 5;
pub const XP_PLAY: u32 = 8;
pub const XP_BATH: u32 = 5;
pub const XP_PET: u32 = 1;
pub const XP_TASK: u32 = 10;
/// Ticked-off tasks earn experience this many times a day at most.
pub const TASKS_PER_DAY: u32 = 5;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Care {
    pub food: f32,
    pub fun: f32,
    pub energy: f32,
    pub clean: f32,
    pub level: u32,
    /// experience gathered toward the next level
    pub xp: u32,
    /// index into ROOMS of the room shown
    pub room: usize,
    /// sleeping because the user put the cat to bed (ends by itself when rested)
    pub nap: bool,
    /// when the needs were last brought up to date (Unix seconds, 0 = never)
    pub updated: i64,
    /// when the last "your cat needs..." notice was shown (Unix seconds)
    pub noticed: i64,
    /// the day (days since 1970) `tasks_today` counts for, and how many tasks earned
    /// experience on it
    pub task_day: i64,
    pub tasks_today: u32,
}

impl Default for Care {
    fn default() -> Self {
        Care { food: 80.0, fun: 80.0, energy: 80.0, clean: 80.0, level: 1, xp: 0, room: 0, nap: false, updated: 0, noticed: 0, task_day: 0, tasks_today: 0 }
    }
}

/// Experience needed to go from `level` to the next one.
pub fn xp_for(level: u32) -> u32 {
    40 + 20 * level
}

/// How many rooms are open at `level`.
pub fn rooms_open(level: u32) -> usize {
    ROOMS.iter().filter(|r| r.2 <= level).count()
}

impl Care {
    /// Run the needs down to `now`. `asleep`: the cat slept through this time (it regains
    /// energy instead of spending it); time PatPet was closed always counts as sleep.
    pub fn advance(&mut self, now: i64, asleep: bool) {
        if self.updated <= 0 || now <= self.updated {
            self.updated = self.updated.max(now);
            return;
        }
        let gap = now - self.updated;
        // more than a minute between two calls: PatPet was not running (or the PC slept)
        let away = gap > 60;
        let h = gap.min(MAX_GAP_S) as f32 / 3600.0;
        let d = if away { h * AWAY_RATE } else { h };
        self.food = (self.food - FOOD_PER_H * d).max(0.0);
        self.fun = (self.fun - FUN_PER_H * d).max(0.0);
        self.clean = (self.clean - CLEAN_PER_H * d).max(0.0);
        if asleep || self.nap || away {
            self.energy = (self.energy + REST_PER_H * h).min(100.0);
        } else {
            self.energy = (self.energy - ENERGY_PER_H * h).max(0.0);
        }
        if self.nap && self.energy >= 100.0 {
            self.nap = false;
        }
        self.updated = now;
    }

    /// The need running lowest, when it is below LOW.
    pub fn lowest(&self) -> Option<Need> {
        let all = [(Need::Food, self.food), (Need::Fun, self.fun), (Need::Energy, self.energy), (Need::Clean, self.clean)];
        all.iter().filter(|(_, v)| *v < LOW).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(n, _)| *n)
    }

    pub fn value(&self, n: Need) -> f32 {
        match n {
            Need::Food => self.food,
            Need::Fun => self.fun,
            Need::Energy => self.energy,
            Need::Clean => self.clean,
        }
    }

    fn fill(v: &mut f32, by: f32) -> bool {
        let earned = *v < NEARLY_FULL;
        *v = (*v + by).min(100.0);
        earned
    }

    /// Feeding; returns levels gained.
    pub fn feed(&mut self) -> u32 {
        let earned = Self::fill(&mut self.food, 35.0);
        self.nap = false;
        if earned {
            self.gain(XP_FEED)
        } else {
            0
        }
    }

    /// Playing makes it happy and a little tired; returns levels gained.
    pub fn play(&mut self) -> u32 {
        let earned = Self::fill(&mut self.fun, 30.0);
        self.energy = (self.energy - 5.0).max(0.0);
        self.nap = false;
        if earned {
            self.gain(XP_PLAY)
        } else {
            0
        }
    }

    /// A bath; returns levels gained.
    pub fn bath(&mut self) -> u32 {
        let earned = self.clean < NEARLY_FULL;
        self.clean = 100.0;
        self.nap = false;
        if earned {
            self.gain(XP_BATH)
        } else {
            0
        }
    }

    /// Petting: a little fun; returns levels gained.
    pub fn pet(&mut self) -> u32 {
        if Self::fill(&mut self.fun, 4.0) {
            self.gain(XP_PET)
        } else {
            0
        }
    }

    /// Put the cat to bed, or wake it up.
    pub fn toggle_nap(&mut self) {
        self.nap = !self.nap;
    }

    /// A task ticked off on `day`: experience, up to TASKS_PER_DAY tasks a day; returns
    /// levels gained.
    pub fn task_done(&mut self, day: i64) -> u32 {
        if self.task_day != day {
            self.task_day = day;
            self.tasks_today = 0;
        }
        if self.tasks_today >= TASKS_PER_DAY {
            return 0;
        }
        self.tasks_today += 1;
        self.gain(XP_TASK)
    }

    /// Add experience; returns how many levels that made.
    pub fn gain(&mut self, xp: u32) -> u32 {
        self.xp += xp;
        let mut ups = 0;
        while self.xp >= xp_for(self.level) {
            self.xp -= xp_for(self.level);
            self.level += 1;
            ups += 1;
        }
        ups
    }

    /// Show the next open room (wrapping around), or the previous one.
    pub fn next_room(&mut self, step: i32) {
        let n = rooms_open(self.level).max(1) as i32;
        self.room = ((self.room.min(n as usize - 1) as i32 + step).rem_euclid(n)) as usize;
    }

    /// The room on show, kept to the rooms already open.
    pub fn room_index(&self) -> usize {
        self.room.min(rooms_open(self.level).max(1) - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_800_000_000;

    fn fresh() -> Care {
        Care { updated: T, ..Default::default() }
    }

    #[test]
    fn needs_run_down_over_time() {
        let mut c = fresh();
        c.advance(T + 3600, false);
        assert_eq!(c.food, 80.0 - FOOD_PER_H);
        assert_eq!(c.fun, 80.0 - FUN_PER_H);
        assert_eq!(c.clean, 80.0 - CLEAN_PER_H);
        assert_eq!(c.updated, T + 3600);
    }

    #[test]
    fn small_steps_add_up_like_one_big_one() {
        let mut a = fresh();
        let mut b = fresh();
        for s in 1..=60 {
            a.advance(T + s * 60, false);
        }
        b.advance(T + 3600, false);
        // b counts the hour as time away (asleep): only energy differs
        assert!((a.food - b.food).abs() < 0.01 && (a.fun - b.fun).abs() < 0.01);
        assert!(a.energy < 80.0 && b.energy > 80.0);
    }

    #[test]
    fn first_start_only_sets_the_clock() {
        let mut c = Care::default();
        c.advance(T, false);
        assert_eq!(c, Care { updated: T, ..Default::default() });
    }

    #[test]
    fn long_absence_is_capped_and_never_negative() {
        let mut c = fresh();
        c.advance(T + 30 * 86400, false);
        assert_eq!((c.food, c.fun), (0.0, 0.0));
        assert!(c.clean >= 0.0);
        assert_eq!(c.energy, 100.0); // slept while away
        assert_eq!(c.lowest(), Some(Need::Food));
    }

    #[test]
    fn sleep_restores_energy_and_a_nap_ends_when_rested() {
        let mut c = fresh();
        c.energy = 10.0;
        c.toggle_nap();
        let mut minutes = 0;
        while c.nap && minutes < 600 {
            minutes += 1;
            c.advance(T + minutes * 60, false);
        }
        // 90 points at REST_PER_H take about 3.6 hours, then it wakes up rested
        assert!((215..=217).contains(&minutes), "{minutes}");
        assert_eq!(c.energy, 100.0);
    }

    #[test]
    fn lowest_need_only_when_low() {
        let mut c = fresh();
        assert_eq!(c.lowest(), None);
        c.clean = 20.0;
        c.fun = 10.0;
        assert_eq!(c.lowest(), Some(Need::Fun));
        assert_eq!(c.value(Need::Clean), 20.0);
    }

    #[test]
    fn actions_fill_needs_and_earn_xp_until_nearly_full() {
        let mut c = fresh();
        c.food = 50.0;
        c.feed();
        assert_eq!((c.food, c.xp), (85.0, XP_FEED));
        c.feed();
        assert_eq!((c.food, c.xp), (100.0, 2 * XP_FEED));
        c.feed(); // already full: no experience
        assert_eq!(c.xp, 2 * XP_FEED);
        c.bath();
        assert_eq!((c.clean, c.xp), (100.0, 2 * XP_FEED + XP_BATH));
        c.play();
        assert_eq!(c.energy, 75.0);
    }

    #[test]
    fn levels_and_rooms() {
        let mut c = fresh();
        assert_eq!(c.gain(xp_for(1) - 1), 0);
        assert_eq!(c.gain(1), 1);
        assert_eq!((c.level, c.xp), (2, 0));
        assert_eq!(c.gain(xp_for(2) + xp_for(3) + 5), 2);
        assert_eq!((c.level, c.xp), (4, 5));
        assert_eq!(rooms_open(1), 1);
        assert_eq!(rooms_open(4), 2);
        assert_eq!(rooms_open(16), ROOMS.len());
    }

    #[test]
    fn task_experience_is_capped_per_day() {
        let mut c = fresh();
        for _ in 0..TASKS_PER_DAY + 3 {
            c.task_done(100);
        }
        assert_eq!(c.xp + xp_for(1) * (c.level - 1), XP_TASK * TASKS_PER_DAY);
        let before = (c.level, c.xp);
        c.task_done(101); // a new day counts again
        assert_ne!((c.level, c.xp), before);
        assert_eq!(c.tasks_today, 1);
    }

    /// One simulated day: PatPet runs 08:00-17:00 with the given care (minute by minute), and
    /// is closed the rest of the day. Returns the minutes the cat spent with a need low.
    fn sim_day(c: &mut Care, day: i64, care: bool) -> u32 {
        let start = T + day * 86400;
        c.advance(start + 8 * 3600, false); // the night, PatPet closed
        let mut low = 0;
        for m in 0..9 * 60 {
            let t = start + 8 * 3600 + m * 60;
            c.advance(t, false);
            if care {
                match m {
                    0 | 240 | 480 => {
                        c.feed();
                    }
                    120 | 360 => {
                        c.play();
                    }
                    300 => {
                        c.bath();
                    }
                    _ => {}
                }
                if m % 50 == 25 {
                    c.pet();
                }
                if m == 100 || m == 250 || m == 400 {
                    c.task_done(day);
                }
            }
            if c.lowest().is_some() {
                low += 1;
            }
        }
        low
    }

    /// The numbers behind the care settings: cargo test balance_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn balance_report() {
        let mut c = fresh();
        let total = |c: &Care| (1..c.level).map(xp_for).sum::<u32>() + c.xp;
        println!("cared for: 3 meals, 2 games, 1 bath, about 10 pats, 3 tasks a day; PatPet on 08-17");
        let mut room = 1;
        let mut before = 0;
        for day in 0..60 {
            let low = sim_day(&mut c, day, true);
            if day == 0 || day == 6 {
                println!("  day {}: {} XP, low for {low} of 540 min", day + 1, total(&c) - before);
            }
            before = total(&c);
            while room < ROOMS.len() && c.level >= ROOMS[room].2 {
                println!("  {} (level {}) on day {}", ROOMS[room].1, ROOMS[room].2, day + 1);
                room += 1;
            }
        }
        println!("left alone from 80 (PatPet open, awake):");
        for need in [Need::Food, Need::Fun, Need::Energy, Need::Clean] {
            let mut x = fresh();
            let mut m = 0;
            while x.value(need) >= LOW && m < 3 * 1440 {
                m += 1;
                x.advance(T + m * 60, false);
            }
            println!("  {need:?} low after {:.1} h", m as f32 / 60.0);
        }
        let mut night = fresh();
        night.advance(T + 15 * 3600, false);
        println!(
            "after 15 h closed (evening to morning): food {:.0}, fun {:.0}, energy {:.0}, clean {:.0}",
            night.food, night.fun, night.energy, night.clean
        );
    }

    #[test]
    fn room_choice_wraps_over_open_rooms() {
        let mut c = fresh();
        c.level = 5; // box, bedroom, living room
        c.next_room(1);
        assert_eq!(c.room_index(), 1);
        c.next_room(-2);
        assert_eq!(c.room_index(), 2);
        c.room = 5; // saved by a later version, or the level dropped
        assert_eq!(c.room_index(), 2);
    }
}
