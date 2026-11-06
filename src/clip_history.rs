//! Riwayat clipboard, hanya di memori dan hilang saat PatPet ditutup.
//! Gambar disimpan sebagai PNG terkompresi supaya memori tetap kecil.

use std::collections::VecDeque;

use slint::{Rgba8Pixel, SharedPixelBuffer};

pub const MAX_ITEMS: usize = 20;
pub const MAX_IMAGES: usize = 8;
pub const MAX_IMAGE_BYTES: usize = 24 * 1024 * 1024;
/// Teks yang sangat panjang dipotong.
pub const MAX_TEXT_CHARS: usize = 20_000;

#[derive(Clone)]
pub enum Clip {
    Text(String),
    Image {
        png: Vec<u8>,
        thumb: SharedPixelBuffer<Rgba8Pixel>,
        width: u32,
        height: u32,
    },
}

impl Clip {
    fn image_bytes(&self) -> usize {
        match self {
            Clip::Image { png, .. } => png.len(),
            Clip::Text(_) => 0,
        }
    }

    fn same_as(&self, other: &Clip) -> bool {
        match (self, other) {
            (Clip::Text(a), Clip::Text(b)) => a == b,
            (Clip::Image { png: a, .. }, Clip::Image { png: b, .. }) => a == b,
            _ => false,
        }
    }
}

#[derive(Default)]
pub struct History {
    items: VecDeque<Clip>,
}

impl History {
    pub fn items(&self) -> &VecDeque<Clip> {
        &self.items
    }

    pub fn get(&self, index: usize) -> Option<&Clip> {
        self.items.get(index)
    }

    /// Tambahkan di depan. Isi yang sama dipindah ke depan, bukan didobel.
    pub fn push(&mut self, clip: Clip) {
        let clip = match clip {
            Clip::Text(t) if t.trim().is_empty() => return,
            Clip::Text(t) => Clip::Text(t.chars().take(MAX_TEXT_CHARS).collect()),
            image => image,
        };
        self.items.retain(|c| !c.same_as(&clip));
        self.items.push_front(clip);
        self.trim();
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Buang yang paling lama sampai semua batas terpenuhi.
    fn trim(&mut self) {
        self.items.truncate(MAX_ITEMS);
        loop {
            let images = self.items.iter().filter(|c| c.image_bytes() > 0).count();
            let bytes: usize = self.items.iter().map(Clip::image_bytes).sum();
            if images <= MAX_IMAGES && bytes <= MAX_IMAGE_BYTES {
                break;
            }
            match self.items.iter().rposition(|c| c.image_bytes() > 0) {
                Some(oldest) => {
                    self.items.remove(oldest);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: usize, tag: u8) -> Clip {
        Clip::Image { png: vec![tag; bytes], thumb: SharedPixelBuffer::new(1, 1), width: 1, height: 1 }
    }

    fn texts(h: &History) -> Vec<String> {
        h.items().iter().filter_map(|c| if let Clip::Text(t) = c { Some(t.clone()) } else { None }).collect()
    }

    #[test]
    fn newest_first_without_duplicates() {
        let mut h = History::default();
        h.push(Clip::Text("a".into()));
        h.push(Clip::Text("b".into()));
        h.push(Clip::Text("a".into()));
        h.push(Clip::Text("   ".into()));
        assert_eq!(texts(&h), vec!["a", "b"]);
    }

    #[test]
    fn keeps_at_most_twenty() {
        let mut h = History::default();
        for i in 0..30 {
            h.push(Clip::Text(i.to_string()));
        }
        assert_eq!(h.items().len(), MAX_ITEMS);
        assert_eq!(texts(&h)[0], "29");
    }

    #[test]
    fn image_limits_drop_the_oldest_images_only() {
        let mut h = History::default();
        h.push(Clip::Text("teks lama".into()));
        for i in 0..10 {
            h.push(image(10, i));
        }
        assert_eq!(h.items().iter().filter(|c| c.image_bytes() > 0).count(), MAX_IMAGES);
        assert_eq!(texts(&h), vec!["teks lama"]);

        let mut big = History::default();
        big.push(image(MAX_IMAGE_BYTES / 2 + 1, 1));
        big.push(image(MAX_IMAGE_BYTES / 2 + 1, 2));
        assert_eq!(big.items().len(), 1);
        assert!(matches!(&big.items()[0], Clip::Image { png, .. } if png[0] == 2));
    }
}
