//! Clipboard Windows: teks untuk kolom isian, serta teks dan gambar untuk
//! riwayat clipboard di tab Shelf.

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};

use crate::image::{bgra_to_rgba, Rgba};

/// Clipboard terbuka selama nilai ini hidup.
struct Open;

impl Open {
    fn new(owner: Option<HWND>) -> Option<Self> {
        // SAFETY: ditutup lagi di Drop.
        unsafe { OpenClipboard(owner).ok().map(|_| Self) }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: pasangan OpenClipboard yang berhasil.
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn set(owner: Option<HWND>, text: &str) {
    let Some(_open) = Open::new(owner) else { return };
    let bytes: Vec<u8> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    // SAFETY: clipboard sedang terbuka.
    let _ = unsafe { EmptyClipboard() };
    put(CF_UNICODETEXT.0.into(), &bytes);
}

/// Taruh gambar di clipboard: DIB untuk semua aplikasi, PNG untuk yang
/// mengerti transparansi.
pub fn set_image(owner: Option<HWND>, image: &Rgba, png: &[u8]) {
    let Some(_open) = Open::new(owner) else { return };
    // SAFETY: clipboard sedang terbuka.
    let _ = unsafe { EmptyClipboard() };
    put(CF_DIB.0.into(), &to_dib(image));
    put(format(w!("PNG")), png);
}

/// Salin `bytes` ke memori global lalu serahkan ke clipboard yang sedang terbuka.
fn put(format: u32, bytes: &[u8]) {
    // SAFETY: memori global seukuran data; setelah SetClipboardData berhasil
    // pemiliknya pindah ke sistem, bila gagal kita lepas sendiri.
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else { return };
        let target = GlobalLock(memory).cast::<u8>();
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        let _ = GlobalUnlock(memory);
        if SetClipboardData(format, Some(HANDLE(memory.0))).is_err() {
            let _ = GlobalFree(Some(memory));
        }
    }
}

/// Salinan isi satu format clipboard (clipboard harus sedang terbuka).
fn take(format: u32) -> Option<Vec<u8>> {
    // SAFETY: data milik sistem; hanya dibaca selama terkunci, sebatas GlobalSize.
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let memory = HGLOBAL(handle.0);
        let source = GlobalLock(memory).cast::<u8>();
        if source.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(source, GlobalSize(memory)).to_vec();
        let _ = GlobalUnlock(memory);
        Some(bytes)
    }
}

fn format(name: windows::core::PCWSTR) -> u32 {
    // SAFETY: nama berupa string konstan berakhiran nol.
    unsafe { RegisterClipboardFormatW(name) }
}

