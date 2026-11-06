//! Antrean notifikasi island. Pesan tampil satu per satu; indikator sistem
//! (volume, kecerahan) tampil seketika dan pesan yang tertunda dilanjutkan.

use std::collections::VecDeque;
use std::time::Duration;

use slint::{Rgba8Pixel, SharedPixelBuffer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Alert,
}

/// Yang terjadi saat notifikasi diklik.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Buka panel pada tab ini.
    Tab(i32),
    /// Buka aplikasi atau halaman Windows (`shell:` atau `ms-settings:`).
    Open(String),
}

#[derive(Clone)]
pub struct Notice {
    pub glyph: &'static str,
    pub title: String,
    pub body: String,
    pub tone: Tone,
    /// 0–1 untuk garis level (volume, kecerahan, baterai).
    pub level: Option<f32>,
    pub action: Option<Action>,
    /// Logo aplikasi pengirim; menggantikan glyph bila ada.
    pub image: Option<SharedPixelBuffer<Rgba8Pixel>>,
    /// `None` = tetap tampil sampai ditutup (mis. alarm).
    pub duration: Option<Duration>,
    /// Indikator sistem: tampil seketika dan menggantikan indikator sebelumnya.
    pub indicator: bool,
}

/// Glyph Segoe MDL2 Assets untuk notifikasi (sama dengan `Icon` di ui/parts.slint).
pub mod glyph {
    pub const ALARM: &str = "\u{EA8F}";
    pub const BATTERY_LOW: &str = "\u{E850}";
    pub const BELL: &str = "\u{EA8F}";
    pub const BOLT: &str = "\u{E945}";
    pub const BRIGHTNESS: &str = "\u{E706}";
    pub const CAMERA: &str = "\u{E722}";
    pub const CALENDAR: &str = "\u{E787}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const LEAF: &str = "\u{E8BE}";
    pub const MUTE: &str = "\u{E74F}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const VOLUME: &str = "\u{E767}";
}

pub const MESSAGE_FOR: Duration = Duration::from_secs(4);
pub const ALERT_FOR: Duration = Duration::from_secs(6);
pub const INDICATOR_FOR: Duration = Duration::from_millis(1500);

impl Notice {
    pub fn message(glyph: &'static str, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            glyph,
            title: title.into(),
            body: body.into(),
            tone: Tone::Accent,
            level: None,
            action: None,
            image: None,
            duration: Some(MESSAGE_FOR),
            indicator: false,
        }
    }

    pub fn alert(glyph: &'static str, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self { tone: Tone::Alert, duration: Some(ALERT_FOR), ..Self::message(glyph, title, body) }
    }

    pub fn indicator(glyph: &'static str, title: impl Into<String>, level: Option<f32>) -> Self {
        Self {
            level: level.map(|l| l.clamp(0.0, 1.0)),
            duration: Some(INDICATOR_FOR),
            indicator: true,
            ..Self::message(glyph, title, "")
        }
    }

    pub fn with_tab(self, tab: i32) -> Self {
        Self { action: Some(Action::Tab(tab)), ..self }
    }

    pub fn with_open(self, target: impl Into<String>) -> Self {
        Self { action: Some(Action::Open(target.into())), ..self }
    }

    /// Notifikasi dari aplikasi lain, tampil sedikit lebih lama supaya sempat dibaca.
    pub fn external(title: String, body: String, logo: Option<SharedPixelBuffer<Rgba8Pixel>>) -> Self {
        Self { image: logo, duration: Some(ALERT_FOR), ..Self::message(glyph::BELL, title, body) }
    }

    pub fn sticky(self) -> Self {
        Self { duration: None, ..self }
    }
}

#[derive(Default)]
pub struct Queue {
    current: Option<Notice>,
    waiting: VecDeque<Notice>,
}

impl Queue {
    pub fn current(&self) -> Option<&Notice> {
        self.current.as_ref()
    }

    /// Tambahkan notifikasi. Mengembalikan `true` bila yang tampil berubah.
    pub fn push(&mut self, notice: Notice) -> bool {
        if notice.indicator {
            // Pesan yang sedang tampil disimpan di depan antrean, lalu dilanjutkan.
            if let Some(shown) = self.current.take().filter(|n| !n.indicator) {
                self.waiting.push_front(shown);
            }
            self.current = Some(notice);
            return true;
        }
        if self.current.is_none() {
            self.current = Some(notice);
            return true;
        }
        self.waiting.push_back(notice);
        false
    }

    /// Tutup yang sedang tampil dan lanjut ke berikutnya.
    pub fn next(&mut self) {
        self.current = self.waiting.pop_front();
    }

    /// Hapus notifikasi tetap (mis. alarm) yang judulnya sama, di mana pun posisinya.
    pub fn remove_sticky(&mut self, title: &str) -> bool {
        self.waiting.retain(|n| !(n.duration.is_none() && n.title == title));
        if self.current.as_ref().is_some_and(|n| n.duration.is_none() && n.title == title) {
            self.next();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(title: &str) -> Notice {
        Notice::message("", title, "isi")
    }

    #[test]
    fn messages_show_one_at_a_time() {
        let mut q = Queue::default();
        assert!(q.push(msg("a")));
        assert!(!q.push(msg("b")));
        assert_eq!(q.current().unwrap().title, "a");
        q.next();
        assert_eq!(q.current().unwrap().title, "b");
        q.next();
        assert!(q.current().is_none());
    }

    #[test]
    fn indicator_interrupts_and_message_resumes() {
        let mut q = Queue::default();
        q.push(msg("a"));
        q.push(msg("b"));
        assert!(q.push(Notice::indicator("", "Volume", Some(0.4))));
        assert_eq!(q.current().unwrap().title, "Volume");
        assert!(q.push(Notice::indicator("", "Volume", Some(0.5))));
        assert_eq!(q.current().unwrap().level, Some(0.5));
        q.next();
        assert_eq!(q.current().unwrap().title, "a");
        q.next();
        assert_eq!(q.current().unwrap().title, "b");
    }

    #[test]
    fn sticky_alarm_is_removed_by_title() {
        let mut q = Queue::default();
        q.push(Notice::alert("", "Waktu habis", "Timer selesai").sticky());
        q.push(msg("b"));
        assert!(q.remove_sticky("Waktu habis"));
        assert_eq!(q.current().unwrap().title, "b");
    }

    #[test]
    fn levels_are_clamped() {
        assert_eq!(Notice::indicator("", "x", Some(1.7)).level, Some(1.0));
    }
}
