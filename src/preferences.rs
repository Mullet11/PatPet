//! `patpet.conf`: memuat, menerapkan, menyimpan nilai yang berubah saat
//! dipakai, memuat ulang saat file disimpan, serta mode ikon dan seret kucing.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::background::Request;
use crate::conf::Conf;
use crate::notices::{glyph, Notice};
use crate::placement::{icon_layout, Rect, Widths};
use crate::platform::{self, Placement};
use crate::settings::{Mode, Settings, DEFAULT_CONF};
use crate::{colors, paths, with_app, App, AppWindow};

/// Simpan setelah perubahan berhenti selama ini, supaya file tidak ditulis berulang.
const SAVE_AFTER: Duration = Duration::from_millis(500);
/// Gerakan lebih dari ini dianggap seret, bukan klik.
const DRAG_THRESHOLD: i32 = 4;
// Ukuran dalam piksel logis; harus sama dengan token di ui/theme.slint.
const PET: f32 = 32.0;
const GAP: f32 = 8.0;
const PANEL_HEIGHT: f32 = 236.0;
const EDGE: f32 = 24.0;

pub struct Drag {
    cursor: (i32, i32),
    pet: (i32, i32),
    moved: bool,
}

/// File pengaturan yang sedang dipakai.
pub struct ConfigFile {
    pub conf: Conf,
    /// Waktu ubah terakhir yang sudah dibaca; perubahan dari luar terdeteksi lewat ini.
    seen: Option<SystemTime>,
}

fn conf_path() -> PathBuf {
    paths::data_dir().join("patpet.conf")
}

fn modified() -> Option<SystemTime> {
    std::fs::metadata(conf_path()).and_then(|m| m.modified()).ok()
}

impl ConfigFile {
    /// Muat `patpet.conf`. Bila belum ada (atau kosong, mis. tersisa dari
    /// editor yang ditutup paksa), buat dari template dan pindahkan nilai
    /// dari `settings.json` lama bila ada.
    pub fn load() -> (Self, Settings, Vec<usize>) {
        let text = std::fs::read_to_string(conf_path()).ok().filter(|t| !t.trim().is_empty());
        let fresh = text.is_none();
        let mut conf = Conf::parse(text.as_deref().unwrap_or(DEFAULT_CONF));
        if fresh {
            let old = std::fs::read_to_string(paths::data_dir().join("settings.json")).ok();
            if let Some(settings) = old.as_deref().and_then(Settings::from_old_json) {
                settings.write_all(&mut conf);
            }
        }
        let (settings, bad) = Settings::from_conf(&conf);
        let mut file = Self { conf, seen: None };
        if fresh {
            file.save();
        } else {
            file.seen = modified();
        }
        (file, settings, bad)
    }

    /// Tulis ke file sementara lalu ganti nama, supaya file tidak pernah setengah jadi.
    pub fn save(&mut self) {
        let path = conf_path();
        let tmp = path.with_extension("conf.tmp");
        if std::fs::write(&tmp, self.conf.text()).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            self.seen = modified();
        }
    }

    /// Isi baru bila file diubah dari luar sejak terakhir dibaca.
    fn changed(&mut self) -> Option<Conf> {
        let now = modified();
        if now == self.seen {
            return None;
        }
        self.seen = now;
        std::fs::read_to_string(conf_path()).ok().map(|t| Conf::parse(&t))
    }
}

/// Buka `patpet.conf` di Notepad.
pub fn open_config() {
    let _ = std::process::Command::new("notepad.exe").arg(conf_path()).spawn();
}

impl App {
    /// Terapkan pengaturan dan warna ke UI dan jendela. `bad` = baris yang tidak terbaca.
    pub fn apply_settings(&mut self, ui: &AppWindow, mut bad: Vec<usize>) {
        bad.extend(colors::apply(ui, &self.config.conf));
        let s = &self.settings;
        ui.set_icon_mode(s.mode == Mode::Icon);
        ui.set_pet_scale(s.pet_scale);
        ui.set_auto_hide(s.auto_hide);
        ui.set_alarm_enabled(s.alarm_enabled);
        if !self.countdown.is_active() {
            ui.set_timer_hours(s.timer_seconds / 3600);
            ui.set_timer_minutes(s.timer_seconds / 60 % 60);
            ui.set_timer_seconds(s.timer_seconds % 60);
        }
        self.pet.set_sleep_after(Duration::from_secs(s.idle_sleep_minutes as u64 * 60));
        platform::set_topmost(s.always_on_top);
        let _ = self.worker.send(Request::SetLocation(s.city.clone()));
        self.update_widths(ui);

        if !bad.is_empty() {
            bad.sort_unstable();
            bad.dedup();
            let lines: Vec<String> = bad.iter().map(usize::to_string).collect();
            let body = format!("patpet.conf baris {} tidak valid", lines.join(", "));
            self.notify(ui, Notice::alert(glyph::SETTINGS, "Pengaturan", body));
        }
    }

