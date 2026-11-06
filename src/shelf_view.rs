//! Tab Shelf: menghubungkan [`Shelf`], riwayat clipboard, dan UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::clip_history::{Clip, History};
use crate::clipboard::Content;
use crate::clock::LocalTime;
use crate::image;
use crate::shelf::{display_name, Shelf};
use crate::notices::{glyph, Notice};
use crate::{on_ui, paths, platform, App, AppWindow, ClipItem, ShelfItem, TAB_SHELF};

/// Ukuran gambar kecil dalam piksel fisik; cukup tajam sampai skala 200%.
const FILE_THUMB: u32 = 80;
const CLIP_THUMB: u32 = 72;
/// Batas menunggu hasil Snipping Tool setelah tombol screenshot diklik.
const SCREENSHOT_WAIT: Duration = Duration::from_secs(120);

pub struct ShelfView {
    shelf: Shelf,
    items: Rc<VecModel<ShelfItem>>,
    /// Gambar kecil per file; hanya untuk file yang masih di Shelf.
    thumbs: HashMap<PathBuf, Image>,
    history: History,
    clips: Rc<VecModel<ClipItem>>,
    screenshot_since: Option<Instant>,
}

impl ShelfView {
    pub fn open(ui: &AppWindow) -> Self {
        let items = Rc::new(VecModel::default());
        ui.set_shelf_items(ModelRc::from(items.clone()));
        let clips = Rc::new(VecModel::default());
        ui.set_clips(ModelRc::from(clips.clone()));
        let mut view = Self {
            shelf: Shelf::load(paths::data_dir().join("shelf.json")),
            items,
            thumbs: HashMap::new(),
            history: History::default(),
            clips,
            screenshot_since: None,
        };
        view.refresh();
        view
    }

    /// Buang file yang sudah dipindah di luar PatPet, lalu gambar ulang daftar.
    pub fn refresh(&mut self) {
        let gone: Vec<PathBuf> = self.shelf.items().iter().filter(|p| !p.exists()).cloned().collect();
        for path in &gone {
            self.shelf.remove(path);
        }
        self.thumbs.retain(|p, _| self.shelf.items().contains(p));
        let rows: Vec<ShelfItem> = self
            .shelf
            .items()
            .iter()
            .map(|p| {
                let icon = self.thumbs.get(p);
                ShelfItem { name: display_name(p).into(), has_icon: icon.is_some(), icon: icon.cloned().unwrap_or_default() }
            })
            .collect();
        self.items.set_vec(rows);
        self.load_thumbnails();
    }

