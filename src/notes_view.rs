//! Status tab Catatan: daftar, suntingan yang ditunda, batal hapus,
//! pemilih deadline, dan notifikasi deadline.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use slint::{Model, ModelRc, VecModel};

use crate::clock::LocalTime;
use crate::notes::{day_of_month, Due as DueState, Moment, Month, Note, Notes};
use crate::notices::{glyph, Notice};
use crate::{paths, with_app, AppWindow, CalendarDay, Due, NoteRow, TAB_NOTES};

/// Suntingan disimpan setelah pengguna berhenti mengetik selama ini.
const SAVE_AFTER: Duration = Duration::from_millis(600);
const UNDO_FOR: Duration = Duration::from_secs(5);
const BACKUPS_KEPT: usize = 7;

struct Picker {
    /// `None` = deadline untuk catatan baru.
    note: Option<i64>,
    day: i64,
    /// Bulan yang sedang tampil di kalender.
    month: Month,
}

pub struct NotesView {
    db: Option<Notes>,
    model: Rc<VecModel<NoteRow>>,
    draft_deadline: Option<Moment>,
    picker: Option<Picker>,
    days: Rc<VecModel<CalendarDay>>,
    deleted: Option<Note>,
    undo_timer: slint::Timer,
    pending: Vec<(i64, String)>,
    save_timer: slint::Timer,
    /// Catatan yang sudah diberi notifikasi lewat deadline di sesi ini.
    notified: HashSet<i64>,
}

impl NotesView {
    pub fn open(ui: &AppWindow) -> Self {
        let dir = paths::data_dir();
        let db = Notes::open(&dir.join("notes.db")).ok();
        if let Some(db) = &db {
            db.backup_daily(&dir.join("backups"), &LocalTime::now().compact_date(), BACKUPS_KEPT);
        }
        let model = Rc::new(VecModel::default());
        ui.set_notes(ModelRc::from(model.clone()));
        let days = Rc::new(VecModel::default());
        ui.set_picker_days(ModelRc::from(days.clone()));
        let view = Self {
            db,
            model,
            draft_deadline: None,
            picker: None,
            days,
            deleted: None,
            undo_timer: slint::Timer::default(),
            pending: Vec::new(),
            save_timer: slint::Timer::default(),
            notified: HashSet::new(),
        };
        view.refresh(ui);
        view
    }

    /// Susun ulang daftar dari database. Jangan dipanggil saat pengguna sedang
    /// mengetik di sebuah catatan, karena kolomnya akan dibuat ulang.
    pub fn refresh(&self, ui: &AppWindow) {
        let Some(db) = &self.db else { return };
        let now = LocalTime::now().moment();
        let notes = db.list(&ui.get_note_search()).unwrap_or_default();
        let rows: Vec<NoteRow> = notes.iter().map(|n| row(n, now)).collect();
        self.model.set_vec(rows);
        ui.set_notes_overdue(db.overdue(now).map_or(0, |v| v.len() as i32));
    }

    pub fn add(&mut self, ui: &AppWindow) -> bool {
        let text = ui.get_note_draft().trim().to_string();
        let Some(db) = &self.db else { return false };
        if text.is_empty() || db.add(&text, self.draft_deadline, &LocalTime::now().stamp()).is_err() {
            return false;
        }
        ui.set_note_draft("".into());
        self.draft_deadline = None;
        ui.set_note_draft_deadline("".into());
        self.refresh(ui);
        true
    }

    pub fn toggle_done(&mut self, ui: &AppWindow, id: i64) {
        self.flush();
        if let Some(db) = &self.db {
            let done = self.model.iter().any(|r| r.id as i64 == id && r.done);
            let _ = db.set_done(id, !done);
        }
        self.refresh(ui);
    }

    pub fn toggle_pin(&mut self, ui: &AppWindow, id: i64) {
        self.flush();
        if let Some(db) = &self.db {
            let _ = db.toggle_pin(id);
        }
        self.refresh(ui);
    }

    pub fn remove(&mut self, ui: &AppWindow, id: i64) {
        self.flush();
        let Some(db) = &self.db else { return };
        self.deleted = db.delete(id).ok().flatten();
        ui.set_can_undo(self.deleted.is_some());
        self.undo_timer.start(slint::TimerMode::SingleShot, UNDO_FOR, || {
            with_app(|app, ui| {
                app.notes.deleted = None;
                ui.set_can_undo(false);
            });
        });
        self.refresh(ui);
    }

    pub fn undo(&mut self, ui: &AppWindow) {
        self.undo_timer.stop();
        ui.set_can_undo(false);
        if let (Some(db), Some(note)) = (&self.db, self.deleted.take()) {
            let _ = db.restore(&note);
        }
        self.refresh(ui);
    }

    /// Simpan isi catatan setelah jeda mengetik, tanpa menyusun ulang daftar.
    pub fn edited(&mut self, id: i64, text: String) {
        match self.pending.iter_mut().find(|(p, _)| *p == id) {
            Some(entry) => entry.1 = text,
            None => self.pending.push((id, text)),
        }
        self.save_timer.start(slint::TimerMode::SingleShot, SAVE_AFTER, || {
            with_app(|app, _| app.notes.flush());
        });
    }

    pub fn flush(&mut self) {
        self.save_timer.stop();
        let Some(db) = &self.db else { return };
        let stamp = LocalTime::now().stamp();
        for (id, text) in self.pending.drain(..) {
            let _ = db.set_content(id, &text, &stamp);
        }
    }

