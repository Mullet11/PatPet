//! Sesi Spotify desktop lewat Windows Media Session (SMTC). Berbasis event,
//! tanpa polling. Event datang dari thread milik Windows; hasilnya dikirim
//! lewat `post`, jadi modul ini tidak menyentuh UI.

use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use slint::{Rgba8Pixel, SharedPixelBuffer};
use windows::core::Interface;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

use crate::image::decode_square;

/// Sisi cover dalam piksel setelah dikecilkan. Cukup tajam untuk 64 px di skala 250%.
const COVER_PX: u32 = 160;

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub can_seek: bool,
    pub duration: Duration,
    /// Posisi saat `at`.
    position: Duration,
    at: Instant,
}

impl Track {
    /// Posisi sekarang, diperkirakan dari laporan terakhir bila sedang diputar.
    pub fn position(&self) -> Duration {
        let pos = if self.playing { self.position + self.at.elapsed() } else { self.position };
        if self.duration.is_zero() { pos } else { pos.min(self.duration) }
    }

    pub fn same_song(&self, other: &Track) -> bool {
        self.title == other.title && self.artist == other.artist
    }
}

pub enum Update {
    /// `None` = Spotify tidak aktif.
    Track(Option<Track>),
    /// Cover untuk lagu dengan judul ini.
    Cover(String, Option<SharedPixelBuffer<Rgba8Pixel>>),
}

type Post = Arc<dyn Fn(Update) + Send + Sync>;

struct Attached {
    session: Session,
    tokens: [i64; 3],
}

struct State {
    manager: Manager,
    attached: Option<Attached>,
    cover_for: String,
    post: Post,
}

/// Pegangan ke sesi Spotify. Melepas semua event handler saat di-drop.
pub struct Media {
    state: Arc<Mutex<State>>,
    sessions_token: i64,
}

impl Media {
    /// Memblokir sebentar menunggu Windows; panggil dari thread latar.
    pub fn start(post: impl Fn(Update) + Send + Sync + 'static) -> Option<Self> {
        let manager = Manager::RequestAsync().ok()?.get().ok()?;
        let state = Arc::new(Mutex::new(State {
            manager: manager.clone(),
            attached: None,
            cover_for: String::new(),
            post: Arc::new(post),
        }));

        // Handler memegang Weak supaya tidak membentuk siklus dengan manager.
        let weak = Arc::downgrade(&state);
        let sessions_token = manager
            .SessionsChanged(&TypedEventHandler::new(move |_, _| {
                reattach(&weak);
                Ok(())
            }))
            .ok()?;

        reattach(&Arc::downgrade(&state));
        Some(Self { state, sessions_token })
    }

    fn session(&self) -> Option<Session> {
        self.state.lock().ok()?.attached.as_ref().map(|a| a.session.clone())
    }

    // Perintah tidak menunggu hasil; perubahan datang kembali lewat event.
    pub fn play_pause(&self) {
        if let Some(s) = self.session() {
            let _ = s.TryTogglePlayPauseAsync();
        }
    }

    pub fn next(&self) {
        if let Some(s) = self.session() {
            let _ = s.TrySkipNextAsync();
        }
    }

    pub fn previous(&self) {
        if let Some(s) = self.session() {
            let _ = s.TrySkipPreviousAsync();
        }
    }

    pub fn seek(&self, to: Duration) {
        if let Some(s) = self.session() {
            let _ = s.TryChangePlaybackPositionAsync((to.as_nanos() / 100) as i64);
        }
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            let _ = state.manager.RemoveSessionsChanged(self.sessions_token);
            if let Some(a) = state.attached.take() {
                detach(a);
            }
        }
    }
}

