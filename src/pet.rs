//! The cat's brain: takes signals (music, idle time, how it is cared for, petting) and decides
//! which animation plays, and which frame of it is showing.
//!
//! Priority: an action being played out (eating, a bath, a level-up...) > a nap > music >
//! asleep from inactivity > a need running low (hungry, sad, sleepy, dirty) > walking > idle,
//! with a yawn at night and a little fidget (grooming, tail wag, stretch) now and then.
//! Time is always passed in, so this stays pure and easy to test.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::care::Need;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Music,
    Sleep,
    Happy,
    Yawn,
    Walk,
    Eat,
    Play,
    Pounce,
    Bath,
    Shake,
    Sad,
    Hungry,
    Dirty,
    Sleepy,
    Stretch,
    Groom,
    TailWag,
    LevelUp,
}

/// Something done to the cat that it acts out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Feed,
    Play,
    Bath,
    LevelUp,
    /// "time for a break?"
    Stretch,
}

const MIN_HOLD: Duration = Duration::from_millis(1500);
const CLICK_COOLDOWN: Duration = Duration::from_secs(2);
const YAWN_EVERY: Duration = Duration::from_secs(10 * 60);
/// A fidget (grooming, tail wag, stretch) comes after this long plus up to FIDGET_SPREAD.
const FIDGET_AFTER: Duration = Duration::from_secs(90);
const FIDGET_SPREAD: u32 = 60;

impl Mood {
    /// (frames in the strip, frames per second, loops)
    pub fn strip(self) -> (usize, f32, bool) {
        match self {
            Mood::Idle => (8, 6.0, true),
            Mood::Music => (8, 8.0, true),
            Mood::Sleep => (8, 4.0, true),
            Mood::Happy => (6, 10.0, false),
            Mood::Yawn => (6, 6.0, false),
            Mood::Walk => (8, 8.0, true),
            Mood::Eat => (6, 6.0, true),
            Mood::Play => (8, 8.0, true),
            Mood::Pounce => (6, 10.0, false),
            Mood::Bath => (6, 6.0, true),
            Mood::Shake => (6, 10.0, false),
            Mood::Sad => (6, 4.0, true),
            Mood::Hungry => (6, 5.0, true),
            Mood::Dirty => (6, 5.0, true),
            Mood::Sleepy => (6, 4.0, true),
            Mood::Stretch => (6, 6.0, false),
            Mood::Groom => (8, 8.0, true),
            Mood::TailWag => (6, 6.0, true),
            Mood::LevelUp => (8, 8.0, false),
        }
    }

    /// How long the strip takes to play once.
    fn once(self) -> Duration {
        let (frames, fps, _) = self.strip();
        Duration::from_secs_f32(frames as f32 / fps)
    }
}

pub struct Pet {
    mood: Mood,
    /// Goes up every time an animation has to start again from its first frame.
    generation: u32,
    playing: bool,
    napping: bool,
    resting: bool,
    walking: bool,
    need: Option<Need>,
    idle: Duration,
    sleep_after: Duration,
    /// the animation being played out, since when and for how long, and what follows it
    shot: Option<(Mood, Instant, Duration)>,
    queue: VecDeque<(Mood, Duration)>,
    last_change: Option<Instant>,
    last_click: Option<Instant>,
    last_yawn: Option<Instant>,
    next_fidget: Option<Instant>,
    rng: u32,
    /// the generation the frame clock belongs to, and when that animation started
    anim: (u32, Option<Instant>),
}

impl Pet {
    pub fn new(sleep_after: Duration) -> Self {
        Self {
            mood: Mood::Idle,
            generation: 0,
            playing: false,
            napping: false,
            resting: false,
            walking: false,
            need: None,
            idle: Duration::ZERO,
            sleep_after,
            shot: None,
            queue: VecDeque::new(),
            last_change: None,
            last_click: None,
            last_yawn: None,
            next_fidget: None,
            rng: 0x2545_F491,
            anim: (u32::MAX, None),
        }
    }

