//! Dekode gambar (cover lagu, logo aplikasi) lewat WIC bawaan Windows, jadi
//! exe tidak perlu dekoder PNG/JPG sendiri. Gambar langsung dikecilkan supaya
//! memori yang dipakai kecil.

use slint::{Rgba8Pixel, SharedPixelBuffer};
use windows::core::Interface;
use windows::Storage::Streams::{IRandomAccessStream, IRandomAccessStreamReference};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand, WICRect,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IStream, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::WinRT::CreateStreamOverRandomAccessStream;

/// COM aktif selama nilai ini hidup pada thread pemanggil.
struct ComScope {
    /// `false` bila COM sudah aktif dengan mode lain (mis. thread UI yang STA);
    /// tetap bisa dipakai, tapi bukan milik kita untuk ditutup.
    owned: bool,
}

impl ComScope {
    fn enter() -> Option<Self> {
        Self::init(COINIT_MULTITHREADED)
    }

    /// Objek Shell (gambar kecil file) butuh apartemen STA.
    fn enter_sta() -> Option<Self> {
        Self::init(COINIT_APARTMENTTHREADED)
    }

    fn init(mode: windows::Win32::System::Com::COINIT) -> Option<Self> {
        // SAFETY: setiap sukses (termasuk S_FALSE) diimbangi CoUninitialize di Drop.
        let hr = unsafe { CoInitializeEx(None, mode) };
        if hr == windows::Win32::Foundation::RPC_E_CHANGED_MODE {
            return Some(Self { owned: false });
        }
        hr.ok().ok().map(|_| Self { owned: true })
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: pasangan CoInitializeEx yang sukses di `init`.
            unsafe { CoUninitialize() };
        }
    }
}

/// Buka gambar dari WinRT, potong tengah menjadi persegi, kecilkan ke `size`
/// piksel, lalu ubah ke RGBA. Memblokir; panggil dari thread latar.
pub fn decode_square(source: &IRandomAccessStreamReference, size: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let _com = ComScope::enter()?;
    let stream = source.OpenReadAsync().ok()?.get().ok()?.cast::<IRandomAccessStream>().ok()?;
    // SAFETY: semua objek COM dilepas otomatis (windows-rs); buffer tujuan
    // berukuran tepat size * size * 4 byte.
    unsafe {
        let stream: IStream = CreateStreamOverRandomAccessStream(&stream).ok()?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let frame = wic
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .ok()?
            .GetFrame(0)
            .ok()?;

        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let side = w.min(h);
        if side == 0 {
            return None;
        }
        let clip = wic.CreateBitmapClipper().ok()?;
        let rect = WICRect { X: ((w - side) / 2) as i32, Y: ((h - side) / 2) as i32, Width: side as i32, Height: side as i32 };
        clip.Initialize(&frame, &rect).ok()?;

        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler.Initialize(&clip, size, size, WICBitmapInterpolationModeFant).ok()?;

        let rgba = wic.CreateFormatConverter().ok()?;
        rgba.Initialize(&scaler, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
            .ok()?;

        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(size, size);
        rgba.CopyPixels(std::ptr::null(), size * 4, buffer.make_mut_bytes()).ok()?;
        Some(buffer)
    }
}

// ------------------------------------------------------------------ RGBA

/// Gambar RGBA (tidak premultiplied), baris atas lebih dulu.
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    /// Gambar kecil persegi untuk kartu: potong tengah lalu kecilkan (rata-rata kotak).
    pub fn thumbnail(&self, size: u32) -> SharedPixelBuffer<Rgba8Pixel> {
        let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(size, size);
        let side = self.width.min(self.height).max(1);
        let (x0, y0) = ((self.width - side.min(self.width)) / 2, (self.height - side.min(self.height)) / 2);
        let dst = out.make_mut_slice();
        for ty in 0..size {
            for tx in 0..size {
                let (sx0, sx1) = (x0 + tx * side / size, x0 + ((tx + 1) * side / size).max(tx * side / size + 1));
                let (sy0, sy1) = (y0 + ty * side / size, y0 + ((ty + 1) * side / size).max(ty * side / size + 1));
                let mut acc = [0u32; 4];
                let mut n = 0u32;
                for y in sy0..sy1.min(self.height) {
                    for x in sx0..sx1.min(self.width) {
                        let i = ((y * self.width + x) * 4) as usize;
                        for c in 0..4 {
                            acc[c] += self.pixels[i + c] as u32;
                        }
                        n += 1;
                    }
                }
                let n = n.max(1);
                dst[(ty * size + tx) as usize] = Rgba8Pixel {
                    r: (acc[0] / n) as u8,
                    g: (acc[1] / n) as u8,
                    b: (acc[2] / n) as u8,
                    a: (acc[3] / n) as u8,
                };
            }
        }
        out
    }
}

