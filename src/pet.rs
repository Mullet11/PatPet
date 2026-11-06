//! Otak kucing: menerima sinyal, memutuskan animasi mana yang tampil.
//! Prioritas: happy > focus > music > yawn > sleep > idle.
//! Waktu selalu dikirim dari luar, jadi logika ini murni dan mudah dites.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Music,
    Sleep,
    Focus,
    Happy,
    Yawn,
}

const MIN_HOLD: Duration = Duration::from_millis(1500);
const CLICK_COOLDOWN: Duration = Duration::from_secs(2);
const YAWN_EVERY: Duration = Duration::from_secs(10 * 60);
/// Batas aman bila UI tidak melapor animasi sekali-jalan sudah selesai.
const ONE_SHOT_TIMEOUT: Duration = Duration::from_secs(4);

pub struct Pet {
    mood: Mood,
    /// Naik setiap kali animasi harus diputar dari awal.
    generation: u32,
    playing: bool,
    focus: bool,
    idle: Duration,
    sleep_after: Duration,
    one_shot: Option<(Mood, Instant)>,
    last_change: Option<Instant>,
    last_click: Option<Instant>,
    last_yawn: Option<Instant>,
}

impl Pet {
    pub fn new(sleep_after: Duration) -> Self {
        Self {
            mood: Mood::Idle,
            generation: 0,
            playing: false,
            focus: false,
            idle: Duration::ZERO,
            sleep_after,
            one_shot: None,
            last_change: None,
            last_click: None,
            last_yawn: None,
        }
    }

    pub fn mood(&self) -> Mood {
        self.mood
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn set_sleep_after(&mut self, after: Duration) {
        self.sleep_after = after;
    }

    pub fn is_night(hour: u32) -> bool {
        hour >= 23 || hour < 4
    }

    pub fn set_music(&mut self, playing: bool, now: Instant) -> bool {
        self.playing = playing;
        self.update(now, false)
    }

    /// `completed`: timer habis sampai nol, kucing ikut senang.
    pub fn set_focus(&mut self, active: bool, completed: bool, now: Instant) -> bool {
        self.focus = active;
        if completed {
            self.trigger_happy(now);
        }
        self.update(now, false)
    }

    /// Mengembalikan `false` bila masih dalam jeda antar-klik.
    pub fn click(&mut self, now: Instant) -> bool {
        if self.last_click.is_some_and(|t| now - t < CLICK_COOLDOWN) {
            return false;
        }
        self.last_click = Some(now);
        self.trigger_happy(now);
        self.update(now, false);
        true
    }

    /// Kucing senang tanpa jeda antar-klik, mis. saat catatan baru ditambahkan.
    pub fn cheer(&mut self, now: Instant) -> bool {
        self.trigger_happy(now);
        self.update(now, false)
    }

    /// Dipanggil berkala. `idle` = lama pengguna tidak menyentuh mouse/keyboard.
    pub fn tick(&mut self, now: Instant, idle: Duration, hour: u32) -> bool {
        self.idle = idle;

        if self.one_shot.is_some_and(|(_, t)| now - t >= ONE_SHOT_TIMEOUT) {
            self.one_shot = None;
        }

        let yawn_due = self.last_yawn.map_or(true, |t| now - t >= YAWN_EVERY);
        if self.one_shot.is_none()
            && Self::is_night(hour)
            && !self.focus
            && !self.playing
            && idle < self.sleep_after
            && yawn_due
        {
            self.one_shot = Some((Mood::Yawn, now));
            self.last_yawn = Some(now);
        }

        self.update(now, false)
    }

    pub fn one_shot_done(&mut self, now: Instant) -> bool {
        if self.one_shot.take().is_none() {
            return false;
        }
        self.update(now, true)
    }

    fn evaluate(&self) -> Mood {
        let shot = self.one_shot.map(|(m, _)| m);
        if shot == Some(Mood::Happy) {
            Mood::Happy
        } else if self.focus {
            Mood::Focus
        } else if self.playing {
            Mood::Music
        } else if shot == Some(Mood::Yawn) {
            Mood::Yawn
        } else if self.idle >= self.sleep_after {
            Mood::Sleep
        } else {
            Mood::Idle
        }
    }

    fn trigger_happy(&mut self, now: Instant) {
        self.one_shot = Some((Mood::Happy, now));
        if self.mood == Mood::Happy {
            self.generation = self.generation.wrapping_add(1); // putar ulang dari awal
        }
    }

    /// Mengembalikan `true` bila UI perlu diperbarui.
    fn update(&mut self, now: Instant, force: bool) -> bool {
        let before = self.generation;
        let next = self.evaluate();
        if matches!(self.one_shot, Some((Mood::Yawn, _))) && next != Mood::Yawn {
            self.one_shot = None; // menguap kalah prioritas
        }
        if next == self.mood {
            return self.generation != before;
        }

        // Tahan sebentar supaya animasi tidak berkedip bolak-balik.
        let may_switch = force
            || next == Mood::Happy
            || self.mood == Mood::Happy
            || self.last_change.map_or(true, |t| now - t >= MIN_HOLD);
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
    fn focus_beats_music() {
        let (mut p, t) = pet();
        p.set_music(true, t);
        p.set_focus(true, false, t + secs(2));
        assert_eq!(p.mood(), Mood::Focus);
    }

    #[test]
    fn happy_beats_everything_and_ends_when_done() {
        let (mut p, t) = pet();
        p.set_focus(true, false, t);
        assert!(p.click(t + secs(3)));
        assert_eq!(p.mood(), Mood::Happy);
        p.one_shot_done(t + secs(4));
        assert_eq!(p.mood(), Mood::Focus);
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
        p.set_music(false, t + Duration::from_millis(500));
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
        p.click(t + secs(3));
        assert_eq!(p.mood(), Mood::Happy);
        assert_ne!(p.generation(), g);
    }

    #[test]
    fn yawns_at_night_then_not_again_soon() {
        let (mut p, t) = pet();
        p.tick(t, Duration::ZERO, NIGHT);
        assert_eq!(p.mood(), Mood::Yawn);
        p.one_shot_done(t + secs(1));
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
    fn stuck_one_shot_times_out() {
        let (mut p, t) = pet();
        p.click(t);
        p.tick(t + secs(5), Duration::ZERO, DAY);
        assert_eq!(p.mood(), Mood::Idle);
    }
}
