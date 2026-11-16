//! The pixel cat's pictures, embedded in the exe: its animation strips (32×32 frames side by
//! side), the small props (16×16 icons side by side) and the room backgrounds. Decoded with
//! WIC into premultiplied BGRA.

use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;

use crate::pet::Mood;

/// Size of one cat frame (px).
pub const FRAME: u32 = 32;
/// Size of one prop icon (px).
pub const PROP: u32 = 16;
/// Size of a room background (px).
pub const ROOM_W: u32 = 232;
pub const ROOM_H: u32 = 70;
/// Where the cat's feet stand in a room (px from its top).
pub const ROOM_FLOOR: u32 = 64;

pub struct Strip {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

const CATS: [(Mood, &[u8]); 19] = [
    (Mood::Idle, include_bytes!("../assets/pet/cat_idle.png")),
    (Mood::Music, include_bytes!("../assets/pet/cat_music.png")),
    (Mood::Sleep, include_bytes!("../assets/pet/cat_sleep.png")),
    (Mood::Happy, include_bytes!("../assets/pet/cat_happy.png")),
    (Mood::Yawn, include_bytes!("../assets/pet/cat_yawn.png")),
    (Mood::Walk, include_bytes!("../assets/pet/cat_walk.png")),
    (Mood::Eat, include_bytes!("../assets/pet/cat_eat.png")),
    (Mood::Play, include_bytes!("../assets/pet/cat_play.png")),
    (Mood::Pounce, include_bytes!("../assets/pet/cat_pounce.png")),
    (Mood::Bath, include_bytes!("../assets/pet/cat_bath.png")),
    (Mood::Shake, include_bytes!("../assets/pet/cat_shake.png")),
    (Mood::Sad, include_bytes!("../assets/pet/cat_sad.png")),
    (Mood::Hungry, include_bytes!("../assets/pet/cat_hungry.png")),
    (Mood::Dirty, include_bytes!("../assets/pet/cat_dirty.png")),
    (Mood::Sleepy, include_bytes!("../assets/pet/cat_sleepy.png")),
    (Mood::Stretch, include_bytes!("../assets/pet/cat_stretch.png")),
    (Mood::Groom, include_bytes!("../assets/pet/cat_groom.png")),
    (Mood::TailWag, include_bytes!("../assets/pet/cat_tailwag.png")),
    (Mood::LevelUp, include_bytes!("../assets/pet/cat_levelup.png")),
];

const PROPS: &[u8] = include_bytes!("../assets/pet/props.png");

/// Room backgrounds, in the order of care::ROOMS.
const ROOMS: [&[u8]; 6] = [
    include_bytes!("../assets/pet/rooms/room_box.png"),
    include_bytes!("../assets/pet/rooms/room_bedroom.png"),
    include_bytes!("../assets/pet/rooms/room_living.png"),
    include_bytes!("../assets/pet/rooms/room_garden.png"),
    include_bytes!("../assets/pet/rooms/room_treehouse.png"),
    include_bytes!("../assets/pet/rooms/room_castle.png"),
];

/// Slot of a mood in the list returned by `cats`.
pub fn index(m: Mood) -> usize {
    CATS.iter().position(|(k, _)| *k == m).unwrap_or(0)
}

fn factory() -> Option<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
}

/// Every cat strip (None for one that fails, so that animation is simply not drawn).
pub fn cats() -> Vec<Option<Strip>> {
    let wic = factory();
    CATS.iter().map(|(_, bytes)| wic.as_ref().and_then(|f| unsafe { decode(f, bytes) })).collect()
}

/// The prop icons, side by side (see app::prop for which is which).
pub fn props() -> Option<Strip> {
    factory().and_then(|f| unsafe { decode(&f, PROPS) })
}

/// Room `i` (index into care::ROOMS).
pub fn room(i: usize) -> Option<Strip> {
    let bytes = ROOMS.get(i)?;
    factory().and_then(|f| unsafe { decode(&f, bytes) })
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
    if w == 0 || h == 0 {
        return None;
    }
    let mut px = vec![0u8; (w * h * 4) as usize];
    conv.CopyPixels(std::ptr::null(), w * 4, &mut px).ok()?;
    Some(Strip { w, h, px })
}