fn available(format: u32) -> bool {
    // SAFETY: tanpa prasyarat.
    unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// Isi clipboard yang dicatat di riwayat.
pub enum Content {
    Text(String),
    /// Sudah berupa PNG (mis. dari Snipping Tool atau browser).
    Png(Vec<u8>),
    Image(Rgba),
}

/// Baca isi clipboard untuk riwayat. `None` bila kosong, berasal dari PatPet
/// sendiri, atau ditandai rahasia oleh aplikasinya (mis. pengelola kata sandi).
pub fn read_for_history(owner: Option<HWND>) -> Option<Content> {
    // SAFETY: tanpa prasyarat.
    if owner.is_some() && unsafe { GetClipboardOwner() }.ok() == owner {
        return None;
    }
    let _open = Open::new(owner)?;
    if available(format(w!("ExcludeClipboardContentFromMonitorProcessing")))
        || available(format(w!("Clipboard Viewer Ignore")))
    {
        return None;
    }
    let history = format(w!("CanIncludeInClipboardHistory"));
    if available(history) && take(history).is_some_and(|b| b.get(..4) == Some(&[0, 0, 0, 0])) {
        return None;
    }

    if available(CF_UNICODETEXT.0.into()) {
        let bytes = take(CF_UNICODETEXT.0.into())?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let len = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        return Some(Content::Text(String::from_utf16_lossy(&units[..len])));
    }
    let png = format(w!("PNG"));
    if available(png) {
        return take(png).map(Content::Png);
    }
    if available(CF_DIB.0.into()) {
        return take(CF_DIB.0.into()).and_then(|dib| from_dib(&dib)).map(Content::Image);
    }
    None
}

/// DIB 24/32-bit (BITMAPINFOHEADER atau V5) menjadi RGBA.
fn from_dib(dib: &[u8]) -> Option<Rgba> {
    let u32_at = |i: usize| dib.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32;
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]);
    let compression = u32_at(16)?;
    if width <= 0 || height == 0 || !(bits == 24 || bits == 32) || !(compression == 0 || compression == 3) {
        return None;
    }
    // BITFIELDS dengan header lama: tiga mask warna setelah header.
    let offset = header + if compression == 3 && header == 40 { 12 } else { 0 };
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let stride = (w * bits as usize).div_ceil(32) * 4;
    let data = dib.get(offset..offset + stride * h)?;
    let mut bgra = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        // Tinggi positif berarti baris bawah lebih dulu.
        let source = if height > 0 { h - 1 - row } else { row };
        let line = &data[source * stride..];
        for x in 0..w {
            match bits {
                32 => bgra.extend_from_slice(&line[x * 4..x * 4 + 4]),
                _ => bgra.extend_from_slice(&[line[x * 3], line[x * 3 + 1], line[x * 3 + 2], 0]),
            }
        }
    }
    Some(Rgba { width: w as u32, height: h as u32, pixels: bgra_to_rgba(bgra, false) })
}

/// RGBA menjadi DIB 32-bit, baris bawah lebih dulu.
fn to_dib(image: &Rgba) -> Vec<u8> {
    let (w, h) = (image.width as usize, image.height as usize);
    let mut dib = Vec::with_capacity(40 + w * h * 4);
    for value in [40u32, image.width, image.height] {
        dib.extend_from_slice(&value.to_le_bytes());
    }
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&32u16.to_le_bytes()); // bit per piksel
    dib.extend_from_slice(&[0; 24]); // BI_RGB, ukuran, resolusi, palet
    for row in (0..h).rev() {
        for p in image.pixels[row * w * 4..(row + 1) * w * 4].chunks_exact(4) {
            dib.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    dib
}

pub fn get(owner: Option<HWND>) -> Option<String> {
    let _open = Open::new(owner)?;
    // SAFETY: data clipboard milik sistem; hanya dibaca selama terkunci dan
    // sampai penutup nol.
    unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT.0.into()).ok()?;
        let memory = HGLOBAL(handle.0);
        let source = GlobalLock(memory).cast::<u16>();
        if source.is_null() {
            return None;
        }
        let len = (0..).take_while(|&i| *source.add(i) != 0).count();
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(source, len));
        let _ = GlobalUnlock(memory);
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_round_trip_keeps_pixels_and_orientation() {
        // 2x2: baris atas merah-hijau, baris bawah biru-putih.
        let image = Rgba {
            width: 2,
            height: 2,
            pixels: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
        };
        let back = from_dib(&to_dib(&image)).unwrap();
        assert_eq!((back.width, back.height), (2, 2));
        assert_eq!(back.pixels, image.pixels);
    }

    #[test]
    fn dib_24_bit_rows_are_padded() {
        // 1x2, 24-bit: tiap baris 3 byte + 1 byte pengisi; baris bawah dulu.
        let mut dib = Vec::new();
        for v in [40u32, 1, 2] {
            dib.extend_from_slice(&v.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[255, 0, 0, 0, 0, 0, 255, 0]); // bawah biru, atas merah
        let image = from_dib(&dib).unwrap();
        assert_eq!(image.pixels, vec![255, 0, 0, 255, 0, 0, 255, 255]);
    }
}
