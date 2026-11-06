#![windows_subsystem = "windows"]

mod alarm;
mod audio;
mod background;
mod clip_history;
mod clipboard;
mod clock;
mod colors;
mod conf;
mod dragdrop;
mod http;
mod image;
mod listener;
mod lyrics;
mod media;
mod notes;
mod notices;
mod notes_view;
mod paths;
mod pet;
mod placement;
mod platform;
mod player;
mod preferences;
mod settings;
mod shelf;
mod shelf_view;
mod system;
mod timer;
mod tint;
mod tray;
mod weather;

use std::cell::RefCell;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use alarm::Alarm;
use background::{Request, Update};
use clock::LocalTime;
use media::Media;
use notes_view::NotesView;
use notices::{glyph, Action, Notice, Tone};
use pet::Pet;
use platform::Event;
use player::Player;
use settings::Settings;
use timer::Countdown;

slint::include_modules!();

const SLEEP_AFTER: Duration = Duration::from_secs(5 * 60);
const TICK: Duration = Duration::from_secs(2);
/// Seberapa sering posisi lagu dan baris lirik diperbarui saat tab Musik terbuka.
const MUSIC_TICK: Duration = Duration::from_millis(250);
/// Seberapa sering sisa timer diperbarui saat berjalan.
const TIMER_TICK: Duration = Duration::from_millis(250);
/// Klik ikon tray tepat setelah panel tertutup karena kehilangan fokus
/// maksudnya menutup, bukan membuka lagi.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

const TAB_MUSIC: i32 = 1;
/// Judul notifikasi alarm; dipakai untuk menutupnya saat alarm dimatikan.
const ALARM_TITLE: &str = "Waktu habis";
const TAB_TIMER: i32 = 2;
const TAB_NOTES: i32 = 3;
const TAB_SHELF: i32 = 4;
const MENU_TOGGLE: u32 = 1;
const MENU_QUIT: u32 = 2;
const MENU_TIMER: u32 = 3;
const MENU_SETTINGS: u32 = 4;

/// Seluruh status aplikasi. Hanya hidup di thread UI.
struct App {
    ui: slint::Weak<AppWindow>,
    pet: Pet,
    shown: Option<LocalTime>,
    power: system::Power,
    closed_at: Option<Instant>,
    worker: Sender<Request>,
    player: Player,
    notices: notices::Queue,
    notice_timer: slint::Timer,
    audio: Option<audio::Audio>,
    listener: Option<listener::Listener>,
    /// Punya identitas tapi belum diizinkan membaca notifikasi; dicek berkala.
    awaiting_access: bool,
    /// Petunjuk "Izinkan akses notifikasi" sudah ditampilkan sesi ini.
    access_hinted: bool,
    music_timer: slint::Timer,
    countdown: Countdown,
    countdown_timer: slint::Timer,
    alarm: Alarm,
    /// Bunyi alarm sungguhan (bukan pratinjau "Dengar bunyinya").
    ringing: bool,
    alarm_timer: slint::Timer,
    notes: NotesView,
    shelf: shelf_view::ShelfView,
    settings: Settings,
    config: preferences::ConfigFile,
    settings_timer: slint::Timer,
    drag: Option<preferences::Drag>,
    widths: placement::Widths,
    /// Disembunyikan karena ada aplikasi layar penuh.
    hidden: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Jalankan `f` dengan status aplikasi. Jangan dipanggil bersarang.
fn with_app(f: impl FnOnce(&mut App, &AppWindow)) {
    APP.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(app) = slot.as_mut() else { return };
        if let Some(ui) = app.ui.upgrade() {
            f(app, &ui);
        }
    });
}

/// Kirim kerja ke thread UI dari thread mana pun.
fn on_ui(f: impl FnOnce(&mut App, &AppWindow) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || with_app(f));
}