    // ---------------------------------------------------------------- deadline

    pub fn pick(&mut self, ui: &AppWindow, id: i32) {
        self.flush();
        let now = LocalTime::now().moment();
        let note = (id >= 0).then_some(id as i64);
        let current = match note {
            Some(id) => self.db.as_ref().and_then(|db| db.list("").ok()?.into_iter().find(|n| n.id == id)?.deadline),
            None => self.draft_deadline,
        };
        // Bawaan: satu jam dari sekarang, dibulatkan ke jam penuh.
        let start = current.unwrap_or(Moment { day: now.day, minute: ((now.hour() + 1) % 24) * 60 });
        let start = if current.is_none() && now.hour() == 23 { Moment { day: now.day + 1, ..start } } else { start };
        self.picker = Some(Picker { note, day: start.day, month: Month::of(start.day) });
        ui.set_picker_hour(start.hour() as i32);
        ui.set_picker_minute((start.minute % 60) as i32);
        self.show_picker(ui);
        ui.set_picking(true);
    }

    pub fn picker_month_shift(&mut self, ui: &AppWindow, by: i32) {
        if let Some(p) = &mut self.picker {
            p.month = p.month.shifted(by);
        }
        self.show_picker(ui);
    }

    pub fn picker_pick_day(&mut self, ui: &AppWindow, day: i32) {
        if let Some(p) = &mut self.picker {
            p.day = day.into();
            // Tanggal dari bulan sebelah: pindah ke bulan itu.
            p.month = Month::of(p.day);
        }
        self.show_picker(ui);
    }

    /// Isi kalender (42 kotak) dan tanggal terpilih.
    fn show_picker(&self, ui: &AppWindow) {
        let Some(p) = &self.picker else { return };
        let now = LocalTime::now().moment();
        let cells: Vec<CalendarDay> = p
            .month
            .grid()
            .iter()
            .map(|&d| CalendarDay {
                label: day_of_month(d).to_string().into(),
                key: d as i32,
                in_month: p.month.contains(d),
                today: d == now.day,
                selected: d == p.day,
            })
            .collect();
        self.days.set_vec(cells);
        ui.set_picker_month(p.month.title().into());
        ui.set_picker_date(Moment { day: p.day, minute: 0 }.date_label(now).into());
    }

    pub fn picker_save(&mut self, ui: &AppWindow) {
        let Some(p) = self.picker.take() else { return };
        let minute = (ui.get_picker_hour().clamp(0, 23) * 60 + ui.get_picker_minute().clamp(0, 59)) as u32;
        self.apply_deadline(ui, p.note, Some(Moment { day: p.day, minute }));
    }

    pub fn picker_clear(&mut self, ui: &AppWindow) {
        if let Some(p) = self.picker.take() {
            self.apply_deadline(ui, p.note, None);
        }
    }

    pub fn picker_cancel(&mut self, ui: &AppWindow) {
        self.picker = None;
        ui.set_picking(false);
    }

    fn apply_deadline(&mut self, ui: &AppWindow, note: Option<i64>, deadline: Option<Moment>) {
        ui.set_picking(false);
        match note {
            Some(id) => {
                if let Some(db) = &self.db {
                    let _ = db.set_deadline(id, deadline, &LocalTime::now().stamp());
                }
                self.notified.remove(&id); // deadline baru boleh diingatkan lagi
                self.refresh(ui);
            }
            None => {
                self.draft_deadline = deadline;
                let now = LocalTime::now().moment();
                ui.set_note_draft_deadline(deadline.map(|d| d.label(now)).unwrap_or_default().into());
            }
        }
    }

    // ----------------------------------------------------------- notifikasi

    /// Notifikasi untuk catatan yang baru saja lewat deadline. Dipanggil tiap menit.
    pub fn check_due(&mut self, ui: &AppWindow) -> Option<Notice> {
        let db = self.db.as_ref()?;
        let now = LocalTime::now().moment();
        let overdue = db.overdue(now).unwrap_or_default();
        ui.set_notes_overdue(overdue.len() as i32);

        let fresh: Vec<&Note> = overdue.iter().filter(|n| !self.notified.contains(&n.id)).collect();
        let notice = match fresh.as_slice() {
            [] => None,
            [one] => Some(Notice::alert(
                glyph::CALENDAR,
                format!("Deadline: {}", shorten(&one.content)),
                one.deadline.map_or(String::new(), |d| d.label(now)),
            )),
            many => Some(Notice::alert(
                glyph::CALENDAR,
                "Deadline terlewat",
                format!("{} catatan sudah melewati deadline", many.len()),
            )),
        };
        self.notified.extend(fresh.iter().map(|n| n.id));
        notice.map(|n| n.with_tab(TAB_NOTES))
    }
}

fn row(n: &Note, now: Moment) -> NoteRow {
    NoteRow {
        id: n.id as i32,
        text: n.content.as_str().into(),
        done: n.done,
        pinned: n.pinned,
        deadline: n.deadline.map(|d| d.label(now)).unwrap_or_default().into(),
        due: match n.due(now) {
            DueState::None => Due::None,
            DueState::Done => Due::Done,
            DueState::Overdue => Due::Overdue,
            DueState::Today => Due::Today,
            DueState::Soon => Due::Soon,
            DueState::Later => Due::Later,
        },
    }
}

/// Baris pertama, paling banyak 50 karakter.
fn shorten(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default().trim();
    if line.chars().count() > 50 {
        format!("{}…", line.chars().take(50).collect::<String>())
    } else {
        line.to_string()
    }
}