    /// The frame showing at `now` (index into the mood's strip). An action that has played
    /// out hands over to whatever comes next here.
    pub fn frame(&mut self, now: Instant) -> (Mood, usize) {
        self.advance_shot(now);
        if self.anim.0 != self.generation || self.anim.1.is_none() {
            self.anim = (self.generation, Some(now));
        }
        let start = self.anim.1.unwrap_or(now);
        let (frames, fps, loops) = self.mood.strip();
        let i = (now.saturating_duration_since(start).as_secs_f32() * fps) as usize;
        let i = if loops || self.shot.is_some() { i % frames } else { i.min(frames - 1) };
        (self.mood, i)
    }

    /// How long until the frame changes (for pacing redraws).
    pub fn frame_period(&self) -> Duration {
        Duration::from_secs_f32(1.0 / self.mood.strip().1)
    }

    #[cfg(test)]
    pub fn mood(&self) -> Mood {
        self.mood
    }

    #[cfg(test)]
    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn is_night(hour: u32) -> bool {
        hour >= 23 || hour < 4
    }

    pub fn is_walking(&self) -> bool {
        self.mood == Mood::Walk
    }

    pub fn set_music(&mut self, playing: bool, now: Instant) -> bool {
        self.playing = playing;
        self.update(now, false)
    }

    /// What the care state says: put to bed, and the need running lowest (if any is low).
    pub fn set_care(&mut self, napping: bool, need: Option<Need>, now: Instant) -> bool {
        self.napping = napping;
        self.need = need;
        self.update(now, false)
    }

    /// The user is on a break: the cat curls up and rests with them.
    pub fn set_resting(&mut self, resting: bool, now: Instant) -> bool {
        self.resting = resting;
        self.update(now, false)
    }

    /// The cat is strolling around its room (only while the room is on screen).
    pub fn set_walking(&mut self, walking: bool, now: Instant) -> bool {
        self.walking = walking;
        self.update(now, false)
    }

    /// Petting; `false` while still in the cooldown since the last time.
    pub fn click(&mut self, now: Instant) -> bool {
        if self.last_click.is_some_and(|t| now - t < CLICK_COOLDOWN) {
            return false;
        }
        self.last_click = Some(now);
        self.cheer(now)
    }

    /// A happy jump, e.g. when a task is ticked off.
    pub fn cheer(&mut self, now: Instant) -> bool {
        self.play_out(&[(Mood::Happy, Mood::Happy.once())], now)
    }

    /// Act out something done to the cat.
    pub fn act(&mut self, a: Action, now: Instant) -> bool {
        let secs = Duration::from_secs_f32;
        match a {
            Action::Feed => self.play_out(&[(Mood::Eat, secs(2.5)), (Mood::Happy, Mood::Happy.once())], now),
            Action::Play => self.play_out(&[(Mood::Play, secs(3.0)), (Mood::Pounce, Mood::Pounce.once()), (Mood::Happy, Mood::Happy.once())], now),
            Action::Bath => self.play_out(&[(Mood::Bath, secs(3.0)), (Mood::Shake, Mood::Shake.once())], now),
            Action::LevelUp => self.play_out(&[(Mood::LevelUp, Mood::LevelUp.once() * 2)], now),
            Action::Stretch => self.play_out(&[(Mood::Stretch, Mood::Stretch.once()), (Mood::Yawn, Mood::Yawn.once())], now),
        }
    }

    /// Called now and then. `idle` = how long the user has not touched mouse or keyboard.
    pub fn tick(&mut self, now: Instant, idle: Duration, hour: u32) -> bool {
        self.idle = idle;
        self.advance_shot(now);
        let calm = self.shot.is_none() && self.base() == Mood::Idle;

        let yawn_due = self.last_yawn.map_or(true, |t| now - t >= YAWN_EVERY);
        if calm && Self::is_night(hour) && yawn_due {
            self.last_yawn = Some(now);
            return self.play_out(&[(Mood::Yawn, Mood::Yawn.once())], now);
        }

        let next = *self.next_fidget.get_or_insert_with(|| now + FIDGET_AFTER);
        if !calm {
            // nothing to fidget about while busy: start counting again once it is calm
            self.next_fidget = None;
        } else if now >= next {
            let r = self.random();
            self.next_fidget = Some(now + FIDGET_AFTER + Duration::from_secs((r % FIDGET_SPREAD) as u64));
            let f = match r % 3 {
                0 => (Mood::Groom, Duration::from_secs(3)),
                1 => (Mood::TailWag, Duration::from_secs(3)),
                _ => (Mood::Stretch, Mood::Stretch.once()),
            };
            return self.play_out(&[f], now);
        }
        self.update(now, false)
    }

