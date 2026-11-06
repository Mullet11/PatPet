//! Pengaturan aplikasi, dibaca dari dan ditulis ke `patpet.conf`.
//! `settings.json` versi lama hanya dibaca sekali untuk dipindahkan.

use serde::Deserialize;

use crate::conf::{bool_text, hms_text, parse_bool, parse_hms, Conf};

/// Isi bawaan `patpet.conf`, lengkap dengan penjelasan untuk pengguna.
pub const DEFAULT_CONF: &str = include_str!("../assets/default.conf");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Island hitam menempel di tengah atas layar.
    #[default]
    Bar,
    /// Ikon kucing yang bisa diseret ke mana saja.
    Icon,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub mode: Mode,
    /// 1 kecil, 2 sedang, 3 besar.
    pub pet_scale: i32,
    pub auto_hide: bool,
    /// Persen lebar layar: island tertutup, island melebar, panel lebar.
    pub widths: [f32; 3],
    /// Kiri-atas kucing pada mode ikon, piksel logis.
    pub icon_pos: Option<(f64, f64)>,
    pub always_on_top: bool,
    pub close_on_focus_loss: bool,
    pub hide_in_fullscreen: bool,
    pub idle_sleep_minutes: i32,
    pub timer_seconds: i32,
    pub alarm_enabled: bool,
    pub alarm_seconds: i32,
    /// Nama kota; `None` = lokasi Windows.
    pub city: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Bar,
            pet_scale: 2,
            auto_hide: false,
            widths: [15.0, 24.0, 38.0],
            icon_pos: None,
            always_on_top: true,
            close_on_focus_loss: true,
            hide_in_fullscreen: true,
            idle_sleep_minutes: 5,
            timer_seconds: 25 * 60,
            alarm_enabled: true,
            alarm_seconds: 60,
            city: None,
        }
    }
}

impl Settings {
    /// Baca pengaturan. Mengembalikan nomor baris yang nilainya tidak valid;
    /// baris itu memakai nilai bawaan.
    pub fn from_conf(c: &Conf) -> (Self, Vec<usize>) {
        let mut s = Self::default();
        let mut bad = Vec::new();

        // Isi `target` dari baris; catat barisnya bila tidak terbaca.
        // Nilai kosong berarti "pakai bawaan" dan bukan kesalahan.
        let mut read = |section: &str, key: &str, apply: &mut dyn FnMut(&str) -> bool| {
            if let Some(e) = c.get(section, key) {
                if !e.value.is_empty() && !apply(e.value) {
                    bad.push(e.line);
                }
            }
        };

        read("tampilan", "mode", &mut |v| match v.to_lowercase().as_str() {
            "bar" => {
                s.mode = Mode::Bar;
                true
            }
            "ikon" | "icon" => {
                s.mode = Mode::Icon;
                true
            }
            _ => false,
        });
        read("tampilan", "ukuran_kucing", &mut int_into(&mut s.pet_scale, 1, 3));
        read("tampilan", "sembunyi_otomatis", &mut bool_into(&mut s.auto_hide));
        for (i, key) in ["lebar_island", "lebar_island_hover", "lebar_panel"].into_iter().enumerate() {
            let widths = &mut s.widths;
            read("tampilan", key, &mut |v| {
                v.trim_end_matches('%').trim().parse::<f32>().ok().filter(|p| (5.0..=90.0).contains(p)).map(|p| widths[i] = p).is_some()
            });
        }
        read("tampilan", "posisi_ikon", &mut |v| {
            let mut it = v.split(',').map(|p| p.trim().parse::<f64>().ok().filter(|n| n.is_finite()));
            match (it.next().flatten(), it.next().flatten(), it.next()) {
                (Some(x), Some(y), None) => {
                    s.icon_pos = Some((x, y));
                    true
                }
                _ => false,
            }
        });
        read("perilaku", "selalu_di_atas", &mut bool_into(&mut s.always_on_top));
        read("perilaku", "tutup_saat_pindah_aplikasi", &mut bool_into(&mut s.close_on_focus_loss));
        read("perilaku", "sembunyi_saat_layar_penuh", &mut bool_into(&mut s.hide_in_fullscreen));
        read("perilaku", "kucing_tidur_setelah", &mut int_into(&mut s.idle_sleep_minutes, 1, 60));
        read("timer", "durasi", &mut |v| parse_hms(v).map(|t| s.timer_seconds = t).is_some());
        read("timer", "bunyikan_alarm", &mut bool_into(&mut s.alarm_enabled));
        read("timer", "lama_alarm", &mut int_into(&mut s.alarm_seconds, 5, 600));
        read("cuaca", "lokasi", &mut |v| {
            s.city = (!v.eq_ignore_ascii_case("otomatis")).then(|| v.chars().take(80).collect());
            true
        });

        bad.sort_unstable();
        (s, bad)
    }

