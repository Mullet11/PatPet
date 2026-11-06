//! Waktu lokal dan format teksnya dalam bahasa Indonesia.

use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

use crate::notes::Moment;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,   // 1–12
    pub day: u16,     // 1–31
    pub weekday: u16, // 0 = Minggu
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

impl LocalTime {
    pub fn now() -> Self {
        // SAFETY: GetLocalTime hanya mengisi struct yang kita miliki.
        let t = unsafe { GetLocalTime() };
        Self {
            year: t.wYear,
            month: t.wMonth,
            day: t.wDay,
            weekday: t.wDayOfWeek,
            hour: t.wHour,
            minute: t.wMinute,
            second: t.wSecond,
        }
    }

    pub fn moment(&self) -> Moment {
        Moment::new(self.year.into(), self.month.into(), self.day.into(), self.hour.into(), self.minute.into())
    }

    /// Cap waktu ISO 8601 dengan zona, mis. "2026-11-06T02:40:15+08:00".
    pub fn stamp(&self) -> String {
        let offset = utc_offset_minutes();
        let sign = if offset < 0 { '-' } else { '+' };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second,
            offset.abs() / 60, offset.abs() % 60
        )
    }

    /// "20261106", untuk nama file cadangan.
    pub fn compact_date(&self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }

    /// "09:41"
    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// Jam:menit setelah `after` detik dari sekarang, mis. akhir timer.
    pub fn time_after(&self, after: u64) -> String {
        let now = (self.hour as u64 * 60 + self.minute as u64) * 60 + self.second as u64;
        let end = (now + after) % (24 * 3600);
        format!("{:02}:{:02}", end / 3600, end / 60 % 60)
    }

    pub fn greeting(&self) -> &'static str {
        match self.hour {
            4..=9 => "Selamat pagi",
            10..=14 => "Selamat siang",
            15..=17 => "Selamat sore",
            _ => "Selamat malam",
        }
    }

    /// "Jumat, 6 November 2026"
    pub fn date_text(&self) -> String {
        const DAYS: [&str; 7] = ["Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu"];
        const MONTHS: [&str; 12] = [
            "Januari", "Februari", "Maret", "April", "Mei", "Juni",
            "Juli", "Agustus", "September", "Oktober", "November", "Desember",
        ];
        let day = DAYS[self.weekday as usize % 7];
        let month = MONTHS[(self.month as usize).clamp(1, 12) - 1];
        format!("{day}, {} {month} {}", self.day, self.year)
    }
}

/// "WIB", "WITA", "WIT", atau "UTC+5:30" untuk zona lain.
pub fn zone_label() -> String {
    zone_from_offset(utc_offset_minutes())
}

/// Selisih waktu lokal dari UTC dalam menit, mis. 480 untuk WITA.
fn utc_offset_minutes() -> i32 {
    let mut info = TIME_ZONE_INFORMATION::default();
    // SAFETY: struct milik kita.
    let id = unsafe { GetTimeZoneInformation(&mut info) };
    let extra = if id == 2 { info.DaylightBias } else { info.StandardBias }; // 2 = sedang DST
    -(info.Bias + extra)
}

fn zone_from_offset(minutes: i32) -> String {
    match minutes {
        420 => "WIB".into(),
        480 => "WITA".into(),
        540 => "WIT".into(),
        m => {
            let sign = if m < 0 { '-' } else { '+' };
            let (h, min) = (m.abs() / 60, m.abs() % 60);
            if min == 0 { format!("UTC{sign}{h}") } else { format!("UTC{sign}{h}:{min:02}") }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(weekday: u16, day: u16, month: u16, hour: u16, minute: u16) -> LocalTime {
        LocalTime { year: 2026, month, day, weekday, hour, minute, second: 0 }
    }

    #[test]
    fn time_is_zero_padded() {
        assert_eq!(at(5, 6, 11, 9, 4).time_text(), "09:04");
        assert_eq!(at(5, 6, 11, 23, 59).time_text(), "23:59");
    }

    #[test]
    fn date_uses_indonesian_names() {
        assert_eq!(at(5, 6, 11, 0, 0).date_text(), "Jumat, 6 November 2026");
        assert_eq!(at(0, 1, 1, 0, 0).date_text(), "Minggu, 1 Januari 2026");
    }

    #[test]
    fn end_time_wraps_past_midnight() {
        assert_eq!(at(5, 6, 11, 14, 5).time_after(25 * 60), "14:30");
        assert_eq!(at(5, 6, 11, 23, 50).time_after(20 * 60), "00:10");
    }

    #[test]
    fn greeting_follows_hour() {
        assert_eq!(at(0, 1, 1, 4, 0).greeting(), "Selamat pagi");
        assert_eq!(at(0, 1, 1, 12, 0).greeting(), "Selamat siang");
        assert_eq!(at(0, 1, 1, 17, 59).greeting(), "Selamat sore");
        assert_eq!(at(0, 1, 1, 3, 0).greeting(), "Selamat malam");
    }

    #[test]
    fn indonesian_zones_have_names() {
        assert_eq!(zone_from_offset(420), "WIB");
        assert_eq!(zone_from_offset(480), "WITA");
        assert_eq!(zone_from_offset(540), "WIT");
        assert_eq!(zone_from_offset(330), "UTC+5:30");
        assert_eq!(zone_from_offset(-300), "UTC-5");
    }
}