fn reattach(weak: &Weak<Mutex<State>>) {
    let Some(state) = weak.upgrade() else { return };
    let Ok(mut s) = state.lock() else { return };

    let spotify = s.manager.GetSessions().ok().and_then(|list| {
        list.into_iter().find(|x| {
            x.SourceAppUserModelId().is_ok_and(|id| id.to_string().to_lowercase().contains("spotify"))
        })
    });

    let same = matches!((&spotify, &s.attached), (Some(n), Some(a)) if n.as_raw() == a.session.as_raw());
    if same {
        return;
    }
    if let Some(old) = s.attached.take() {
        detach(old);
    }

    let Some(session) = spotify else {
        s.cover_for.clear();
        (s.post)(Update::Track(None));
        return;
    };

    let (w1, w2, w3) = (weak.clone(), weak.clone(), weak.clone());
    let tokens = [
        session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            publish(&w1);
            Ok(())
        })),
        session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            publish(&w2);
            Ok(())
        })),
        session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            publish(&w3);
            Ok(())
        })),
    ];
    let tokens = tokens.map(|t| t.unwrap_or_default());
    s.attached = Some(Attached { session, tokens });
    drop(s);
    publish(weak);
}

fn detach(a: Attached) {
    let _ = a.session.RemoveMediaPropertiesChanged(a.tokens[0]);
    let _ = a.session.RemovePlaybackInfoChanged(a.tokens[1]);
    let _ = a.session.RemoveTimelinePropertiesChanged(a.tokens[2]);
}

fn publish(weak: &Weak<Mutex<State>>) {
    let Some(state) = weak.upgrade() else { return };
    let (session, post) = {
        let Ok(s) = state.lock() else { return };
        let Some(a) = &s.attached else { return };
        (a.session.clone(), s.post.clone())
    };
    // Sesi bisa tertutup di tengah jalan; SessionsChanged akan menyusul.
    let Some((track, props)) = read_track(&session) else { return };

    let new_cover = {
        let Ok(mut s) = state.lock() else { return };
        let fresh = s.cover_for != track.title;
        if fresh {
            s.cover_for = track.title.clone();
        }
        fresh
    };
    let title = track.title.clone();
    post(Update::Track(Some(track)));

    if new_cover {
        let cover = props
            .Thumbnail()
            .ok()
            .and_then(|r| decode_square(&r, COVER_PX));
        post(Update::Cover(title, cover));
    }
}

fn read_track(
    session: &Session,
) -> Option<(Track, windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties)> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let info = session.GetPlaybackInfo().ok()?;
    let time = session.GetTimelineProperties().ok()?;

    let ticks = |t: windows::Foundation::TimeSpan| Duration::from_nanos(t.Duration.max(0) as u64 * 100);
    // Umur laporan posisi: sekarang dikurangi LastUpdatedTime (keduanya FILETIME, 100 ns).
    let updated = time.LastUpdatedTime().map(|d| d.UniversalTime).unwrap_or(0);
    let age = if updated > 0 { Duration::from_nanos((now_filetime() - updated).max(0) as u64 * 100) } else { Duration::ZERO };
    let at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);

    let track = Track {
        title: props.Title().map(|s| s.to_string()).unwrap_or_default(),
        artist: props.Artist().map(|s| s.to_string()).unwrap_or_default(),
        album: props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default(),
        playing: info.PlaybackStatus().is_ok_and(|s| s == Status::Playing),
        can_seek: info
            .Controls()
            .and_then(|c| c.IsPlaybackPositionEnabled())
            .unwrap_or(false),
        duration: time.EndTime().map(ticks).unwrap_or_default(),
        position: time.Position().map(ticks).unwrap_or_default(),
        at,
    };
    Some((track, props))
}

fn now_filetime() -> i64 {
    // SAFETY: tanpa prasyarat.
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

/// "3:07", atau "1:02:05" bila lebih dari satu jam.
pub fn format_time(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_format() {
        assert_eq!(format_time(Duration::from_secs(7)), "0:07");
        assert_eq!(format_time(Duration::from_secs(187)), "3:07");
        assert_eq!(format_time(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn position_runs_only_while_playing() {
        let base = Track {
            title: "a".into(),
            artist: "b".into(),
            album: String::new(),
            playing: false,
            can_seek: true,
            duration: Duration::from_secs(100),
            position: Duration::from_secs(10),
            at: Instant::now() - Duration::from_secs(5),
        };
        assert_eq!(base.position(), Duration::from_secs(10));
        let playing = Track { playing: true, ..base.clone() };
        assert!(playing.position() >= Duration::from_secs(15));
        let near_end = Track { playing: true, position: Duration::from_secs(99), ..base };
        assert_eq!(near_end.position(), Duration::from_secs(100));
    }
}