fn main() -> Result<(), slint::PlatformError> {
    platform::install()?;

    let ui = AppWindow::new()?;
    ui.show()?; // membuat jendela supaya skala layar sudah diketahui
    ui.set_device_scale(ui.window().scale_factor());
    ui.set_zone(clock::zone_label().into());

    let (config, settings, bad_lines) = preferences::ConfigFile::load();
    let city = settings.city.clone();

    let app = App {
        ui: ui.as_weak(),
        pet: Pet::new(SLEEP_AFTER),
        shown: None,
        power: system::Power::read(),
        closed_at: None,
        worker: background::spawn(city, |update| on_ui(move |app, ui| app.background_update(ui, update))),
        player: Player::new(&ui),
        notices: notices::Queue::default(),
        notice_timer: slint::Timer::default(),
        audio: None,
        listener: None,
        awaiting_access: false,
        access_hinted: false,
        music_timer: slint::Timer::default(),
        countdown: Countdown::new(),
        countdown_timer: slint::Timer::default(),
        alarm: Alarm::new(),
        ringing: false,
        alarm_timer: slint::Timer::default(),
        notes: NotesView::open(&ui),
        shelf: shelf_view::ShelfView::open(&ui),
        settings,
        config,
        settings_timer: slint::Timer::default(),
        drag: None,
        widths: placement::Widths::for_screen(platform::monitor_width(), 0.15, 0.24, 0.38),
        hidden: false,
    };
    show_power(&ui, app.power);
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    with_app(|app, ui| app.apply_settings(ui, bad_lines));
    with_app(|app, _| {
        app.audio = audio::Audio::start(
            |volume, muted| on_ui(move |app, ui| app.volume_changed(ui, volume, muted)),
            || on_ui(|app, _| app.audio.as_mut().map_or((), audio::Audio::attach_default)),
        );
    });
    with_app(App::start_listener);

    // Menghubungi layanan media Windows bisa memakan waktu; jangan tahan jendela.
    std::thread::spawn(|| {
        let media = Media::start(|update| on_ui(move |app, ui| app.media_update(ui, update)));
        on_ui(move |app, _| app.player.media = media);
    });

    ui.on_toggle(|| {
        with_app(|app, ui| {
            if app.drag_end() {
                // Kucing baru saja diseret, bukan diklik.
            } else if app.ringing {
                app.stop_alarm(ui); // klik island atau kucing mematikan alarm
            } else if let Some(action) = app.notices.current().filter(|n| !n.indicator).map(|n| n.action.clone()) {
                // Klik notifikasi: tutup, lalu jalankan aksinya bila ada.
                app.next_notice(ui);
                match action {
                    Some(Action::Tab(tab)) => {
                        ui.set_tab(tab);
                        app.set_open(ui, true);
                    }
                    Some(Action::Open(target)) => listener::open(&target),
                    None => {}
                }
            } else {
                app.set_open(ui, !ui.get_open());
            }
        })
    });
    ui.on_tab_changed(|| with_app(App::page_changed));
    ui.on_pet_clicked(|| {
        with_app(|app, ui| {
            if app.pet.click(Instant::now()) {
                show_pet(ui, &app.pet);
            }
        })
    });
    ui.on_pet_finished(|| {
        with_app(|app, ui| {
            if app.pet.one_shot_done(Instant::now()) {
                show_pet(ui, &app.pet);
            }
        })
    });
    ui.on_refresh_weather(|| with_app(|app, _| drop(app.worker.send(Request::RefreshWeather))));
    ui.on_play_pause(|| with_app(|app, _| app.player.media.as_ref().map_or((), Media::play_pause)));
    ui.on_next(|| with_app(|app, _| app.player.media.as_ref().map_or((), Media::next)));
    ui.on_previous(|| with_app(|app, _| app.player.media.as_ref().map_or((), Media::previous)));
    ui.on_seek(|f| with_app(|app, _| app.player.seek(f)));
    ui.on_seek_to(|s| with_app(|app, _| app.player.seek_to(s)));
    ui.on_timer_start(|secs| {
        with_app(|app, ui| {
            app.countdown.start(Duration::from_secs(secs.max(0) as u64), Instant::now());
            app.timer_changed(ui);
            app.save_soon(); // ingat durasi terakhir
        })
    });
    ui.on_timer_pause_resume(|| with_app(App::pause_resume_timer));
    ui.on_timer_stop(|| {
        with_app(|app, ui| {
            app.countdown.stop();
            app.timer_changed(ui);
        })
    });
    ui.on_stop_alarm(|| with_app(App::stop_alarm));
    ui.on_note_search_edited(|_| with_app(|app, ui| app.notes.refresh(ui)));
    ui.on_note_add(|| {
        with_app(|app, ui| {
            if app.notes.add(ui) && app.pet.cheer(Instant::now()) {
                show_pet(ui, &app.pet);
            }
        })
    });
    ui.on_note_toggle_done(|id| with_app(|app, ui| app.notes.toggle_done(ui, id.into())));
    ui.on_note_toggle_pin(|id| with_app(|app, ui| app.notes.toggle_pin(ui, id.into())));
    ui.on_note_remove(|id| with_app(|app, ui| app.notes.remove(ui, id.into())));
    ui.on_note_edited(|id, text| with_app(|app, _| app.notes.edited(id.into(), text.into())));
    ui.on_note_undo(|| with_app(|app, ui| app.notes.undo(ui)));
    ui.on_note_pick_deadline(|id| with_app(|app, ui| app.notes.pick(ui, id)));
    ui.on_picker_month_shift(|d| with_app(|app, ui| app.notes.picker_month_shift(ui, d)));
    ui.on_picker_pick_day(|d| with_app(|app, ui| app.notes.picker_pick_day(ui, d)));
    ui.on_picker_save(|| with_app(|app, ui| app.notes.picker_save(ui)));
    ui.on_picker_clear(|| with_app(|app, ui| app.notes.picker_clear(ui)));
    ui.on_picker_cancel(|| with_app(|app, ui| app.notes.picker_cancel(ui)));
    ui.on_shelf_open(|i| with_app(|app, _| app.shelf.open_item(i)));
    ui.on_shelf_remove(|i| with_app(|app, _| app.shelf.remove(i)));
    ui.on_shelf_drag(shelf_view::start_drag);
    ui.on_screenshot(|| with_app(|app, _| app.shelf.screenshot()));
    ui.on_clip_copy(|i| with_app(|app, _| app.shelf.copy(i)));
    ui.on_clip_keep(|i| with_app(|app, _| app.shelf.keep(i)));
    ui.on_clips_clear(|| with_app(|app, _| app.shelf.clear_clips()));
    ui.on_drag_start(|| with_app(|app, _| app.drag_start()));
    ui.on_drag_move(|| with_app(App::drag_move));
    ui.on_open_config(preferences::open_config);

    platform::on_event(handle_event);

    let clock = slint::Timer::default();
    clock.start(slint::TimerMode::Repeated, TICK, || with_app(App::tick));
    with_app(App::tick);

    let result = ui.run();
    // Simpan suntingan dan pengaturan terakhir, lepas handler SMTC dan thread latar sebelum keluar.
    with_app(|app, ui| {
        app.notes.flush();
        app.save_settings(ui);
    });
    APP.with(|cell| cell.borrow_mut().take());
    result
}