    /// Tulis nilai yang bisa diubah PatPet sendiri (posisi kucing, timer).
    pub fn write_runtime(&self, c: &mut Conf) {
        if let Some((x, y)) = self.icon_pos {
            c.set("tampilan", "posisi_ikon", &format!("{}, {}", x.round(), y.round()));
        }
        c.set("timer", "durasi", &hms_text(self.timer_seconds));
        c.set("timer", "bunyikan_alarm", bool_text(self.alarm_enabled));
    }

    /// Tulis semua nilai, dipakai saat memindahkan pengaturan lama.
    pub fn write_all(&self, c: &mut Conf) {
        c.set("tampilan", "mode", if self.mode == Mode::Icon { "ikon" } else { "bar" });
        c.set("tampilan", "ukuran_kucing", &self.pet_scale.to_string());
        c.set("tampilan", "sembunyi_otomatis", bool_text(self.auto_hide));
        c.set("perilaku", "selalu_di_atas", bool_text(self.always_on_top));
        c.set("perilaku", "tutup_saat_pindah_aplikasi", bool_text(self.close_on_focus_loss));
        c.set("perilaku", "sembunyi_saat_layar_penuh", bool_text(self.hide_in_fullscreen));
        c.set("perilaku", "kucing_tidur_setelah", &self.idle_sleep_minutes.to_string());
        c.set("timer", "lama_alarm", &self.alarm_seconds.to_string());
        c.set("cuaca", "lokasi", self.city.as_deref().unwrap_or("otomatis"));
        self.write_runtime(c);
    }