    /// Muat ulang bila `patpet.conf` disimpan dari luar. Dipanggil berkala.
    pub fn reload_config_if_changed(&mut self, ui: &AppWindow) {
        if let Some(conf) = self.config.changed() {
            let (settings, bad) = Settings::from_conf(&conf);
            self.config.conf = conf;
            self.settings = settings;
            self.apply_settings(ui, bad);
        }
    }

    /// Simpan sebentar lagi; nilai timer dibaca dari UI saat itu.
    pub fn save_soon(&mut self) {
        self.settings_timer.start(slint::TimerMode::SingleShot, SAVE_AFTER, || {
            with_app(App::save_settings);
        });
    }

    pub fn save_settings(&mut self, ui: &AppWindow) {
        self.settings_timer.stop();
        // Jangan menimpa perubahan pengguna yang belum dibaca.
        self.reload_config_if_changed(ui);
        let s = &mut self.settings;
        s.timer_seconds = ui.get_timer_hours() * 3600 + ui.get_timer_minutes() * 60 + ui.get_timer_seconds();
        s.alarm_enabled = ui.get_alarm_enabled();
        let before = self.config.conf.text();
        s.write_runtime(&mut self.config.conf);
        if self.config.conf.text() != before {
            self.config.save();
        }
    }

    /// Hitung ulang lebar island dari lebar layar, lalu tata letak jendela.
    pub fn update_widths(&mut self, ui: &AppWindow) {
        let [closed, wide, panel] = self.settings.widths.map(|p| p / 100.0);
        self.widths = Widths::for_screen(platform::monitor_width(), closed, wide, panel);
        ui.set_island_closed(self.widths.closed);
        ui.set_island_wide(self.widths.wide);
        ui.set_panel_wide(self.widths.panel);
        self.place_window(ui);
    }

    /// Mode bar: tengah atas. Mode ikon: hitung letak kucing dan panel.
    pub fn place_window(&mut self, ui: &AppWindow) {
        if self.settings.mode == Mode::Bar {
            platform::place(Placement::TopCenter);
            return;
        }
        let scale = platform::scale();
        let px = |v: f32| (v * scale).round() as i32;
        let pet_size = PET as i32 * (self.settings.pet_scale as f32 * scale).round().max(1.0) as i32;

        let wanted = match self.settings.icon_pos {
            Some((x, y)) => (px(x as f32), px(y as f32)),
            None => {
                // Bawaan: pojok kanan atas layar tempat kursor berada.
                let (cx, cy) = platform::cursor_pos();
                let work = platform::work_area_at(cx, cy);
                (work.right - pet_size - px(EDGE), work.top + px(EDGE))
            }
        };
        let work: Rect = platform::work_area_at(wanted.0 + pet_size / 2, wanted.1 + pet_size / 2);
        let layout = icon_layout(wanted, pet_size, px(GAP), px(self.widths.panel), px(PANEL_HEIGHT), work);

        ui.set_pet_x(layout.pet.0 as f32 / scale);
        ui.set_pet_y(layout.pet.1 as f32 / scale);
        ui.set_panel_below(layout.panel_below);
        platform::place(Placement::At(layout.window.0, layout.window.1));

        let pos = layout.pet_on_screen;
        self.settings.icon_pos = Some(((pos.0 as f32 / scale).into(), (pos.1 as f32 / scale).into()));
    }

    pub fn drag_start(&mut self) {
        let scale = platform::scale();
        let (x, y) = self.settings.icon_pos.unwrap_or_default();
        let pet = ((x as f32 * scale).round() as i32, (y as f32 * scale).round() as i32);
        self.drag = Some(Drag { cursor: platform::cursor_pos(), pet, moved: false });
    }

    pub fn drag_move(&mut self, ui: &AppWindow) {
        let Some(drag) = &mut self.drag else { return };
        let (x, y) = platform::cursor_pos();
        let (dx, dy) = (x - drag.cursor.0, y - drag.cursor.1);
        if !drag.moved && dx.abs().max(dy.abs()) <= DRAG_THRESHOLD {
            return;
        }
        drag.moved = true;
        let scale = platform::scale();
        let to = (drag.pet.0 + dx, drag.pet.1 + dy);
        self.settings.icon_pos = Some(((to.0 as f32 / scale).into(), (to.1 as f32 / scale).into()));
        if ui.get_open() {
            self.set_open(ui, false);
        }
        self.place_window(ui);
    }

    /// Mengakhiri seret. `true` bila kucing benar-benar dipindah (bukan klik).
    pub fn drag_end(&mut self) -> bool {
        let moved = self.drag.take().is_some_and(|d| d.moved);
        if moved {
            self.save_soon();
        }
        moved
    }

    /// Sembunyi saat aplikasi lain layar penuh. Dipanggil berkala.
    pub fn check_fullscreen(&mut self) {
        let hide = self.settings.hide_in_fullscreen && platform::fullscreen_app_active();
        if hide != self.hidden {
            self.hidden = hide;
            platform::set_hidden(hide);
        }
    }
}