fn handle_event(event: Event) {
    match event {
        Event::TrayMenu => {
            // Menu punya loop pesan sendiri; status tidak boleh sedang dipinjam.
            let (mut panel, mut timer) = (String::new(), String::new());
            with_app(|app, ui| {
                panel = if ui.get_open() { "Tutup panel" } else { "Buka panel" }.into();
                timer = match app.countdown.state() {
                    timer::State::Running { .. } => "Jeda timer".into(),
                    timer::State::Paused { .. } => "Lanjutkan timer".into(),
                    timer::State::Idle => format!("Mulai timer ({})", timer::format(app.chosen_duration(ui))),
                };
            });
            let items = [
                Some((MENU_TOGGLE, panel.as_str())),
                Some((MENU_TIMER, timer.as_str())),
                Some((MENU_SETTINGS, "Pengaturan")),
                None,
                Some((MENU_QUIT, "Keluar")),
            ];
            match platform::show_menu(&items) {
                Some(MENU_TOGGLE) => with_app(|app, ui| app.set_open(ui, !ui.get_open())),
                Some(MENU_TIMER) => with_app(|app, ui| {
                    if app.countdown.is_active() {
                        app.pause_resume_timer(ui);
                    } else {
                        app.countdown.start(app.chosen_duration(ui), Instant::now());
                        app.timer_changed(ui);
                    }
                }),
                Some(MENU_SETTINGS) => preferences::open_config(),
                Some(MENU_QUIT) => {
                    let _ = slint::quit_event_loop();
                }
                _ => {}
            }
        }
        event => with_app(|app, ui| app.handle_event(ui, event)),
    }
}