    /// Baca `settings.json` versi lama (C# atau Rust sebelum Revisi 5).
    pub fn from_old_json(text: &str) -> Option<Self> {
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase", default)]
        struct Old {
            mode: Option<String>,
            icon_left: Option<f64>,
            icon_top: Option<f64>,
            always_on_top: Option<bool>,
            auto_hide: Option<bool>,
            hide_in_fullscreen: Option<bool>,
            close_on_focus_loss: Option<bool>,
            dock_scale: Option<i32>,
            idle_sleep_minutes: Option<i32>,
            timer: OldTimer,
            location: OldLocation,
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase", default)]
        struct OldTimer {
            duration_seconds: Option<i32>,
            alarm_enabled: Option<bool>,
            alarm_seconds: Option<i32>,
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase", default)]
        struct OldLocation {
            use_device_location: Option<bool>,
            name: Option<String>,
        }

        let old: Old = serde_json::from_str(text).ok()?;
        let d = Self::default();
        Some(Self {
            mode: if old.mode.as_deref() == Some("Icon") { Mode::Icon } else { Mode::Bar },
            pet_scale: old.dock_scale.unwrap_or(d.pet_scale).clamp(1, 3),
            auto_hide: old.auto_hide.unwrap_or(d.auto_hide),
            icon_pos: old.icon_left.zip(old.icon_top),
            always_on_top: old.always_on_top.unwrap_or(d.always_on_top),
            close_on_focus_loss: old.close_on_focus_loss.unwrap_or(d.close_on_focus_loss),
            hide_in_fullscreen: old.hide_in_fullscreen.unwrap_or(d.hide_in_fullscreen),
            idle_sleep_minutes: old.idle_sleep_minutes.unwrap_or(d.idle_sleep_minutes).clamp(1, 60),
            timer_seconds: old.timer.duration_seconds.unwrap_or(d.timer_seconds).clamp(0, 24 * 3600 - 1),
            alarm_enabled: old.timer.alarm_enabled.unwrap_or(d.alarm_enabled),
            alarm_seconds: old.timer.alarm_seconds.unwrap_or(d.alarm_seconds).clamp(5, 600),
            city: match old.location.use_device_location {
                Some(false) => old.location.name.filter(|n| !n.trim().is_empty()),
                _ => None,
            },
            ..d
        })
    }
}

/// Isi `dst` dari teks "ya"/"tidak"; `false` bila tidak terbaca.
fn bool_into(dst: &mut bool) -> impl FnMut(&str) -> bool + '_ {
    move |v| parse_bool(v).map(|b| *dst = b).is_some()
}

/// Isi `dst` dari bilangan bulat dalam rentang `min..=max`.
fn int_into(dst: &mut i32, min: i32, max: i32) -> impl FnMut(&str) -> bool + '_ {
    move |v| v.parse::<i32>().ok().filter(|n| (min..=max).contains(n)).map(|n| *dst = n).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_conf_reads_as_defaults() {
        let (s, bad) = Settings::from_conf(&Conf::parse(DEFAULT_CONF));
        assert!(bad.is_empty(), "baris tidak valid: {bad:?}");
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn invalid_lines_are_reported_and_ignored() {
        let text = DEFAULT_CONF
            .replace("mode = bar", "mode = melayang")
            .replace("ukuran_kucing = 2", "ukuran_kucing = 9")
            .replace("lokasi = otomatis", "lokasi = Banjarbaru");
        let (s, bad) = Settings::from_conf(&Conf::parse(&text));
        assert_eq!(bad.len(), 2);
        assert_eq!(s.mode, Mode::Bar);
        assert_eq!(s.pet_scale, 2);
        assert_eq!(s.city.as_deref(), Some("Banjarbaru"));
    }

    #[test]
    fn runtime_values_round_trip() {
        let mut c = Conf::parse(DEFAULT_CONF);
        let s = Settings { icon_pos: Some((1084.4, 52.8)), timer_seconds: 300, alarm_enabled: false, ..Settings::default() };
        s.write_runtime(&mut c);
        let (back, bad) = Settings::from_conf(&c);
        assert!(bad.is_empty());
        assert_eq!(back.icon_pos, Some((1084.0, 53.0)));
        assert_eq!(back.timer_seconds, 300);
        assert!(!back.alarm_enabled);
    }

    #[test]
    fn migrates_old_json() {
        let old = r#"{"mode":"Icon","iconLeft":1084,"iconTop":53,"dockScale":3,"hideInFullscreen":false,
            "timer":{"durationSeconds":300,"alarmEnabled":true,"alarmSeconds":60},"idleSleepMinutes":7,
            "location":{"useDeviceLocation":false,"name":"Banjarmasin"},"lastTrack":{"title":"x"}}"#;
        let s = Settings::from_old_json(old).unwrap();
        assert_eq!(s.mode, Mode::Icon);
        assert_eq!(s.pet_scale, 3);
        assert_eq!(s.icon_pos, Some((1084.0, 53.0)));
        assert!(!s.hide_in_fullscreen);
        assert_eq!(s.idle_sleep_minutes, 7);
        assert_eq!(s.city.as_deref(), Some("Banjarmasin"));

        let mut c = Conf::parse(DEFAULT_CONF);
        s.write_all(&mut c);
        let (back, bad) = Settings::from_conf(&c);
        assert!(bad.is_empty());
        assert_eq!(back, s);
    }
}
