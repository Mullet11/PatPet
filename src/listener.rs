//! Notifikasi dari aplikasi lain (WhatsApp, Chrome, dll.) lewat
//! UserNotificationListener. Hanya bekerja bila PatPet berjalan dengan
//! identitas aplikasi (lihat packaging/install.ps1) dan akses notifikasi
//! diizinkan. Tanpa itu, fitur ini diam-diam tidak aktif.
//!
//! Event `NotificationChanged` tidak sampai ke aplikasi desktop, jadi daftar
//! notifikasi dibaca berkala di thread sendiri dan hanya yang baru diteruskan.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use slint::{Rgba8Pixel, SharedPixelBuffer};
use windows::core::Interface;
use windows::ApplicationModel::Package;
use windows::Foundation::Size;
use windows::Storage::Streams::IRandomAccessStreamReference;
use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds, UserNotification};

use crate::image::decode_square;

/// Sisi logo aplikasi di notifikasi, piksel. Cukup untuk 24 px di skala 200%.
const LOGO_PX: u32 = 48;
/// Jeda antar-pembacaan daftar notifikasi.
const POLL_EVERY: Duration = Duration::from_secs(2);

/// Notifikasi aplikasi lain yang siap ditampilkan.
pub struct Incoming {
    pub title: String,
    pub body: String,
    /// AppUserModelID, untuk membuka aplikasinya saat notifikasi diklik.
    pub app_id: String,
    pub logo: Option<SharedPixelBuffer<Rgba8Pixel>>,
}

pub enum Access {
    /// PatPet berjalan tanpa paket identitas.
    NoIdentity,
    /// Izin sedang diminta; jawaban datang lewat `on_answer`.
    Asking,
    Denied,
    Listening(Listener),
}

/// Thread pembaca notifikasi. Berhenti saat di-drop.
pub struct Listener {
    stop: Arc<AtomicBool>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Izin membaca notifikasi sudah diberikan (mis. lewat Pengaturan Windows).
pub fn allowed() -> bool {
    Package::Current().is_ok()
        && UserNotificationListener::Current()
            .and_then(|l| l.GetAccessStatus())
            .is_ok_and(|s| s == UserNotificationListenerAccessStatus::Allowed)
}

/// Mulai mendengarkan, atau minta izin bila belum pernah ditanya.
/// `on_answer` dipanggil (dari thread lain) setelah permintaan izin dijawab.
pub fn start(on_incoming: impl Fn(Incoming) + Send + 'static, on_answer: impl Fn() + Send + 'static) -> Access {
    if Package::Current().is_err() {
        return Access::NoIdentity;
    }
    let Ok(listener) = UserNotificationListener::Current() else { return Access::NoIdentity };
    match listener.GetAccessStatus() {
        Ok(UserNotificationListenerAccessStatus::Allowed) => {}
        Ok(UserNotificationListenerAccessStatus::Unspecified) => {
            // Permintaan dimulai dari thread UI; jawabannya ditunggu di thread lain.
            let Ok(asking) = listener.RequestAccessAsync() else { return Access::Denied };
            thread::spawn(move || {
                let _ = asking.get();
                on_answer();
            });
            return Access::Asking;
        }
        _ => return Access::Denied,
    }

    let stop = Arc::new(AtomicBool::new(false));
    let running = stop.clone();
    // Objek dari thread UI (STA) tidak boleh dipakai di thread lain; ambil yang baru di sana.
    drop(listener);
    let spawned = thread::Builder::new().name("notifikasi".into()).spawn(move || {
        let Ok(listener) = UserNotificationListener::Current() else { return };
        // Notifikasi yang sudah ada sebelum PatPet mulai tidak ditampilkan.
        let mut seen = current_ids(&listener).unwrap_or_default();
        while !running.load(Ordering::Relaxed) {
            thread::sleep(POLL_EVERY);
            let Some(list) = toasts(&listener) else { continue };
            let mut now = HashSet::with_capacity(list.len());
            for n in &list {
                let Ok(id) = n.Id() else { continue };
                now.insert(id);
                if !seen.contains(&id) {
                    if let Some(incoming) = read(n) {
                        on_incoming(incoming);
                    }
                }
            }
            // Hanya simpan id yang masih ada, supaya memori tidak terus bertambah.
            seen = now;
        }
    });
    match spawned {
        Ok(_) => Access::Listening(Listener { stop }),
        Err(_) => Access::Denied,
    }
}

fn toasts(listener: &UserNotificationListener) -> Option<Vec<UserNotification>> {
    Some(listener.GetNotificationsAsync(NotificationKinds::Toast).ok()?.get().ok()?.into_iter().collect())
}

fn current_ids(listener: &UserNotificationListener) -> Option<HashSet<u32>> {
    Some(toasts(listener)?.iter().filter_map(|n| n.Id().ok()).collect())
}

/// Teks pertama jadi judul (biasanya pengirim), sisanya isi.
fn read(n: &UserNotification) -> Option<Incoming> {
    let info = n.AppInfo().ok()?;
    let display = info.DisplayInfo().ok()?;
    let app = display.DisplayName().map(|s| s.to_string()).unwrap_or_default();
    let binding = n.Notification().ok()?.Visual().ok()?.GetBinding(&KnownNotificationBindings::ToastGeneric().ok()?).ok()?;
    let texts: Vec<String> = binding
        .GetTextElements()
        .ok()?
        .into_iter()
        .filter_map(|t| t.Text().ok().map(|s| s.to_string()))
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect();

    let (title, body) = match texts.as_slice() {
        [] => (app.clone(), String::new()),
        [only] => (app.clone(), only.clone()),
        [first, rest @ ..] => (first.clone(), rest.join(" · ")),
    };
    let logo = display
        .GetLogo(Size { Width: LOGO_PX as f32, Height: LOGO_PX as f32 })
        .and_then(|r| r.cast::<IRandomAccessStreamReference>())
        .ok()
        .and_then(|r| decode_square(&r, LOGO_PX));
    Some(Incoming {
        title: title.chars().take(80).collect(),
        body: body.chars().take(160).collect(),
        app_id: info.AppUserModelId().map(|s| s.to_string()).unwrap_or_default(),
        logo,
    })
}

/// Buka aplikasi (`shell:AppsFolder\<AUMID>`, bekerja untuk aplikasi Store
/// maupun biasa) atau halaman Windows (`ms-settings:...`).
pub fn open(target: &str) {
    let _ = std::process::Command::new("explorer.exe").arg(target).spawn();
}