impl App {
    fn tick(&mut self, ui: &AppWindow) {
        let now = LocalTime::now();
        if self.shown.is_none_or(|s| (s.hour, s.minute, s.day) != (now.hour, now.minute, now.day)) {
            ui.set_time(now.time_text().into());
            ui.set_date(now.date_text().into());
            ui.set_greeting(now.greeting().into());
            self.shown = Some(now);

            // Label deadline ("Hari ini", "Besok") dan notifikasi diperbarui tiap menit.
            if let Some(notice) = self.notes.check_due(ui) {
                self.notify(ui, notice);
            }
            if !self.notes_visible(ui) {
                self.notes.refresh(ui);
            }
        }
        if self.pet.tick(Instant::now(), platform::user_idle(), now.hour.into()) {
            show_pet(ui, &self.pet);
        }
        self.check_fullscreen();
        self.reload_config_if_changed(ui);
        if self.awaiting_access && listener::allowed() {
            self.start_listener(ui);
        }
    }

    fn handle_event(&mut self, ui: &AppWindow, event: Event) {
        match event {
            Event::TrayClick => {
                let recently_closed = self.closed_at.is_some_and(|t| t.elapsed() < REOPEN_GUARD);
                if ui.get_open() || !recently_closed {
                    self.set_open(ui, !ui.get_open());
                }
            }
            Event::Deactivated if ui.get_open() && self.settings.close_on_focus_loss => self.set_open(ui, false),
            Event::Escape if ui.get_open() => self.set_open(ui, false),
            Event::PowerChanged => {
                let now = system::Power::read();
                let before = std::mem::replace(&mut self.power, now);
                show_power(ui, now);
                if !now.has_battery {
                    return;
                }
                let level = Some(now.percent as f32 / 100.0);
                if now.plugged != before.plugged {
                    let title = if now.plugged { "Mengisi daya" } else { "Charger dicabut" };
                    self.notify(ui, Notice::indicator(glyph::BOLT, title, level));
                }
                // Peringatan sekali saat baterai turun melewati 20% dan 10%.
                if let Some(limit) = [10, 20].into_iter().find(|&l| !now.plugged && now.percent <= l && before.percent > l) {
                    let body = format!("Sisa {limit}%. Colokkan charger.");
                    self.notify(ui, Notice::alert(glyph::BATTERY_LOW, "Baterai lemah", body));
                }
            }
            Event::Brightness(percent) => {
                self.notify(ui, Notice::indicator(glyph::BRIGHTNESS, "Kecerahan", Some(percent as f32 / 100.0)));
            }
            Event::BatterySaver(on) => {
                let title = if on { "Hemat baterai nyala" } else { "Hemat baterai mati" };
                self.notify(ui, Notice::indicator(glyph::LEAF, title, None));
            }
            Event::CapsLock(on) => {
                let title = if on { "Caps Lock nyala" } else { "Caps Lock mati" };
                self.notify(ui, Notice::indicator(glyph::KEYBOARD, title, None));
            }
            Event::ScaleChanged(scale) => {
                ui.set_device_scale(scale);
                self.update_widths(ui);
            }
            Event::DisplayChanged => self.update_widths(ui),
            Event::ClipboardChanged => self.shelf.clipboard_changed(),
            Event::DragHover(on) => self.drag_hover(ui, on),
            Event::FilesDropped(paths) => {
                self.shelf.add(paths);
            }
            _ => {}
        }
    }

    fn set_open(&mut self, ui: &AppWindow, open: bool) {
        ui.set_open(open);
        if open {
            platform::activate(); // supaya klik di luar menutup panel
        } else {
            self.closed_at = Some(Instant::now());
            self.notes.flush();
        }
        self.page_changed(ui);
    }

    fn notes_visible(&self, ui: &AppWindow) -> bool {
        ui.get_open() && ui.get_tab() == TAB_NOTES
    }

    /// Panel dibuka/ditutup atau tab berganti.
    fn page_changed(&mut self, ui: &AppWindow) {
        if self.notes_visible(ui) {
            self.notes.refresh(ui);
        } else {
            self.notes.flush();
        }
        if ui.get_open() && ui.get_tab() == TAB_SHELF {
            self.shelf.refresh(); // file yang dipindah di luar PatPet hilang dari daftar
        }
        self.sync_music_timer(ui);
    }

