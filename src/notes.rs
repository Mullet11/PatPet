//! Catatan di SQLite. Skema dan format kolom sama persis dengan versi C#,
//! jadi `notes.db` lama langsung terbaca tanpa migrasi.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};

pub const MAX_CHARS: usize = 4000;

/// Waktu lokal setingkat menit: hari sejak 1970-01-01 dan menit dalam hari.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Moment {
    pub day: i64,
    pub minute: u32,
}

impl Moment {
    pub fn new(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> Self {
        Self { day: days_from_civil(year, month, day), minute: hour * 60 + minute }
    }

    pub fn hour(&self) -> u32 {
        self.minute / 60
    }

    /// Format kolom `deadline`: "2026-11-06T21:30:00" (urut sebagai teks).
    pub fn to_db(&self) -> String {
        let (y, m, d) = civil_from_days(self.day);
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:00", self.minute / 60, self.minute % 60)
    }

    pub fn from_db(s: &str) -> Option<Self> {
        let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<u32>().ok();
        let (y, mo, d, h, mi) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?);
        ((1..=12).contains(&mo) && (1..=31).contains(&d) && h < 24 && mi < 60)
            .then(|| Self::new(y.into(), mo, d, h, mi))
    }

    /// "Hari ini 21:30", "Besok 08:00", "Sen, 9 Nov 14:00", "9 Nov 2027 14:00".
    pub fn label(&self, now: Moment) -> String {
        let time = format!("{:02}:{:02}", self.minute / 60, self.minute % 60);
        match self.day - now.day {
            0 => format!("Hari ini {time}"),
            1 => format!("Besok {time}"),
            -1 => format!("Kemarin {time}"),
            _ => {
                let (y, m, d) = civil_from_days(self.day);
                let month = MONTHS[(m - 1) as usize];
                if y == civil_from_days(now.day).0 {
                    format!("{}, {d} {month} {time}", WEEKDAYS[weekday(self.day)])
                } else {
                    format!("{d} {month} {y} {time}")
                }
            }
        }
    }

    /// "Sen, 9 Nov" untuk pemilih tanggal.
    pub fn date_label(&self, now: Moment) -> String {
        match self.day - now.day {
            0 => "Hari ini".into(),
            1 => "Besok".into(),
            _ => {
                let (_, m, d) = civil_from_days(self.day);
                format!("{}, {d} {}", WEEKDAYS[weekday(self.day)], MONTHS[(m - 1) as usize])
            }
        }
    }
}

const WEEKDAYS: [&str; 7] = ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"];
const MONTHS_FULL: [&str; 12] = [
    "Januari", "Februari", "Maret", "April", "Mei", "Juni",
    "Juli", "Agustus", "September", "Oktober", "November", "Desember",
];

/// Satu bulan untuk pemilih tanggal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Month {
    pub year: i64,
    /// 1–12
    pub month: u32,
}

impl Month {
    pub fn of(day: i64) -> Self {
        let (year, month, _) = civil_from_days(day);
        Self { year, month }
    }

    pub fn shifted(self, by: i32) -> Self {
        let index = self.year * 12 + self.month as i64 - 1 + by as i64;
        Self { year: index.div_euclid(12), month: index.rem_euclid(12) as u32 + 1 }
    }

    /// "November 2026"
    pub fn title(self) -> String {
        format!("{} {}", MONTHS_FULL[(self.month - 1) as usize], self.year)
    }

    /// 42 hari (6 minggu) yang tampil di kalender, mulai Senin sebelum
    /// atau tepat pada tanggal 1. Nilainya hari sejak 1970-01-01.
    pub fn grid(self) -> [i64; 42] {
        let first = days_from_civil(self.year, self.month, 1);
        let monday_offset = (weekday(first) + 6) % 7; // Senin = 0
        let start = first - monday_offset as i64;
        std::array::from_fn(|i| start + i as i64)
    }

    pub fn contains(self, day: i64) -> bool {
        Self::of(day) == self
    }
}

/// Tanggal dalam bulan (1–31).
pub fn day_of_month(day: i64) -> u32 {
    civil_from_days(day).2
}
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];

/// 0 = Minggu. 1970-01-01 adalah Kamis.
fn weekday(day: i64) -> usize {
    (day + 4).rem_euclid(7) as usize
}

// Konversi tanggal kalender ↔ jumlah hari (algoritma Howard Hinnant).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    None,
    Done,
    Overdue,
    Today,
    /// Dalam 2 hari.
    Soon,
    Later,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: i64,
    pub content: String,
    pub pinned: bool,
    pub done: bool,
    pub deadline: Option<Moment>,
    created: String,
    updated: String,
}

impl Note {
    pub fn due(&self, now: Moment) -> Due {
        match self.deadline {
            None => Due::None,
            Some(_) if self.done => Due::Done,
            Some(d) if d <= now => Due::Overdue,
            Some(d) if d.day == now.day => Due::Today,
            Some(d) if (d.day - now.day) * 1440 + d.minute as i64 - now.minute as i64 <= 2 * 1440 => Due::Soon,
            Some(_) => Due::Later,
        }
    }
}

