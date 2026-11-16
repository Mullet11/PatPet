//! The pixel cat's sprite strips (assets/pet/*.png, 32×32 frames side by side), embedded in
//! the exe and decoded with WIC into premultiplied BGRA once, at start-up.

use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;

use crate::pet::Mood;

/// Size of one frame (px).
pub const FRAME: u32 = 32;

pub struct Strip {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

const PNGS: [(Mood, &[u8]); 6] = [
    (Mood::Idle, include_bytes!("../assets/pet/cat_idle.png")),
    (Mood::Music, include_bytes!("../assets/pet/cat_music.png")),
    (Mood::Sleep, include_bytes!("../assets/pet/cat_sleep.png")),
    (Mood::Focus, include_bytes!("../assets/pet/cat_focus.png")),
    (Mood::Happy, include_bytes!("../assets/pet/cat_happy.png")),
    (Mood::Yawn, include_bytes!("../assets/pet/cat_yawn.png")),
];

/// Slot of a mood in the arrays returned by `load`.
pub fn index(m: Mood) -> usize {
    PNGS.iter().position(|(k, _)| *k == m).unwrap_or(0)
}

/// Decode every strip (None for one that fails, so the cat is simply not drawn).
pub fn load() -> Vec<Option<Strip>> {
    let wic: Option<IWICImagingFactory> = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() };
    PNGS.iter()
        .map(|(_, bytes)| wic.as_ref().and_then(|f| unsafe { decode(f, bytes) }))
        .collect()
}

unsafe fn decode(f: &IWICImagingFactory, bytes: &[u8]) -> Option<Strip> {
    let stream = f.CreateStream().ok()?;
    stream.InitializeFromMemory(bytes).ok()?;
    let dec = f.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
    let frame = dec.GetFrame(0).ok()?;
    let conv = f.CreateFormatConverter().ok()?;
    conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
        .ok()?;
    let (mut w, mut h) = (0u32, 0u32);
    conv.GetSize(&mut w, &mut h).ok()?;
    if w < FRAME || h < FRAME {
        return None;
    }
    let mut px = vec![0u8; (w * h * 4) as usize];
    conv.CopyPixels(std::ptr::null(), w * 4, &mut px).ok()?;
    Some(Strip { w, h, px })
}