    /// Timer posisi lagu hanya jalan bila tab Musik terlihat dan lagu diputar.
    fn sync_music_timer(&mut self, ui: &AppWindow) {
        let needed = ui.get_open() && ui.get_tab() == TAB_MUSIC && self.player.playing();
        if needed == self.music_timer.running() {
            return;
        }
        if needed {
            self.player.refresh(ui);
            self.music_timer.start(slint::TimerMode::Repeated, MUSIC_TICK, || {
                with_app(|app, ui| app.player.refresh(ui));
            });
        } else {
            self.music_timer.stop();
        }
    }

    /// Durasi yang sedang diatur di tab Timer.
    fn chosen_duration(&self, ui: &AppWindow) -> Duration {
        let secs = ui.get_timer_hours() * 3600 + ui.get_timer_minutes() * 60 + ui.get_timer_seconds();
        Duration::from_secs(secs.max(0) as u64)
    }

    fn pause_resume_timer(&mut self, ui: &AppWindow) {
        let now = Instant::now();
        if self.countdown.is_running() {
            self.countdown.pause(now);
        } else {
            self.countdown.resume(now);
        }
        self.timer_changed(ui);
    }

    /// Setelah status timer berubah: perbarui UI, kucing, dan detak timer.
    fn timer_changed(&mut self, ui: &AppWindow) {
        self.show_timer(ui);
        if self.pet.set_focus(self.countdown.is_running(), false, Instant::now()) {
            show_pet(ui, &self.pet);
        }
        if !self.countdown.is_running() {
            self.countdown_timer.stop();
        } else if !self.countdown_timer.running() {
            self.countdown_timer.start(slint::TimerMode::Repeated, TIMER_TICK, || {
                with_app(App::countdown_tick);
            });
        }
    }

    fn countdown_tick(&mut self, ui: &AppWindow) {
        if self.countdown.finished(Instant::now()) {
            self.timer_finished(ui);
        } else {
            self.show_timer(ui);
        }
    }

    fn timer_finished(&mut self, ui: &AppWindow) {
        self.countdown_timer.stop();
        self.show_timer(ui);
        if self.pet.set_focus(false, true, Instant::now()) {
            show_pet(ui, &self.pet);
        }

        let total = timer::format(self.countdown.total());
        if ui.get_alarm_enabled() {
            self.alarm.start();
            self.ringing = true;
            ui.set_ringing(true);
            let ring_for = Duration::from_secs(self.settings.alarm_seconds as u64);
            self.alarm_timer.start(slint::TimerMode::SingleShot, ring_for, || {
                with_app(App::stop_alarm);
            });
            let body = format!("Timer {total} selesai. Klik untuk mematikan alarm.");
            self.notify(ui, Notice::alert(glyph::ALARM, ALARM_TITLE, body).sticky().with_tab(TAB_TIMER));
        } else {
            let body = format!("Timer {total} selesai");
            self.notify(ui, Notice::alert(glyph::ALARM, ALARM_TITLE, body).with_tab(TAB_TIMER));
        }
    }

    fn stop_alarm(&mut self, ui: &AppWindow) {
        self.alarm.stop();
        self.alarm_timer.stop();
        self.ringing = false;
        ui.set_ringing(false);
        if self.notices.remove_sticky(ALARM_TITLE) {
            self.show_notice(ui);
        }
    }

    // ---------------------------------------------------------- notifikasi

    pub fn notify(&mut self, ui: &AppWindow, notice: Notice) {
        if self.notices.push(notice) {
            self.show_notice(ui);
        }
    }

    fn next_notice(&mut self, ui: &AppWindow) {
        self.notices.next();
        self.show_notice(ui);
    }

    /// Tampilkan notifikasi terdepan dan atur kapan ia ditutup.
    fn show_notice(&mut self, ui: &AppWindow) {
        let Some(n) = self.notices.current() else {
            self.notice_timer.stop();
            ui.set_notice(NoticeView::default());
            return;
        };
        ui.set_notice(NoticeView {
            active: true,
            has_image: n.image.is_some(),
            image: n.image.clone().map(slint::Image::from_rgba8).unwrap_or_default(),
            glyph: n.glyph.into(),
            title: n.title.as_str().into(),
            body: n.body.as_str().into(),
            alert: n.tone == Tone::Alert,
            level: n.level.unwrap_or(-1.0),
        });
        match n.duration {
            Some(d) => self.notice_timer.start(slint::TimerMode::SingleShot, d, || with_app(App::next_notice)),
            None => self.notice_timer.stop(),
        }
    }

