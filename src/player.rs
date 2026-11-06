//! Status tab Musik: lagu yang diputar, cover, dan lirik, beserta cara
//! menampilkannya di UI.

use std::rc::Rc;
use std::time::Duration;

use slint::{Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

use crate::background::Request;
use crate::lyrics::{self, Fetch, Lyrics};
use crate::media::{format_time, Media, Track};
use crate::tint;
use crate::{AppWindow, LyricLine, LyricsMode, NowPlaying, Palette};

pub struct Player {
    pub media: Option<Media>,
    track: Option<Track>,
    lyrics: Option<Lyrics>,
    /// Baris yang sedang disorot, supaya UI hanya diubah saat baris berganti.
    shown_line: Option<Option<usize>>,
    lines: Rc<VecModel<LyricLine>>,
}

impl Player {
    pub fn new(ui: &AppWindow) -> Self {
        let lines = Rc::new(VecModel::default());
        ui.set_lyric_lines(ModelRc::from(lines.clone()));
        Self { media: None, track: None, lyrics: None, shown_line: None, lines }
    }

    pub fn playing(&self) -> bool {
        self.track.as_ref().is_some_and(|t| t.playing)
    }

    /// Mengembalikan permintaan lirik bila lagunya baru.
    pub fn set_track(&mut self, ui: &AppWindow, track: Option<Track>) -> Option<Request> {
        let new_song = match (&self.track, &track) {
            (Some(old), Some(new)) => !old.same_song(new),
            (None, Some(_)) => true,
            _ => false,
        };
        self.track = track;

        let Some(t) = &self.track else {
            self.lyrics = None;
            ui.set_now(NowPlaying::default());
            ui.set_has_cover(false);
            return None;
        };

        let request = new_song.then(|| {
            self.lyrics = None;
            self.shown_line = None;
            self.lines.set_vec(Vec::new());
            ui.set_lyric_current(-1);
            ui.set_has_cover(false);
            ui.set_has_tint(false);
            show_status(ui, "Mencari lirik…");
            Request::Lyrics {
                key: key(t),
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                duration: t.duration,
            }
        });
        self.refresh(ui);
        request
    }

    pub fn set_cover(&self, ui: &AppWindow, title: &str, cover: Option<SharedPixelBuffer<Rgba8Pixel>>) {
        if self.track.as_ref().is_none_or(|t| t.title != title) {
            return;
        }
        let Some(buffer) = cover else {
            ui.set_has_cover(false);
            ui.set_has_tint(false);
            return;
        };
        // Warna garis waktu dan kotak lirik mengikuti cover, dijaga beda dari latar island.
        let ink = ui.global::<Palette>().get_ink();
        let pixels: Vec<[u8; 4]> = buffer.as_slice().iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
        match tint::from_cover(&pixels, (ink.red(), ink.green(), ink.blue())) {
            Some(t) => {
                let rgb = |(r, g, b): tint::Rgb| Color::from_rgb_u8(r, g, b);
                ui.set_tint(rgb(t.accent));
                ui.set_tint_card(rgb(t.card));
                ui.set_has_tint(true);
            }
            None => ui.set_has_tint(false),
        }
        ui.set_cover(Image::from_rgba8(buffer));
        ui.set_has_cover(true);
    }

    pub fn set_lyrics(&mut self, ui: &AppWindow, for_key: &str, result: Fetch) {
        if self.track.as_ref().is_none_or(|t| key(t) != for_key) {
            return; // lagu sudah berganti
        }
        self.shown_line = None;
        match result {
            Fetch::Found(Lyrics::Synced(lines)) => {
                let rows: Vec<LyricLine> = lines
                    .iter()
                    .map(|(at, text)| LyricLine { text: text.as_str().into(), seconds: at.as_secs_f32() })
                    .collect();
                self.lines.set_vec(rows);
                ui.set_lyric_current(-1);
                ui.set_lyrics_mode(LyricsMode::Synced);
                self.lyrics = Some(Lyrics::Synced(lines));
            }
            Fetch::Found(Lyrics::Plain(text)) => {
                ui.set_lyrics_plain(text.as_str().into());
                ui.set_lyrics_mode(LyricsMode::Plain);
                self.lyrics = Some(Lyrics::Plain(text));
            }
            Fetch::Found(Lyrics::Instrumental) => show_status(ui, "Lagu instrumental, tanpa lirik"),
            Fetch::NotFound => show_status(ui, "Lirik tidak ditemukan"),
            Fetch::Failed => show_status(ui, "Lirik gagal dimuat. Periksa koneksi internet"),
        }
        self.refresh(ui);
    }

    /// Perbarui posisi, garis waktu, dan baris lirik.
    pub fn refresh(&mut self, ui: &AppWindow) {
        let Some(t) = &self.track else { return };
        let position = t.position();
        let progress = if t.duration.is_zero() { 0.0 } else { position.as_secs_f32() / t.duration.as_secs_f32() };
        ui.set_now(NowPlaying {
            available: true,
            playing: t.playing,
            can_seek: t.can_seek,
            title: or(&t.title, "Tanpa judul"),
            artist: or(&t.artist, "Spotify"),
            progress,
            position: format_time(position).into(),
            duration: format_time(t.duration).into(),
        });

        if let Some(Lyrics::Synced(lines)) = &self.lyrics {
            let current = lyrics::index_at(lines, position);
            if self.shown_line != Some(current) {
                self.shown_line = Some(current);
                ui.set_lyric_current(current.map_or(-1, |i| i as i32));
            }
        }
    }

    pub fn seek(&mut self, fraction: f32) {
        let Some(t) = &self.track else { return };
        self.seek_to(t.duration.as_secs_f32() * fraction.clamp(0.0, 1.0));
    }

    /// Lompat ke detik tertentu, mis. saat baris lirik diklik.
    pub fn seek_to(&mut self, seconds: f32) {
        let (Some(media), Some(t)) = (&self.media, &self.track) else { return };
        if t.can_seek && !t.duration.is_zero() {
            media.seek(Duration::from_secs_f32(seconds.clamp(0.0, t.duration.as_secs_f32())));
        }
    }
}

fn show_status(ui: &AppWindow, text: &str) {
    ui.set_lyrics_status(text.into());
    ui.set_lyrics_mode(LyricsMode::Status);
}

fn key(t: &Track) -> String {
    format!("{}\u{1f}{}", t.title, t.artist)
}

fn or(value: &str, fallback: &str) -> SharedString {
    if value.trim().is_empty() { fallback.into() } else { value.into() }
}
