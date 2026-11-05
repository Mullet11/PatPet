//! Waktu lokal dan format teksnya dalam bahasa Indonesia.

use windows::Win32::System::SystemInformation::GetLocalTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,   // 1–12
    pub day: u16,     // 1–31
    pub weekday: u16, // 0 = Minggu
    pub hour: u16,
    pub minute: u16,
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
        }
    }

    /// "09:41"
    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// "Jumat, 6 November"
    pub fn date_text(&self) -> String {
        const DAYS: [&str; 7] = ["Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu"];
        const MONTHS: [&str; 12] = [
            "Januari", "Februari", "Maret", "April", "Mei", "Juni",
            "Juli", "Agustus", "September", "Oktober", "November", "Desember",
        ];
        let day = DAYS[self.weekday as usize % 7];
        let month = MONTHS[(self.month as usize).clamp(1, 12) - 1];
        format!("{day}, {} {month}", self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(weekday: u16, day: u16, month: u16, hour: u16, minute: u16) -> LocalTime {
        LocalTime { year: 2026, month, day, weekday, hour, minute }
    }

    #[test]
    fn time_is_zero_padded() {
        assert_eq!(at(5, 6, 11, 9, 4).time_text(), "09:04");
        assert_eq!(at(5, 6, 11, 23, 59).time_text(), "23:59");
    }

    #[test]
    fn date_uses_indonesian_names() {
        assert_eq!(at(5, 6, 11, 0, 0).date_text(), "Jumat, 6 November");
        assert_eq!(at(0, 1, 1, 0, 0).date_text(), "Minggu, 1 Januari");
    }
}