    /// Dengarkan notifikasi aplikasi lain bila PatPet punya identitas dan izin.
    fn start_listener(&mut self, ui: &AppWindow) {
        let access = listener::start(
            |n| {
                let target = format!("shell:AppsFolder\\{}", n.app_id);
                on_ui(move |app, ui| {
                    let notice = Notice::external(n.title, n.body, n.logo);
                    let notice = if n.app_id.is_empty() { notice } else { notice.with_open(target) };
                    app.notify(ui, notice);
                })
            },
            || on_ui(App::start_listener),
        );
        self.awaiting_access = matches!(access, listener::Access::Asking | listener::Access::Denied);
        match access {
            listener::Access::Listening(l) => self.listener = Some(l),
            listener::Access::Denied if !self.access_hinted => {
                self.access_hinted = true;
                let notice = Notice::message(glyph::BELL, "Izinkan akses notifikasi", "Klik untuk membuka Pengaturan Windows")
                    .with_open("ms-settings:privacy-notifications");
                self.notify(ui, notice);
            }
            _ => {}
        }
    }

    fn volume_changed(&mut self, ui: &AppWindow, volume: f32, muted: bool) {
        let notice = if muted {
            Notice::indicator(glyph::MUTE, "Suara dimatikan", None)
        } else {
            Notice::indicator(glyph::VOLUME, "Volume", Some(volume))
        };
        self.notify(ui, notice);
    }

    fn show_timer(&self, ui: &AppWindow) {
        let now = Instant::now();
        let c = &self.countdown;
        let remaining = c.remaining(now);
        let (state, status) = match c.state() {
            timer::State::Idle => (TimerState::Idle, String::new()),
            timer::State::Running { .. } => {
                let end = LocalTime::now().time_after(remaining.as_secs_f64().ceil() as u64);
                (TimerState::Running, format!("Selesai pukul {end}"))
            }
            timer::State::Paused { .. } => (TimerState::Paused, "Dijeda".into()),
        };
        ui.set_timer(TimerView {
            state,
            remaining: timer::format(remaining).into(),
            progress: c.progress(now),
            status: status.into(),
            total: timer::format(c.total()).into(),
        });
    }

    fn background_update(&mut self, ui: &AppWindow, update: Update) {
        match update {
            Update::Temperature(c) => ui.set_cpu_temp(c.unwrap_or(0)),
            Update::Weather(w) => ui.set_weather(Weather {
                ready: w.ready,
                busy: w.busy,
                temperature: w.temperature.into(),
                summary: w.summary.into(),
                place: w.place.into(),
            }),
            Update::Lyrics { key, result } => self.player.set_lyrics(ui, &key, result),
        }
    }

    fn media_update(&mut self, ui: &AppWindow, update: media::Update) {
        match update {
            media::Update::Track(track) => {
                if let Some(request) = self.player.set_track(ui, track) {
                    let _ = self.worker.send(request);
                }
                if self.pet.set_music(self.player.playing(), Instant::now()) {
                    show_pet(ui, &self.pet);
                }
                self.sync_music_timer(ui);
            }
            media::Update::Cover(title, cover) => self.player.set_cover(ui, &title, cover),
        }
    }
}

fn show_power(ui: &AppWindow, p: system::Power) {
    ui.set_power(Power {
        has_battery: p.has_battery,
        percent: p.percent.into(),
        plugged: p.plugged,
    });
}

fn show_pet(ui: &AppWindow, pet: &Pet) {
    ui.set_mood(match pet.mood() {
        pet::Mood::Idle => Mood::Idle,
        pet::Mood::Music => Mood::Music,
        pet::Mood::Sleep => Mood::Sleep,
        pet::Mood::Focus => Mood::Focus,
        pet::Mood::Happy => Mood::Happy,
        pet::Mood::Yawn => Mood::Yawn,
    });
    ui.set_generation(pet.generation() as i32);
}
