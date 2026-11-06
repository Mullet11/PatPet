//! Warna UI dari `patpet.conf`. Warna dasar punya nilai bawaan; warna detail
//! yang dikosongkan mengikuti salah satu warna dasar.

use slint::{Color, ComponentHandle};

use crate::conf::{parse_hex, Conf};
use crate::{AppWindow, Palette};

type Set = fn(&Palette<'_>, Color);

/// (bagian, kunci, bawaan, setter). Bawaan harus sama dengan assets/default.conf.
const BASE: [(&str, &str, &str, Set); 11] = [
    ("warna_dasar", "latar", "#000000", |p, c| p.set_ink(c)),
    ("warna_dasar", "kartu", "#171412", |p, c| p.set_raised(c)),
    ("warna_dasar", "kartu_disorot", "#231F1C", |p, c| p.set_raised_hover(c)),
    ("warna_dasar", "garis", "#2B2623", |p, c| p.set_line(c)),
    ("warna_teks", "teks", "#FEF5E6", |p, c| p.set_text(c)),
    ("warna_teks", "teks_redup", "#A6988B", |p, c| p.set_muted(c)),
    ("warna_teks", "teks_samar", "#6E645C", |p, c| p.set_faint(c)),
    ("warna_aksen", "aksen", "#FDD5B5", |p, c| p.set_accent(c)),
    ("warna_aksen", "teks_di_aksen", "#1E1612", |p, c| p.set_on_accent(c)),
    ("warna_aksen", "musik", "#7882C8", |p, c| p.set_music(c)),
    ("warna_aksen", "peringatan", "#E65A6E", |p, c| p.set_alert(c)),
];

/// (kunci di [warna_detail], kunci dasar yang diikuti bila kosong, setter).
const DETAIL: [(&str, &str, Set); 11] = [
    ("baterai", "teks", |p, c| p.set_battery(c)),
    ("baterai_mengisi", "aksen", |p, c| p.set_battery_charging(c)),
    ("baterai_lemah", "peringatan", |p, c| p.set_battery_low(c)),
    ("suhu_panas", "peringatan", |p, c| p.set_temp_hot(c)),
    ("progres_timer", "aksen", |p, c| p.set_timer_progress(c)),
    ("progres_lagu", "musik", |p, c| p.set_music_progress(c)),
    ("sakelar_nyala", "aksen", |p, c| p.set_switch_on(c)),
    ("bingkai_fokus", "aksen", |p, c| p.set_focus_ring(c)),
    ("deadline_lewat", "peringatan", |p, c| p.set_due_overdue(c)),
    ("deadline_hari_ini", "aksen", |p, c| p.set_due_today(c)),
    ("notifikasi", "aksen", |p, c| p.set_notice(c)),
];

fn color((r, g, b, a): (u8, u8, u8, u8)) -> Color {
    Color::from_argb_u8(a, r, g, b)
}

/// Terapkan semua warna. Mengembalikan nomor baris yang kodenya tidak valid.
pub fn apply(ui: &AppWindow, conf: &Conf) -> Vec<usize> {
    let palette = ui.global::<Palette>();
    let mut bad = Vec::new();
    let mut base = Vec::with_capacity(BASE.len());

    let mut read = |section: &str, key: &str| -> Option<(u8, u8, u8, u8)> {
        let entry = conf.get(section, key)?;
        if entry.value.is_empty() {
            return None;
        }
        let parsed = parse_hex(entry.value);
        if parsed.is_none() {
            bad.push(entry.line);
        }
        parsed
    };

    for (section, key, default, set) in BASE {
        let value = read(section, key).or_else(|| parse_hex(default)).expect("warna bawaan valid");
        set(&palette, color(value));
        base.push((key, value));
    }
    for (key, follows, set) in DETAIL {
        let fallback = base.iter().find(|(k, _)| *k == follows).map(|(_, v)| *v).expect("warna dasar ada");
        set(&palette, color(read("warna_detail", key).unwrap_or(fallback)));
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DEFAULT_CONF;

    #[test]
    fn defaults_match_the_template() {
        let conf = Conf::parse(DEFAULT_CONF);
        for (section, key, default, _) in BASE {
            assert_eq!(conf.get(section, key).map(|e| e.value), Some(default), "{section}.{key}");
        }
        for (key, follows, _) in DETAIL {
            assert!(conf.get("warna_detail", key).is_some(), "warna_detail.{key} ada di template");
            assert!(BASE.iter().any(|(_, k, _, _)| *k == follows), "{key} mengikuti warna dasar yang ada");
        }
    }
}
