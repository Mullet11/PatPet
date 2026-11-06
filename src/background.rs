//! Satu thread latar untuk kerja yang lambat atau memblokir: suhu laptop,
//! lokasi, cuaca, dan lirik. Hasilnya dikirim ke UI lewat `post`.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::lyrics::{self, Fetch};
use crate::system::Thermal;
use crate::weather::{self, Position};

const TEMPERATURE_EVERY: Duration = Duration::from_secs(10);
const WEATHER_EVERY: Duration = Duration::from_secs(30 * 60);
const LOCATION_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const NO_ACCESS_RETRY: Duration = Duration::from_secs(10 * 60);
const RETRY: [Duration; 3] = [Duration::from_secs(30), Duration::from_secs(120), Duration::from_secs(600)];

pub enum Request {
    RefreshWeather,
    /// `None` = pakai lokasi Windows; `Some` = nama kota dari patpet.conf.
    SetLocation(Option<String>),
    Lyrics { key: String, title: String, artist: String, album: String, duration: Duration },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeatherView {
    pub ready: bool,
    pub busy: bool,
    pub temperature: String,
    pub summary: String,
    /// Nama daerah, atau pesan status bila `ready` false.
    pub place: String,
}

impl WeatherView {
    fn status(message: &str, busy: bool) -> Self {
        Self { busy, place: message.into(), ..Default::default() }
    }
}

pub enum Update {
    /// Suhu laptop dalam °C; `None` bila sensor tidak tersedia.
    Temperature(Option<i32>),
    Weather(WeatherView),
    /// Hasil lirik untuk lagu `key`; abaikan bila lagu sudah berganti.
    Lyrics { key: String, result: Fetch },
}

/// `city`: nama kota pilihan pengguna; `None` = pakai lokasi Windows.
pub fn spawn(city: Option<String>, post: impl Fn(Update) + Send + 'static) -> Sender<Request> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("latar".into())
        .spawn(move || {
            let mut worker = Worker::new(post);
            worker.set_city(city);
            worker.run(rx)
        })
        .expect("gagal membuat thread latar");
    tx
}

struct Place {
    lat: f64,
    lon: f64,
    name: String,
    found_at: Instant,
}

struct Worker<F> {
    post: F,
    thermal: Option<Thermal>,
    temperature: Option<i32>,
    place: Option<Place>,
    /// Nama kota dari pengaturan; lokasi Windows tidak dipakai.
    city: Option<String>,
    shown: WeatherView,
    failures: usize,
    next_temperature: Instant,
    next_weather: Instant,
}

impl<F: Fn(Update)> Worker<F> {
    fn new(post: F) -> Self {
        let now = Instant::now();
        Self {
            post,
            thermal: Thermal::open(),
            temperature: None,
            place: None,
            city: None,
            shown: WeatherView::default(),
            failures: 0,
            next_temperature: now,
            next_weather: now,
        }
    }

    fn set_city(&mut self, city: Option<String>) {
        self.city = city;
        self.place = None;
        self.shown = WeatherView::default();
        self.failures = 0;
    }

    fn run(mut self, requests: mpsc::Receiver<Request>) {
        let mut force_location = false;
        loop {
            let now = Instant::now();
            if now >= self.next_temperature {
                self.read_temperature();
                self.next_temperature = now + TEMPERATURE_EVERY;
            }
            if now >= self.next_weather {
                self.next_weather = now + self.refresh_weather(force_location);
                force_location = false;
            }

            let wake = self.next_temperature.min(self.next_weather);
            match requests.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Request::RefreshWeather) => {
                    force_location = true;
                    self.next_weather = Instant::now();
                }
                Ok(Request::SetLocation(city)) => {
                    if city != self.city {
                        self.set_city(city);
                        self.next_weather = Instant::now();
                    }
                }
                Ok(Request::Lyrics { key, title, artist, album, duration }) => {
                    let result = lyrics::fetch(&title, &artist, &album, duration);
                    (self.post)(Update::Lyrics { key, result });
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn read_temperature(&mut self) {
        let value = self.thermal.as_mut().and_then(Thermal::read_celsius);
        if value != self.temperature {
            self.temperature = value;
            (self.post)(Update::Temperature(value));
        }
    }

    fn show(&mut self, view: WeatherView) {
        if view != self.shown {
            self.shown = view.clone();
            (self.post)(Update::Weather(view));
        }
    }

    /// Mengembalikan jeda sampai percobaan berikutnya.
    fn refresh_weather(&mut self, force_location: bool) -> Duration {
        let mut busy = self.shown.clone();
        busy.busy = true;
        if !busy.ready && self.place.is_none() {
            busy.place = "Mencari lokasi…".into();
        }
        self.show(busy);

        if let (Some(name), None) = (&self.city, &self.place) {
            // Kota dari pengaturan: cari koordinatnya sekali.
            match weather::search_city(name) {
                Some(found) => match found.into_iter().next() {
                    Some(c) => self.place = Some(Place { lat: c.lat, lon: c.lon, name: c.name, found_at: Instant::now() }),
                    None => {
                        let text = format!("Kota \"{name}\" tidak ditemukan. Periksa lokasi di patpet.conf");
                        self.show(WeatherView::status(&text, false));
                        return NO_ACCESS_RETRY;
                    }
                },
                None => {
                    self.show(WeatherView::status("Cuaca gagal dimuat. Periksa koneksi internet", false));
                    return self.retry();
                }
            }
        }

        let stale = self.place.as_ref().is_none_or(|p| p.found_at.elapsed() >= LOCATION_EVERY);
        if self.city.is_none() && (force_location || stale) {
            match weather::device_position() {
                Position::Found { lat, lon } => {
                    let old = self.place.take().map(|p| p.name).unwrap_or_default();
                    let name = weather::place_name(lat, lon).unwrap_or(old);
                    self.place = Some(Place { lat, lon, name, found_at: Instant::now() });
                }
                Position::Denied => {
                    self.place = None;
                    self.show(WeatherView::status("Nyalakan akses lokasi Windows untuk melihat cuaca", false));
                    return NO_ACCESS_RETRY;
                }
                // Lokasi lama tetap dipakai bila ada.
                Position::Unavailable if self.place.is_some() => {}
                Position::Unavailable => {
                    self.show(WeatherView::status("Lokasi belum ditemukan. Coba perbarui lagi nanti", false));
                    return self.retry();
                }
            }
        }

        let Some((lat, lon, name)) = self.place.as_ref().map(|p| (p.lat, p.lon, p.name.clone())) else {
            return self.retry();
        };
        if !self.shown.ready {
            self.show(WeatherView::status("Memuat cuaca…", true));
        }
        match weather::current(lat, lon) {
            Some((celsius, code)) => {
                self.failures = 0;
                let view = WeatherView {
                    ready: true,
                    busy: false,
                    temperature: format!("{}°C", celsius.round() as i32),
                    summary: weather::describe(code).into(),
                    place: name,
                };
                self.show(view);
                WEATHER_EVERY
            }
            None => {
                let view = if self.shown.ready {
                    WeatherView { busy: false, ..self.shown.clone() } // tetap tampilkan data lama
                } else {
                    WeatherView::status("Cuaca gagal dimuat. Periksa koneksi internet", false)
                };
                self.show(view);
                self.retry()
            }
        }
    }

    fn retry(&mut self) -> Duration {
        let wait = RETRY[self.failures.min(RETRY.len() - 1)];
        self.failures += 1;
        wait
    }
}