    fn random(&mut self) -> u32 {
        // xorshift: plenty for picking a fidget
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// Start a sequence of animations right away (replacing one already playing).
    fn play_out(&mut self, seq: &[(Mood, Duration)], now: Instant) -> bool {
        self.queue = seq.iter().copied().collect();
        let restart = self.queue.front().map(|(m, _)| *m) == Some(self.mood);
        self.shot = None;
        self.next_shot(now);
        if restart {
            self.generation = self.generation.wrapping_add(1); // play it again from the start
        }
        self.update(now, true);
        true
    }

    fn next_shot(&mut self, now: Instant) {
        self.shot = self.queue.pop_front().map(|(m, d)| (m, now, d));
    }

    /// Move on through the sequence once the current animation has had its time.
    fn advance_shot(&mut self, now: Instant) {
        let mut moved = false;
        while let Some((_, since, d)) = self.shot {
            if now.saturating_duration_since(since) < d {
                break;
            }
            self.next_shot(since + d);
            moved = true;
        }
        if moved {
            self.update(now, true);
        }
    }

    /// The mood when no action is being played out.
    fn base(&self) -> Mood {
        if self.napping || self.resting {
            Mood::Sleep
        } else if self.playing {
            Mood::Music
        } else if self.idle >= self.sleep_after {
            Mood::Sleep
        } else if let Some(n) = self.need {
            match n {
                Need::Food => Mood::Hungry,
                Need::Fun => Mood::Sad,
                Need::Energy => Mood::Sleepy,
                Need::Clean => Mood::Dirty,
            }
        } else if self.walking {
            Mood::Walk
        } else {
            Mood::Idle
        }
    }

    /// Returns `true` when the picture has to be redrawn.
    fn update(&mut self, now: Instant, force: bool) -> bool {
        let before = self.generation;
        let next = self.shot.map(|(m, _, _)| m).unwrap_or_else(|| self.base());
        if next == self.mood {
            return self.generation != before;
        }
        // hold a moment, so the cat does not flicker between two moods
        let may_switch = force || self.last_change.map_or(true, |t| now - t >= MIN_HOLD);
        if !may_switch {
            return self.generation != before;
        }
        self.mood = next;
        self.last_change = Some(now);
        self.generation = self.generation.wrapping_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u32 = 14;
    const NIGHT: u32 = 23;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn ms(s: u64) -> Duration {
        Duration::from_millis(s)
    }

    fn pet() -> (Pet, Instant) {
        (Pet::new(secs(5 * 60)), Instant::now())
    }

    #[test]
    fn starts_idle() {
        let (p, _) = pet();
        assert_eq!(p.mood(), Mood::Idle);
    }

    #[test]
    fn music_playing_shows_music() {
        let (mut p, t) = pet();
        p.set_music(true, t);
        assert_eq!(p.mood(), Mood::Music);
    }

    #[test]
    fn happy_beats_everything_and_ends_when_done() {
        let (mut p, t) = pet();
        p.set_music(true, t);
        assert!(p.click(t + secs(3)));
        assert_eq!(p.mood(), Mood::Happy);
        p.frame(t + secs(4));
        assert_eq!(p.mood(), Mood::Music);
    }

    #[test]
    fn sleeps_after_idle_threshold() {
        let (mut p, t) = pet();
        p.tick(t, secs(6 * 60), DAY);
        assert_eq!(p.mood(), Mood::Sleep);
    }

    #[test]
    fn music_prevents_sleep() {
        let (mut p, t) = pet();
        p.set_music(true, t);
        p.tick(t + secs(2), secs(30 * 60), DAY);
        assert_eq!(p.mood(), Mood::Music);
    }

    #[test]
    fn input_wakes_pet() {
        let (mut p, t) = pet();
        p.tick(t, secs(6 * 60), DAY);
        p.tick(t + secs(2), Duration::ZERO, DAY);
        assert_eq!(p.mood(), Mood::Idle);
    }

    #[test]
    fn short_flicker_is_held_back() {
        let (mut p, t) = pet();
        p.set_music(true, t);
        p.set_music(false, t + ms(500));
        assert_eq!(p.mood(), Mood::Music);
        p.tick(t + secs(2), Duration::ZERO, DAY);
        assert_eq!(p.mood(), Mood::Idle);
    }

    #[test]
    fn click_has_cooldown() {
        let (mut p, t) = pet();
        assert!(p.click(t));
        assert!(!p.click(t + secs(1)));
        assert!(p.click(t + secs(3)));
    }

    #[test]
    fn repeated_happy_restarts_animation() {
        let (mut p, t) = pet();
        p.click(t);
        let g = p.generation();
        p.click(t + ms(2100));
        assert_eq!(p.mood(), Mood::Happy);
        assert_ne!(p.generation(), g);
    }

    #[test]
    fn yawns_at_night_then_not_again_soon() {
        let (mut p, t) = pet();
        p.tick(t, Duration::ZERO, NIGHT);
        assert_eq!(p.mood(), Mood::Yawn);
        p.tick(t + secs(2), Duration::ZERO, NIGHT);
        assert_eq!(p.mood(), Mood::Idle);
        p.tick(t + secs(60), Duration::ZERO, NIGHT);
        assert_eq!(p.mood(), Mood::Idle);
    }

    #[test]
    fn no_yawn_during_the_day() {
        let (mut p, t) = pet();
        p.tick(t, Duration::ZERO, DAY);
        assert_eq!(p.mood(), Mood::Idle);
    }

    #[test]
    fn frames_loop_and_one_shots_hand_over() {
        let (mut p, t) = pet();
        assert_eq!(p.frame(t), (Mood::Idle, 0));
        assert_eq!(p.frame(t + ms(1400)), (Mood::Idle, 0)); // 8 frames at 6 fps wraps
        p.click(t + secs(3));
        assert_eq!(p.frame(t + secs(3)), (Mood::Happy, 0));
        assert_eq!(p.frame(t + secs(3) + ms(250)), (Mood::Happy, 2));
        // 6 frames at 10 fps: done after 0.6 s, back to idle
        assert_eq!(p.frame(t + secs(4)).0, Mood::Idle);
    }

    #[test]
    fn actions_play_out_in_order() {
        let (mut p, t) = pet();
        p.act(Action::Bath, t);
        assert_eq!(p.frame(t + secs(1)).0, Mood::Bath);
        assert_eq!(p.frame(t + ms(3100)).0, Mood::Shake);
        assert_eq!(p.frame(t + secs(5)).0, Mood::Idle);
        p.act(Action::Play, t + secs(10));
        assert_eq!(p.frame(t + secs(12)).0, Mood::Play);
        assert_eq!(p.frame(t + ms(13_100)).0, Mood::Pounce);
        assert_eq!(p.frame(t + ms(13_700)).0, Mood::Happy);
        assert_eq!(p.frame(t + secs(15)).0, Mood::Idle);
    }

    #[test]
    fn needs_nap_and_walking() {
        let (mut p, t) = pet();
        p.set_walking(true, t);
        assert_eq!(p.mood(), Mood::Walk);
        p.set_care(false, Some(Need::Food), t + secs(2));
        assert_eq!(p.mood(), Mood::Hungry);
        p.set_care(true, Some(Need::Food), t + secs(4));
        assert_eq!(p.mood(), Mood::Sleep);
        // an action wakes it for its length, then it goes back to sleep
        p.act(Action::Feed, t + secs(6));
        assert_eq!(p.mood(), Mood::Eat);
        p.frame(t + secs(10));
        assert_eq!(p.mood(), Mood::Sleep);
    }

    #[test]
    fn fidgets_now_and_then_when_calm() {
        let (mut p, t) = pet();
        p.tick(t, Duration::ZERO, DAY);
        assert_eq!(p.mood(), Mood::Idle);
        p.tick(t + FIDGET_AFTER + secs(FIDGET_SPREAD as u64), Duration::ZERO, DAY);
        assert!(matches!(p.mood(), Mood::Groom | Mood::TailWag | Mood::Stretch), "{:?}", p.mood());
    }
}
