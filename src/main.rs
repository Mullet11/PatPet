#![windows_subsystem = "windows"]

mod clock;
mod pet;
mod platform;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use clock::LocalTime;
use pet::Pet;

slint::include_modules!();

const SLEEP_AFTER: Duration = Duration::from_secs(5 * 60);
const TICK: Duration = Duration::from_secs(2);

struct App {
    pet: Pet,
    shown: Option<LocalTime>,
}

fn main() -> Result<(), slint::PlatformError> {
    platform::install()?;

    let ui = AppWindow::new()?;
    let app = Rc::new(RefCell::new(App { pet: Pet::new(SLEEP_AFTER), shown: None }));

    ui.on_pet_clicked({
        let (ui, app) = (ui.as_weak(), app.clone());
        move || {
            let mut app = app.borrow_mut();
            if app.pet.click(Instant::now()) {
                show_pet(&ui.unwrap(), &app.pet);
            }
        }
    });

    ui.on_pet_finished({
        let (ui, app) = (ui.as_weak(), app.clone());
        move || {
            let mut app = app.borrow_mut();
            if app.pet.one_shot_done(Instant::now()) {
                show_pet(&ui.unwrap(), &app.pet);
            }
        }
    });

    ui.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    let tick = {
        let (ui, app) = (ui.as_weak(), app.clone());
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let mut app = app.borrow_mut();
            let now = LocalTime::now();

            if app.shown.map_or(true, |s| (s.hour, s.minute, s.day) != (now.hour, now.minute, now.day)) {
                ui.set_time(now.time_text().into());
                ui.set_date(now.date_text().into());
                app.shown = Some(now);
            }

            if app.pet.tick(Instant::now(), platform::user_idle(), now.hour.into()) {
                show_pet(&ui, &app.pet);
            }
        }
    };
    tick();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, TICK, tick);

    ui.run()
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