    /// Ambil gambar kecil yang belum ada di thread latar.
    fn load_thumbnails(&self) {
        let missing: Vec<PathBuf> = self.shelf.items().iter().filter(|p| !self.thumbs.contains_key(*p)).cloned().collect();
        if missing.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            for path in missing {
                let thumb = image::file_thumbnail(&path, FILE_THUMB);
                on_ui(move |app, _| app.shelf.thumbnail_ready(path, thumb));
            }
        });
    }

    fn thumbnail_ready(&mut self, path: PathBuf, thumb: Option<SharedPixelBuffer<Rgba8Pixel>>) {
        let Some(index) = self.shelf.items().iter().position(|p| *p == path) else { return };
        // Gagal pun dicatat (gambar kosong) supaya tidak dicoba terus-menerus.
        let image = thumb.map(Image::from_rgba8).unwrap_or_default();
        let has_icon = image.size().width > 0;
        self.thumbs.insert(path, image.clone());
        if let Some(mut row) = self.items.row_data(index) {
            row.icon = image;
            row.has_icon = has_icon;
            self.items.set_row_data(index, row);
        }
    }

    pub fn add(&mut self, paths: Vec<PathBuf>) -> usize {
        let added = paths.into_iter().filter(|p| p.exists()).filter(|p| self.shelf.add(p.clone())).count();
        if added > 0 {
            self.refresh();
        }
        added
    }

    pub fn remove(&mut self, index: i32) {
        if let Some(path) = self.path(index) {
            self.shelf.remove(&path);
            self.refresh();
        }
    }

    pub fn open_item(&self, index: i32) {
        if let Some(path) = self.path(index) {
            crate::listener::open(&path.to_string_lossy());
        }
    }

    pub fn path(&self, index: i32) -> Option<PathBuf> {
        self.shelf.items().get(usize::try_from(index).ok()?).cloned()
    }

    /// Setelah kartu diseret keluar: bila Explorer menerima atau filenya
    /// sudah pindah, keluarkan dari Shelf.
    pub fn after_drag(&mut self, path: &Path, accepted: bool) {
        if accepted || !path.exists() {
            self.shelf.remove(path);
        }
        self.refresh();
    }

    // ------------------------------------------------------------ clipboard

    /// Isi clipboard berubah: catat di riwayat. Gambar diolah di thread latar.
    pub fn clipboard_changed(&mut self) {
        match platform::clipboard_content() {
            Some(Content::Text(text)) => {
                self.history.push(Clip::Text(text));
                self.show_clips();
            }
            Some(Content::Png(png)) => {
                std::thread::spawn(move || {
                    if let Some(decoded) = image::decode(&png) {
                        let thumb = decoded.thumbnail(CLIP_THUMB);
                        on_ui(move |app, ui| app.image_copied(ui, png, thumb, decoded.width, decoded.height));
                    }
                });
            }
            Some(Content::Image(rgba)) => {
                std::thread::spawn(move || {
                    if let Some(png) = image::encode_png(&rgba) {
                        let thumb = rgba.thumbnail(CLIP_THUMB);
                        on_ui(move |app, ui| app.image_copied(ui, png, thumb, rgba.width, rgba.height));
                    }
                });
            }
            None => {}
        }
    }

    /// Gambar baru di clipboard. `true` bila itu hasil tombol screenshot
    /// dan sudah disimpan di Shelf.
    fn image_copied(&mut self, png: Vec<u8>, thumb: SharedPixelBuffer<Rgba8Pixel>, width: u32, height: u32) -> bool {
        let mut saved = false;
        if self.screenshot_since.take().is_some_and(|t| t.elapsed() < SCREENSHOT_WAIT) {
            if let Some(path) = save_png(&png, "Screenshot") {
                saved = self.add(vec![path]) > 0;
            }
        }
        self.history.push(Clip::Image { png, thumb, width, height });
        self.show_clips();
        saved
    }

    fn show_clips(&self) {
        let rows: Vec<ClipItem> = self
            .history
            .items()
            .iter()
            .map(|clip| match clip {
                Clip::Text(text) => ClipItem { text: preview(text).into(), is_image: false, image: Image::default() },
                Clip::Image { thumb, width, height, .. } => ClipItem {
                    text: format!("Gambar {width}x{height}").into(),
                    is_image: true,
                    image: Image::from_rgba8(thumb.clone()),
                },
            })
            .collect();
        self.clips.set_vec(rows);
    }

    /// Salin lagi ke clipboard dan pindahkan ke depan riwayat.
    pub fn copy(&mut self, index: i32) {
        let Some(clip) = usize::try_from(index).ok().and_then(|i| self.history.get(i)).cloned() else { return };
        match &clip {
            Clip::Text(text) => platform::set_clipboard_text(text),
            Clip::Image { png, .. } => {
                if let Some(rgba) = image::decode(png) {
                    platform::set_clipboard_image(&rgba, png);
                }
            }
        }
        self.history.push(clip);
        self.show_clips();
    }

    /// Simpan gambar dari riwayat sebagai file PNG lalu taruh di Shelf.
    pub fn keep(&mut self, index: i32) {
        let Some(Clip::Image { png, .. }) = usize::try_from(index).ok().and_then(|i| self.history.get(i)) else { return };
        if let Some(path) = save_png(png, "Gambar") {
            self.add(vec![path]);
        }
    }

    pub fn clear_clips(&mut self) {
        self.history.clear();
        self.show_clips();
    }

    /// Buka alat potong layar Windows; hasilnya masuk lewat clipboard.
    pub fn screenshot(&mut self) {
        self.screenshot_since = Some(Instant::now());
        crate::listener::open("ms-screenclip:");
    }
}

/// Satu baris pendek untuk kartu teks.
fn preview(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(160).collect()
}

/// Simpan PNG di folder Shelf PatPet dengan nama berwaktu.
fn save_png(png: &[u8], prefix: &str) -> Option<PathBuf> {
    let dir = paths::data_dir().join("Shelf");
    std::fs::create_dir_all(&dir).ok()?;
    let t = LocalTime::now();
    let stem = format!(
        "{prefix} {:04}-{:02}-{:02} {:02}.{:02}.{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    let path = (0..)
        .map(|n| dir.join(if n == 0 { format!("{stem}.png") } else { format!("{stem} ({n}).png") }))
        .find(|p| !p.exists())?;
    std::fs::write(&path, png).ok()?;
    Some(path)
}

impl App {
    fn image_copied(&mut self, ui: &AppWindow, png: Vec<u8>, thumb: SharedPixelBuffer<Rgba8Pixel>, width: u32, height: u32) {
        if self.shelf.image_copied(png, thumb, width, height) {
            let notice = Notice::message(glyph::CAMERA, "Screenshot disimpan", "Ada di Shelf, siap diseret ke folder mana pun")
                .with_tab(TAB_SHELF);
            self.notify(ui, notice);
        }
    }

    /// File diseret di atas island: buka tab Shelf supaya terlihat tempat jatuhnya.
    pub fn drag_hover(&mut self, ui: &AppWindow, on: bool) {
        ui.set_drop_hover(on);
        if on && !(ui.get_open() && ui.get_tab() == TAB_SHELF) {
            ui.set_tab(TAB_SHELF);
            ui.set_open(true); // tanpa mengaktifkan jendela; Explorer masih memegang seretan
            self.page_changed(ui);
        }
    }
}

/// Kartu Shelf mulai diseret. Seret OLE punya loop pesan sendiri, jadi
/// dijalankan setelah event Slint ini selesai dan tanpa meminjam status.
pub fn start_drag(index: i32) {
    let mut path = None;
    crate::with_app(|app, _| path = app.shelf.path(index));
    let Some(path) = path else { return };
    slint::Timer::single_shot(Duration::ZERO, move || {
        let accepted = platform::drag_out(&path);
        crate::with_app(|app, _| app.shelf.after_drag(&path, accepted));
    });
}