/// Ubah BGRA (dari GDI/clipboard) menjadi RGBA. Bila semua alpha nol
/// (gambar tanpa saluran alpha), anggap buram penuh.
pub fn bgra_to_rgba(mut pixels: Vec<u8>, premultiplied: bool) -> Vec<u8> {
    let opaque = pixels.chunks_exact(4).all(|p| p[3] == 0);
    for p in pixels.chunks_exact_mut(4) {
        p.swap(0, 2);
        if opaque {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            for c in 0..3 {
                p[c] = ((p[c] as u32 * 255) / p[3] as u32).min(255) as u8;
            }
        }
    }
    pixels
}

/// Gambar kecil sebuah file seperti di Explorer: pratinjau untuk gambar,
/// ikon untuk file lain. Memblokir; panggil dari thread latar.
pub fn file_thumbnail(path: &std::path::Path, size: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT};

    let _com = ComScope::enter_sta()?;
    // SAFETY: bitmap dilepas setelah piksel disalin; buffer seukuran size*size*4.
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        let bitmap = factory.GetImage(SIZE { cx: size as i32, cy: size as i32 }, SIIGBF_RESIZETOFIT).ok()?;
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (size * size * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(dc, bitmap, 0, size, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(bitmap.into());
        if lines == 0 {
            return None;
        }
        let rgba = bgra_to_rgba(bgra, true);
        Some(SharedPixelBuffer::clone_from_slice(&rgba, size, size))
    }
}

/// Simpan RGBA sebagai PNG.
pub fn encode_png(image: &Rgba) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Imaging::{GUID_ContainerFormatPng, WICBitmapEncoderNoCache};
    use windows::Win32::System::Com::STREAM_SEEK_SET;
    use windows::Win32::UI::Shell::SHCreateMemStream;

    let _com = ComScope::enter()?;
    // SAFETY: objek COM dilepas otomatis; buffer sumber hidup selama panggilan.
    unsafe {
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let source = wic
            .CreateBitmapFromMemory(image.width, image.height, &GUID_WICPixelFormat32bppRGBA, image.width * 4, &image.pixels)
            .ok()?;
        let stream = SHCreateMemStream(None)?;
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).ok()?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
        let frame = frame?;
        frame.Initialize(None).ok()?;
        frame.SetSize(image.width, image.height).ok()?;
        frame.WriteSource(&source, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        encoder.Commit().ok()?;

        let mut end = 0u64;
        stream.Seek(0, windows::Win32::System::Com::STREAM_SEEK_END, Some(&mut end)).ok()?;
        stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut png = vec![0u8; end as usize];
        let mut read = 0u32;
        stream.Read(png.as_mut_ptr().cast(), end as u32, Some(&mut read)).ok().ok()?;
        png.truncate(read as usize);
        Some(png)
    }
}

/// Buka PNG (atau format lain yang dikenal WIC) menjadi RGBA ukuran penuh.
pub fn decode(bytes: &[u8]) -> Option<Rgba> {
    use windows::Win32::UI::Shell::SHCreateMemStream;

    let _com = ComScope::enter()?;
    // SAFETY: objek COM dilepas otomatis; buffer tujuan seukuran gambar.
    unsafe {
        let stream = SHCreateMemStream(Some(bytes))?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let frame = wic
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .ok()?
            .GetFrame(0)
            .ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let rgba = wic.CreateFormatConverter().ok()?;
        rgba.Initialize(&frame, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
            .ok()?;
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        rgba.CopyPixels(std::ptr::null(), w * 4, &mut pixels).ok()?;
        Some(Rgba { width: w, height: h, pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_averages_a_centered_square() {
        // 4x2: kiri merah, kanan biru; potong tengah 2x2 = kolom 1–2.
        let mut pixels = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                pixels.extend_from_slice(if x < 2 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
            }
        }
        let t = Rgba { width: 4, height: 2, pixels }.thumbnail(1);
        let p = t.as_slice()[0];
        assert_eq!((p.r, p.g, p.b, p.a), (127, 0, 127, 255));
    }

    #[test]
    fn bgra_without_alpha_becomes_opaque() {
        let rgba = bgra_to_rgba(vec![10, 20, 30, 0, 40, 50, 60, 0], false);
        assert_eq!(rgba, vec![30, 20, 10, 255, 60, 50, 40, 255]);
    }
}
