//! Baterai dan suhu laptop, tanpa hak admin.

use windows::core::w;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
    PDH_MORE_DATA,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Power {
    pub has_battery: bool,
    pub percent: u8,
    pub plugged: bool,
}

impl Power {
    pub fn read() -> Self {
        let mut s = SYSTEM_POWER_STATUS::default();
        // SAFETY: struct milik kita.
        if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
            return Self { has_battery: false, percent: 0, plugged: true };
        }
        Self::from_status(s.ACLineStatus, s.BatteryFlag, s.BatteryLifePercent)
    }

    fn from_status(ac_line: u8, flag: u8, percent: u8) -> Self {
        // 128 = tidak ada baterai, 255 = tidak diketahui
        let has_battery = flag != 128 && flag != 255 && percent <= 100;
        Self {
            has_battery,
            percent: if has_battery { percent } else { 0 },
            plugged: ac_line == 1,
        }
    }
}

/// Suhu dari counter "Thermal Zone Information" (Kelvin). Handle ditutup di Drop.
pub struct Thermal {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

impl Thermal {
    pub fn open() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: handle keluaran milik kita; query ditutup bila counter gagal.
        unsafe {
            if PdhOpenQueryW(None, 0, &mut query) != 0 {
                return None;
            }
            let path = w!("\\Thermal Zone Information(*)\\Temperature");
            if PdhAddEnglishCounterW(query, path, 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
        }
        Some(Self { query, counter })
    }

    /// Suhu tertinggi dari semua zona, dalam °C.
    pub fn read_celsius(&mut self) -> Option<i32> {
        // SAFETY: buffer dialokasikan sesuai ukuran yang diminta PDH dan
        // hanya dibaca sebanyak `count` item.
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            let rc = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if rc != PDH_MORE_DATA || size == 0 || size > 64 * 1024 {
                return None;
            }
            let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::with_capacity(size as usize / item_size + 1);
            let rc = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                Some(buf.as_mut_ptr()),
            );
            if rc != 0 {
                return None;
            }
            buf.set_len(count as usize);
            buf.iter()
                .filter(|i| i.FmtValue.CStatus == 0)
                .map(|i| i.FmtValue.Anonymous.doubleValue - 273.15)
                .filter(|c| *c > 0.0 && *c < 125.0)
                .fold(None, |max: Option<f64>, c| Some(max.map_or(c, |m| m.max(c))))
                .map(|c| c.round() as i32)
        }
    }
}

impl Drop for Thermal {
    fn drop(&mut self) {
        // SAFETY: query dibuka di `open` dan hanya ditutup di sini.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laptop_on_battery() {
        let p = Power::from_status(0, 0, 76);
        assert_eq!(p, Power { has_battery: true, percent: 76, plugged: false });
    }

    #[test]
    fn desktop_without_battery() {
        assert!(!Power::from_status(1, 128, 255).has_battery);
        assert!(!Power::from_status(1, 255, 255).has_battery);
    }
}