pub struct Notes {
    db: Connection,
}

const COLUMNS: &str = "id, content, is_pinned, created_at, updated_at, deadline, is_done";

impl Notes {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        Self::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA cache_size = -256;
             CREATE TABLE IF NOT EXISTS notes (
               id         INTEGER PRIMARY KEY AUTOINCREMENT,
               content    TEXT    NOT NULL,
               is_pinned  INTEGER NOT NULL DEFAULT 0,
               created_at TEXT    NOT NULL,
               updated_at TEXT    NOT NULL,
               deadline   TEXT    NULL,
               is_done    INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_notes_order ON notes(is_done, is_pinned DESC, deadline, updated_at DESC);",
        )?;
        Ok(Self { db })
    }

    fn row(r: &Row) -> rusqlite::Result<Note> {
        Ok(Note {
            id: r.get(0)?,
            content: r.get(1)?,
            pinned: r.get::<_, i64>(2)? != 0,
            created: r.get(3)?,
            updated: r.get(4)?,
            deadline: r.get::<_, Option<String>>(5)?.as_deref().and_then(Moment::from_db),
            done: r.get::<_, i64>(6)? != 0,
        })
    }

    /// Belum selesai dulu, lalu yang disematkan, lalu deadline terdekat, lalu yang terbaru diubah.
    pub fn list(&self, search: &str) -> rusqlite::Result<Vec<Note>> {
        let pattern = format!("%{}%", escape_like(search.trim()));
        let mut stmt = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM notes
             WHERE content LIKE ?1 ESCAPE '\\'
             ORDER BY is_done ASC, is_pinned DESC, deadline IS NULL, deadline ASC, updated_at DESC"
        ))?;
        let rows = stmt.query_map([pattern], Self::row)?.collect();
        rows
    }

    /// Belum selesai dan deadline sudah lewat.
    pub fn overdue(&self, now: Moment) -> rusqlite::Result<Vec<Note>> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM notes
             WHERE is_done = 0 AND deadline IS NOT NULL AND deadline <= ?1 ORDER BY deadline"
        ))?;
        let rows = stmt.query_map([now.to_db()], Self::row)?.collect();
        rows
    }

    pub fn add(&self, content: &str, deadline: Option<Moment>, stamp: &str) -> rusqlite::Result<i64> {
        self.db.execute(
            "INSERT INTO notes (content, is_pinned, created_at, updated_at, deadline, is_done)
             VALUES (?1, 0, ?2, ?2, ?3, 0)",
            params![clip(content), stamp, deadline.map(|d| d.to_db())],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn set_content(&self, id: i64, content: &str, stamp: &str) -> rusqlite::Result<()> {
        self.db
            .execute("UPDATE notes SET content = ?1, updated_at = ?2 WHERE id = ?3", params![clip(content), stamp, id])
            .map(drop)
    }

    pub fn set_deadline(&self, id: i64, deadline: Option<Moment>, stamp: &str) -> rusqlite::Result<()> {
        self.db
            .execute(
                "UPDATE notes SET deadline = ?1, updated_at = ?2 WHERE id = ?3",
                params![deadline.map(|d| d.to_db()), stamp, id],
            )
            .map(drop)
    }

    pub fn set_done(&self, id: i64, done: bool) -> rusqlite::Result<()> {
        self.db.execute("UPDATE notes SET is_done = ?1 WHERE id = ?2", params![done, id]).map(drop)
    }

    pub fn toggle_pin(&self, id: i64) -> rusqlite::Result<()> {
        self.db.execute("UPDATE notes SET is_pinned = 1 - is_pinned WHERE id = ?1", [id]).map(drop)
    }

    /// Menghapus dan mengembalikan catatannya supaya bisa dibatalkan.
    pub fn delete(&self, id: i64) -> rusqlite::Result<Option<Note>> {
        let note = self
            .db
            .query_row(&format!("SELECT {COLUMNS} FROM notes WHERE id = ?1"), [id], Self::row)
            .optional()?;
        self.db.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        Ok(note)
    }

    /// Mengembalikan catatan yang baru dihapus, dengan id dan isi yang sama.
    pub fn restore(&self, n: &Note) -> rusqlite::Result<()> {
        self.db
            .execute(
                &format!("INSERT INTO notes ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
                params![n.id, n.content, n.pinned, n.created, n.updated, n.deadline.map(|d| d.to_db()), n.done],
            )
            .map(drop)
    }

    /// Salinan harian `notes-YYYYMMDD.db`; hanya `keep` salinan terbaru yang disimpan.
    pub fn backup_daily(&self, dir: &Path, today: &str, keep: usize) {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let target = dir.join(format!("notes-{today}.db"));
        if !target.exists() {
            let _ = self.db.execute("VACUUM INTO ?1", [target.to_string_lossy()]);
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("notes-") && n.ends_with(".db")))
            .collect();
        files.sort();
        for old in files.iter().rev().skip(keep) {
            let _ = std::fs::remove_file(old);
        }
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn clip(s: &str) -> String {
    s.chars().take(MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: &str = "2026-11-06T10:00:00+08:00";

    fn at(day: u32, hour: u32, minute: u32) -> Moment {
        Moment::new(2026, 11, day, hour, minute)
    }

    #[test]
    fn calendar_round_trips() {
        for day in [-1000, 0, 19_000, 20_398, 60_000] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(weekday(at(6, 0, 0).day), 5); // 6 Nov 2026 hari Jumat
    }

    #[test]
    fn deadline_text_matches_old_format() {
        let m = at(6, 21, 30);
        assert_eq!(m.to_db(), "2026-11-06T21:30:00");
        assert_eq!(Moment::from_db("2026-11-06T21:30:00"), Some(m));
        assert_eq!(Moment::from_db("rusak"), None);
    }

    #[test]
    fn calendar_grid_starts_on_monday() {
        let nov = Month { year: 2026, month: 11 };
        let grid = nov.grid();
        // 1 November 2026 hari Minggu, jadi baris pertama mulai Senin 26 Oktober.
        assert_eq!(civil_from_days(grid[0]), (2026, 10, 26));
        assert_eq!(weekday(grid[0]), 1);
        assert_eq!(civil_from_days(grid[6]), (2026, 11, 1));
        assert!(nov.contains(grid[6]) && !nov.contains(grid[0]));
        assert_eq!(nov.title(), "November 2026");
    }

    #[test]
    fn months_shift_across_years() {
        let jan = Month { year: 2027, month: 1 };
        assert_eq!(jan.shifted(-1), Month { year: 2026, month: 12 });
        assert_eq!(jan.shifted(13), Month { year: 2028, month: 2 });
        assert_eq!(Month::of(at(6, 0, 0).day), Month { year: 2026, month: 11 });
    }

    #[test]
    fn labels_are_relative() {
        let now = at(6, 10, 0);
        assert_eq!(at(6, 21, 30).label(now), "Hari ini 21:30");
        assert_eq!(at(7, 8, 0).label(now), "Besok 08:00");
        assert_eq!(at(5, 8, 0).label(now), "Kemarin 08:00");
        assert_eq!(at(9, 14, 0).label(now), "Sen, 9 Nov 14:00");
        assert_eq!(Moment::new(2027, 1, 2, 9, 5).label(now), "2 Jan 2027 09:05");
    }

    #[test]
    fn due_states() {
        let now = at(6, 10, 0);
        let mut n = Note {
            id: 1,
            content: "x".into(),
            pinned: false,
            done: false,
            deadline: Some(at(6, 9, 0)),
            created: STAMP.into(),
            updated: STAMP.into(),
        };
        assert_eq!(n.due(now), Due::Overdue);
        n.deadline = Some(at(6, 20, 0));
        assert_eq!(n.due(now), Due::Today);
        n.deadline = Some(at(8, 9, 0));
        assert_eq!(n.due(now), Due::Soon);
        n.deadline = Some(at(20, 9, 0));
        assert_eq!(n.due(now), Due::Later);
        n.done = true;
        assert_eq!(n.due(now), Due::Done);
    }

    #[test]
    fn list_orders_and_searches() {
        let db = Notes::in_memory();
        let a = db.add("beli susu", None, STAMP).unwrap();
        let b = db.add("tugas kuliah", Some(at(7, 9, 0)), STAMP).unwrap();
        let c = db.add("100% selesai", None, STAMP).unwrap();
        db.toggle_pin(a).unwrap();
        db.set_done(c, true).unwrap();

        let ids: Vec<i64> = db.list("").unwrap().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![a, b, c]);
        assert_eq!(db.list("KULIAH").unwrap().len(), 1);
        assert_eq!(db.list("100%").unwrap().len(), 1); // % dicari apa adanya
    }

    #[test]
    fn delete_can_be_undone() {
        let db = Notes::in_memory();
        let id = db.add("penting", Some(at(7, 9, 0)), STAMP).unwrap();
        let deleted = db.delete(id).unwrap().unwrap();
        assert!(db.list("").unwrap().is_empty());
        db.restore(&deleted).unwrap();
        assert_eq!(db.list("").unwrap(), vec![deleted]);
    }

    #[test]
    fn overdue_skips_done_and_future() {
        let db = Notes::in_memory();
        let late = db.add("lewat", Some(at(6, 8, 0)), STAMP).unwrap();
        let done = db.add("sudah", Some(at(6, 8, 0)), STAMP).unwrap();
        db.add("nanti", Some(at(6, 23, 0)), STAMP).unwrap();
        db.set_done(done, true).unwrap();
        let ids: Vec<i64> = db.overdue(at(6, 10, 0)).unwrap().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![late]);
    }
}
