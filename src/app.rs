use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Power::POWERBROADCAST_SETTING;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::events::{self, Event};
use crate::gfx::{Al, Gfx, BLACK, C, WHITE};
use crate::lyrics::{self, Line};
use crate::media::{MediaCmd, MediaState};
use crate::care::{self, Need};
use crate::pet::{Action, Mood, Pet};
use crate::store::{self, Data, Task};
use crate::util::*;
use crate::{clip, shelf, shots, sound, system, theme, thumbs, tray, update};

// ------------------------------------------------------------------ style

const T2: C = C(1.0, 1.0, 1.0, 0.62);
const T3: C = C(1.0, 1.0, 1.0, 0.38);
/// White that stays white in every theme (exact white is swapped for the theme's text color).
const PURE_WHITE: C = C(0.9999, 0.9999, 0.9999, 1.0);
const SURF: C = C(1.0, 1.0, 1.0, 0.08);
const SURF_H: C = C(1.0, 1.0, 1.0, 0.16);
const TASK_C: C = C::hex(0xFFC53D);
const SHELF_C: C = C::hex(0xB18CFF);
const RED: C = C::hex(0xFF5C5C);
const GREEN: C = C::hex(0x4CD787);
const MEDIA_C: C = C::hex(0xF2F2F7);
const CLIP_C: C = C::hex(0x5AC8FA);
const HEART_C: C = C::hex(0xFF7AA2);

// ------------------------------------------------------------------ the cat

/// Size of the cat in the closed island (DIP), the same at every screen scale.
const CAT_SIZE: f32 = 32.0;
/// The cat falls asleep after this long without mouse or keyboard input.
const CAT_SLEEP_AFTER: Duration = Duration::from_secs(5 * 60);
/// How long the hearts of a happy cat float up.
const HEARTS_SECS: f32 = 1.3;
/// Stroking the cat in the closed island: this much sideways mouse travel over it (in cat
/// widths) within STROKE_WINDOW counts as petting.
const STROKE_WIDTHS: f32 = 2.5;
const STROKE_WINDOW: Duration = Duration::from_millis(1500);
/// Prop icons in assets/pet/props.png, left to right.
mod prop {
    pub const BOWL: usize = 0;
    pub const YARN: usize = 2;
    pub const ZZZ: usize = 4;
    pub const FISH: usize = 6;
    pub const DIRT: usize = 7;
    pub const RAIN: usize = 8;
    pub const STAR: usize = 9;
    pub const BOLT: usize = 10;
    pub const SOAP: usize = 11;
}
/// Room view in the Pet tab: background pixels are drawn this many DIP each (232 px → 464 DIP).
const ROOM_SCALE: f32 = 2.0;
const ROOM_H: f32 = crate::sprites::ROOM_H as f32 * ROOM_SCALE;
/// Walking speed in the room (share of the room's width per second).
const WALK_SPEED: f32 = 0.07;
/// A "your cat needs..." notice comes at most this often.
const NEED_NOTICE_EVERY: i64 = 3 * 3600;
/// What the closed island says during a break.
const BREAK_LABEL: &str = "Break";
/// Heart glyph (Segoe Fluent Icons / MDL2: HeartFill).
const HEART: char = '\u{EB52}';

// shelf tiles (DIP); the shelf page is 500 wide with 18 px padding on each side
const SHELF_TW: f32 = 80.0;
const SHELF_TH: f32 = 96.0;
const SHELF_GAP: f32 = 9.0;
const SHELF_BODY_W: f32 = 500.0 - 36.0;

/// How many shelf tiles fit side by side in a body `w` wide.
fn shelf_per(w: f32) -> usize {
    (((w + SHELF_GAP) / (SHELF_TW + SHELF_GAP)).floor() as usize).max(1)
}

/// Width of the concave fillet joining the notch to the top screen edge (DIP).
const SHOULDER: f32 = 9.0;

mod ic {
    pub const MUSIC: char = '\u{E8D6}';
    pub const PLAY: char = '\u{E768}';
    pub const PAUSE: char = '\u{E769}';
    pub const PREV: char = '\u{E892}';
    pub const NEXT: char = '\u{E893}';
    pub const LYRICS: char = '\u{E8BD}';
    pub const FOLDER: char = '\u{E8B7}';
    pub const TASKS: char = '\u{E9D5}';
    pub const VOL: char = '\u{E767}';
    pub const MUTE: char = '\u{E74F}';
    pub const BRIGHT: char = '\u{E706}';
    pub const WIFI: char = '\u{E701}';
    pub const ETHERNET: char = '\u{E839}';
    pub const HEADPHONE: char = '\u{E7F6}';
    pub const ADD: char = '\u{E710}';
    pub const CLOSE: char = '\u{E711}';
    pub const BOX: char = '\u{E739}';
    pub const BOX_ON: char = '\u{E73A}';
    pub const MINUS: char = '\u{E738}';
    pub const DOC: char = '\u{E7C3}';
    pub const CHECK: char = '\u{E73E}';
    pub const CLIP: char = '\u{E77F}';
    pub const DOWNLOAD: char = '\u{E896}';
    pub const LEFT: char = '\u{E76B}';
    pub const RIGHT: char = '\u{E76C}';
}

// ------------------------------------------------------------------ types

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Pet,
    Media,
    Shelf,
    Tasks,
    Clipboard,
}
const TABS: [Tab; 5] = [Tab::Pet, Tab::Media, Tab::Shelf, Tab::Tasks, Tab::Clipboard];

#[derive(Clone, Copy, PartialEq, Debug)]
enum ViewKey {
    Compact(Live),
    /// line mode: just a thin line at the top edge until the mouse comes
    Line,
    Toast(u64),
    Expanded(Tab, bool),
}

/// Number fields that can be typed into directly.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Field {
    TaskTime,
}

/// An in-progress edit: field, typed text, and whether the next key replaces the text.
struct Edit {
    field: Field,
    buf: String,
    fresh: bool,
}

/// What the closed island is showing. Priority: media > task due soon > the cat alone.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Live {
    Idle,
    Media,
    Task,
    /// a break with the cat: the time left
    Break,
}

// compact-island metrics (DIP)
const PILL_H: f32 = 32.0;
const TASK_SOON_MIN: i64 = 15;
/// Early task reminders: how long before the deadline, and how the notice says it.
/// Bit i of `Task::alerts` marks entry i as shown.
pub const REMINDERS: [i64; 5] = [86400, 6 * 3600, 3 * 3600, 3600, 1800];

/// "in 1 day", "in 6 hours", "in 40 minutes": how far off a deadline `left` seconds away is.
fn left_label(left: i64) -> String {
    let plural = |n: i64, w: &str| if n == 1 { format!("in 1 {w}") } else { format!("in {n} {w}s") };
    if left >= 86400 - 1800 {
        plural(((left + 43200) / 86400).max(1), "day")
    } else if left >= 3600 - 300 {
        plural(((left + 1800) / 3600).max(1), "hour")
    } else {
        plural(((left + 30) / 60).max(1), "minute")
    }
}
/// A task put on a later day without a time is due at 09:00.
const DEFAULT_TIME: i64 = 9 * 3600;

/// The reminders that are already too late for a task due at `due` (a task added 2 hours
/// before its deadline does not get the 1-day, 6-hour and 3-hour notices afterwards).
pub fn passed_alerts(due: Option<i64>, now: i64) -> u8 {
    let Some(d) = due else { return 0 };
    let left = d - now;
    REMINDERS.iter().enumerate().filter(|(_, s)| left <= **s).fold(0, |m, (i, _)| m | (1 << i))
}

/// Which early reminder to show now for a task due at `due`: the index of the closest one that
/// has come up, if it was not shown yet, plus the new `alerts` bits. Overdue tasks get none
/// (the "due now" notice covers them).
pub fn reminder_due(due: i64, now: i64, alerts: u8) -> (Option<usize>, u8) {
    let reached = passed_alerts(Some(due), now);
    if due <= now {
        return (None, alerts | reached);
    }
    if reached & !alerts == 0 {
        return (None, alerts);
    }
    let closest = (0..REMINDERS.len()).rev().find(|i| reached & (1 << i) != 0);
    (closest, alerts | reached)
}
/// Clock in the closed island: bold, sized so the digits sit about as tall as the artwork.
const CLOCK_SIZE: f32 = 18.5;
/// Width of the closed island when nothing is going on: just an empty black notch.
const IDLE_W: f32 = 110.0;
// hover on the closed island: how much it grows, and the intent delays that keep a cursor just
// passing by (on its way to a browser tab) from waking it, and stop flicker at the edge
const HOVER_W: f32 = 16.0;
const HOVER_H: f32 = 4.0;
const HOVER_IN: Duration = Duration::from_millis(50);
const HOVER_OUT: Duration = Duration::from_millis(120);
/// critically damped (no overshoot): settles in ~180 ms growing, ~250 ms shrinking
const HOVER_W_IN: f32 = 26.0;
const HOVER_W_OUT: f32 = 18.0;
// line mode: the thin line the closed island shrinks to, and how long it stays open after
// the mouse leaves
const LINE_W: f32 = 96.0;
const LINE_H: f32 = 5.0;
const LINE_OUT: Duration = Duration::from_millis(200);
/// line <-> pill spring: critically damped, within 1 px of its target after about 250 ms
const LINE_SPRING_W: f32 = 19.0;
/// the pill dropping out of the line: it widens first, then comes down with its content
const LINE_DROP_WIDEN: f32 = 28.0;
const LINE_DROP_FALL: f32 = 16.0;
/// and going back up: it rises first, then narrows into the line
const LINE_RISE: f32 = 24.0;
/// the open panel closes this long after the mouse left it
const PANEL_OUT: Duration = Duration::from_millis(300);
/// how far beside the line (each side) the mouse still opens it: about 1 cm (96 px = 1 inch)
const LINE_HIT_PAD: f32 = 38.0;
// one-line toasts (Caps Lock)
const SHORT_H: f32 = 34.0;
const SHORT_PAD: f32 = 12.0;
const SHORT_ICON: f32 = 18.0;
const SHORT_SIZE: f32 = 13.0;
// two-line notices (Wi-Fi, headphones/TWS, charger, reminders): sized to their text
const TWO_H: f32 = 46.0;
const TWO_PAD: f32 = 9.0;
const TWO_ICON: f32 = 28.0;
const TWO_GAP: f32 = 9.0;
const TWO_TITLE: f32 = 13.5;
const TWO_SUB: f32 = 11.5;
const TWO_MAX_W: f32 = 380.0;
/// Launch / quit / hide transition: the island grows out of (or sinks back into) the top edge.
/// Width the island starts from / shrinks to, as a fraction of its resting width.
const POP_W: f32 = 0.45;
/// Give up waiting for the closing animation after this long.
const LEAVE_MAX: Duration = Duration::from_millis(700);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Leave {
    Quit,
    Hide,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    Tab(Tab),
    PlayPause,
    Prev,
    Next,
    Seek(R),
    Lyrics,
    LyricLine(f64),
    ShelfItem(usize),
    ShelfRemove(usize),
    ShelfClear,
    ShelfAll,
    ShelfCheck(usize),
    TaskField,
    TaskTime(i64),
    Edit(Field),
    TaskAdd,
    TaskToggle(u64),
    TaskDel(u64),
    /// calendar: previous / next month, back to today, pick a day
    CalMonth(i32),
    CalToday,
    CalDay(i64),
    ClipCopy(usize),
    ClipDel(usize),
    ClipClear,
    ClipCopyAll,
    Toast,
    VolBar(R),
    /// clicking the cat in the open island
    Pet,
    /// one of the care cards in the Pet tab (feed, play, sleep, bath)
    Care(Need),
    /// show the previous / next open room
    Room(i32),
}

pub enum Deferred {
    DragOut(Vec<String>),
    Menu,
}

enum TIcon {
    G(char),
    Txt(String),
    Battery(f32, bool),
    /// an icon from props.png
    Prop(usize),
}

const K_INFO: u8 = 0;
const K_VOL: u8 = 1;
const K_BRIGHT: u8 = 2;

/// What tapping a notice does besides opening its tab.
#[derive(Clone, Copy, PartialEq)]
enum Tap {
    None,
    /// download and install the update it announces
    Update,
    /// open the releases page (the automatic update failed)
    Releases,
    /// start the break the notice suggests
    Break,
}

struct Toast {
    id: u64,
    tap: Tap,
    kind: u8,
    icon: TIcon,
    title: String,
    sub: String,
    level: Option<f32>,
    accent: C,
    until: Instant,
    tab: Option<Tab>,
}

enum Lyr {
    None,
    Loading,
    Missing,
    Found(Vec<Line>),
}

#[derive(Clone, Copy)]
struct Spring {
    x: f32,
    v: f32,
    t: f32,
}

impl Spring {
    fn new(x: f32) -> Spring {
        Spring { x, v: 0.0, t: x }
    }
    fn step(&mut self, dt: f32) {
        let k: f32 = 300.0;
        self.step_with(dt, k, 2.0 * k.sqrt() * 0.74);
    }
    /// Critically damped at angular speed `w`: quick and without any bounce.
    fn step_fast(&mut self, dt: f32, w: f32) {
        self.step_with(dt, w * w, 2.0 * w);
    }
    fn step_with(&mut self, dt: f32, k: f32, c: f32) {
        let a = -k * (self.x - self.t) - c * self.v;
        self.v += a * dt;
        self.x += self.v * dt;
    }
    fn done(&self) -> bool {
        (self.x - self.t).abs() < 0.2 && self.v.abs() < 3.0
    }
    fn snap(&mut self) {
        self.x = self.t;
        self.v = 0.0;
    }
}

struct Slide {
    from_tab: Tab,
    from_lyr: bool,
    from_w: f32,
    from_h: f32,
    /// +1 slides left (moving to a later tab), -1 slides right
    dir: f32,
    t: f32,
}

const SLIDE_SECS: f32 = 0.34;

struct Ui {
    hits: Vec<(R, Act)>,
    mouse: Option<(f32, f32)>,
}

impl Ui {
    fn hover(&self, r: R) -> bool {
        self.mouse.map(|m| r.contains(m)).unwrap_or(false)
    }
    fn hit(&mut self, r: R, a: Act) -> bool {
        self.hits.push((r, a));
        self.hover(r)
    }
}

/// Sent by a second launch (e.g. from Windows Search) to bring the island back.
pub const WM_APP_SHOW: u32 = WM_APP + 3;
/// `patpet.exe --quit`: the running copy plays its closing animation, saves and exits.
pub const WM_APP_QUIT: u32 = WM_APP + 4;

const TIMER_ANIM: usize = 1;
const TIMER_TICK: usize = 2;

// ------------------------------------------------------------------ app

pub struct App {
    /// the main window (primary display): timers, tray, clipboard and frame pacing talk to it
    pub hwnd: HWND,
    /// the island being worked on (the primary display's, except inside `each` / `route`)
    s: Surface,
    /// the islands on the other displays
    others: Vec<Surface>,
    /// the window of the island the panel opens (or last opened) on
    home: HWND,
    /// the window whose island the context menu was opened on
    menu_on: HWND,
    gfx: Gfx,
    pub data: Data,
    dirty: bool,
    last_min: i64,
    expanded: bool,
    tab: Tab,
    lyrics_open: bool,
    media: Option<MediaState>,
    art: Option<ID2D1Bitmap>,
    art_color: C,
    art_key: u64,
    pending_play: Option<(bool, Instant)>,
    media_tx: Sender<MediaCmd>,
    lyr_key: u64,
    lyr: Lyr,
    lyr_scroll: f32,
    toast: Option<Toast>,
    toast_seq: u64,
    audio: Option<system::Audio>,
    caps: bool,
    lang: u16,
    net: Option<bool>,
    net_check: Option<Instant>,
    power: Option<(bool, u8)>,
    low_warned: bool,
    bright_seen: bool,
    input: String,
    input_focus: bool,
    surrogate: Option<u16>,
    task_due: Option<i64>,
    task_scroll: usize,
    /// calendar: the month shown (year, 1-12) and the picked day (day number)
    cal_month: (i64, i64),
    sel_day: i64,
    /// clipboard history, newest first (memory only)
    clips: Vec<clip::Item>,
    clip_scroll: usize,
    /// which history entry was copied back from the list, and when (for the "Copied" mark)
    clip_copied: Option<(Instant, String)>,
    /// text just dropped onto the island (its row says "Saved" for a moment)
    clip_dropped: Option<(Instant, String)>,
    /// when "Copy all" last copied the whole list (its button says "Copied" for a moment)
    clip_all_at: Option<Instant>,
    /// text PatPet itself just put on the clipboard: its echo must not reorder the history
    clip_ignore: Option<String>,
    /// the clipboard changed: read it at this time (see on_clipboard)
    clip_due: Option<Instant>,
    /// when the last "Copied" notice was shown
    copied_at: Option<Instant>,
    /// the context menu of the island is open
    menu_hold: bool,
    /// new screenshots -> Shelf
    shots: shots::Watcher,
    shots_next: Instant,
    shelf_scroll: usize,
    /// animated shelf position (in tiles), slides toward `shelf_scroll`
    shelf_x: f32,
    drag_hover: bool,
    /// the drag over the island carries text (it goes to the Clipboard tab)
    drag_text: bool,
    /// Shelf image previews by path; None while decoding (or when the file is not readable)
    thumbs: HashMap<String, Option<ID2D1Bitmap>>,
    drag_expanded: bool,
    collapse_at: Option<Instant>,
    /// the mouse has been over the panel since it opened (a panel opened from the tray, with
    /// the cursor far away, must not close by itself right away)
    panel_entered: bool,
    anim_on: bool,
    /// the frames now coming are only for the cat's next animation frame
    cat_only: bool,
    last_frame: Instant,
    t0: Instant,
    last_sec: Instant,
    tick_ms: u32,
    demo: bool,
    demo_idle: bool,
    /// Selected shelf paths. Dragging a selected tile carries the whole selection.
    shelf_sel: Vec<String>,
    /// hidden by the user from the menu (separate from auto-hiding in full screen)
    pub user_hidden: bool,
    edit: Option<Edit>,
    demo_switch: Option<Instant>,
    /// testing aid (PATPET_DEMO=stress): next step and how many were done
    demo_stress: Option<(Instant, u32)>,
    pub deferred: Option<Deferred>,
    pub autostart: bool,
    /// closing animation in progress: what to do at the end, when it started, target width
    leaving: Option<(Leave, Instant, f32)>,
    /// when to hand buffers and idle pages back to Windows after the island got small again
    trim_at: Option<Instant>,
    /// start and frame count of the running animation (debug log)
    anim_stat: (Instant, u32),
    /// the running animation changed what is shown (not just a hover): trim memory afterwards
    anim_view_change: bool,
    /// last seen modification time of theme.json (reloaded when it changes)
    theme_mtime: Option<std::time::SystemTime>,
    /// a newer release found on GitHub
    upd_avail: Option<update::Release>,
    /// checking or installing right now
    upd_busy: bool,
    /// the user asked: report the result even when there is nothing new
    upd_manual: bool,
    /// next automatic check
    upd_next: Instant,
    /// the version whose "Update available" notice was already shown
    upd_shown: Option<String>,
    /// the pixel cat: mood logic, its sprite strips (by sprites::index) and the frame on screen
    pet: Pet,
    /// break reminders (see breaks.rs)
    breaks: crate::breaks::Breaks,
    cat_strips: Vec<Option<crate::sprites::Strip>>,
    cat_bmps: Vec<Option<crate::gfx::Pixmap>>,
    props_bmp: Option<crate::gfx::Pixmap>,
    /// the room shown in the Pet tab (index into care::ROOMS); decoded while that tab is open
    room_bmp: Option<(usize, crate::gfx::Pixmap)>,
    /// where the cat is in its room (0 = left wall, 1 = right wall), where it is heading,
    /// when it sets off again, and when its position was last moved
    room_x: f32,
    room_goal: Option<f32>,
    room_rest: Instant,
    room_step: Instant,
    room_rng: u32,
    cat_frame: (Mood, usize),
    /// when the hearts of a happy cat started floating
    hearts_at: Option<Instant>,
}

/// A display: where the top of the screen is centred, its DPI scale and its handle.
#[derive(Clone, Copy, Default)]
struct Display {
    cx: i32,
    top: i32,
    scale: f32,
    handle: HMONITOR,
}

/// Every display, the primary one first.
fn displays() -> Vec<Display> {
    unsafe extern "system" fn add(hm: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        let list = &mut *(lp.0 as *mut Vec<Display>);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(hm, &mut mi).as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let r = mi.rcMonitor;
            let d = Display { cx: (r.left + r.right) / 2, top: r.top, scale: dx as f32 / 96.0, handle: hm };
            // MONITORINFOF_PRIMARY
            if mi.dwFlags & 1 != 0 {
                list.insert(0, d);
            } else {
                list.push(d);
            }
        }
        TRUE
    }
    let mut list: Vec<Display> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(add), LPARAM(&mut list as *mut Vec<Display> as isize));
    }
    if list.is_empty() {
        // no answer: the primary display alone
        unsafe {
            let hm = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(hm, &mut mi);
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let r = mi.rcMonitor;
            list.push(Display { cx: (r.left + r.right) / 2, top: r.top, scale: dx as f32 / 96.0, handle: hm });
        }
    }
    list
}

/// One island. There is one per display, each in its own window; they all show the same
/// thing, except that the open panel is only on the display it was opened on (`App::home`).
struct Surface {
    hwnd: HWND,
    display: Display,
    mon_cx: i32,
    mon_top: i32,
    sw: Spring,
    sh: Spring,
    sr: Spring,
    view: ViewKey,
    view_age: f32,
    /// content size for `view`, worked out together with it (see draw)
    view_wh: Option<(f32, f32)>,
    /// the line came from the closed pill (its content fades out as it shrinks); from the open
    /// island or a notice it shrinks empty
    line_from_pill: bool,
    /// the pill came down out of the line: its content slides down with it instead of fading in
    pill_from_line: bool,
    /// the screen's center in window coordinates: fixed on screen, unlike the center of an
    /// island whose width moves by whole pixels
    screen_cx: f32,
    /// right after closing in line mode: do not treat the cursor as hovering (see tick)
    no_hover_until: Option<Instant>,
    hits: Vec<(R, Act)>,
    mouse: Option<(f32, f32)>,
    tracking: bool,
    press: Option<(Option<Act>, f32, f32)>,
    slide: Option<Slide>,
    /// not on screen: hidden by the user, or a full-screen app is in front on this display
    hidden: bool,
    win: (i32, i32, i32, i32),
    isl_px: (i32, i32, i32, i32),
    /// hover progress 0..1 and its velocity, driven by its own spring
    hov: f32,
    hov_v: f32,
    /// hover as decided after the intent delays
    hover_on: bool,
    /// when the cursor entered / left the island
    hover_in: Option<Instant>,
    hover_out: Option<Instant>,
    /// how far the open (expanded) colors are faded in, 0..1
    open_k: f32,
    /// a long line under the mouse is sliding (keeps frames coming), and which one since when
    marquee_on: std::cell::Cell<bool>,
    marquee_at: std::cell::Cell<Option<(u64, Instant)>>,
    /// where the cat was drawn last (window DIP), for stroking it with the mouse
    cat_rect: std::cell::Cell<Option<R>>,
    /// stroking in progress: sideways travel so far, last x, when it started
    stroke: Option<(f32, f32, Instant)>,
}

impl Surface {
    fn new(hwnd: HWND, display: Display) -> Surface {
        Surface {
            hwnd,
            display,
            mon_cx: display.cx,
            mon_top: display.top,
            sw: Spring::new(40.0),
            sh: Spring::new(20.0),
            sr: Spring::new(10.0),
            view: ViewKey::Compact(Live::Idle),
            view_age: 0.0,
            view_wh: None,
            line_from_pill: false,
            pill_from_line: false,
            screen_cx: 0.0,
            no_hover_until: None,
            hits: Vec::new(),
            mouse: None,
            tracking: false,
            press: None,
            slide: None,
            hidden: false,
            win: (0, 0, 0, 0),
            isl_px: (0, 0, 0, 0),
            hov: 0.0,
            hov_v: 0.0,
            hover_on: false,
            hover_in: None,
            hover_out: None,
            open_k: 0.0,
            marquee_on: std::cell::Cell::new(false),
            marquee_at: std::cell::Cell::new(None),
            cat_rect: std::cell::Cell::new(None),
            stroke: None,
        }
    }
}


impl App {
    pub fn new(hwnd: HWND) -> windows::core::Result<App> {
        let primary = displays().into_iter().next().unwrap_or_default();
        let mut gfx = Gfx::new(primary.scale)?;
        theme::ensure_file();
        let th = theme::load();
        gfx.set_theme(&th);
        let first_run = store::first_run();
        let mut data = store::load();
        if first_run && std::env::var_os("PATPET_DEMO").is_none() {
            // Fresh install: on by default. Saved at once, so what the user changes later stays.
            tray::set_autostart(true);
            data.settings.screenshots = true;
            store::save(&data);
        }
        let media_tx = crate::media::spawn();
        let audio = system::Audio::new();
        system::watch_network();
        system::watch_brightness(hwnd);
        system::install_kb_hook();
        let n = Instant::now();
        let app = App {
            hwnd,
            s: Surface::new(hwnd, primary),
            others: Vec::new(),
            home: hwnd,
            menu_on: hwnd,
            gfx,
            data,
            dirty: false,
            last_min: 0,
            expanded: false,
            tab: Tab::Pet,
            lyrics_open: false,
            media: None,
            art: None,
            art_color: MEDIA_C,
            art_key: 0,
            pending_play: None,
            media_tx,
            lyr_key: 0,
            lyr: Lyr::None,
            lyr_scroll: 0.0,
            toast: None,
            toast_seq: 0,
            audio,
            caps: system::caps_state(),
            lang: system::fg_lang(),
            net: Some(system::net_status().0),
            net_check: None,
            power: system::power(),
            low_warned: false,
            bright_seen: false,
            input: String::new(),
            input_focus: false,
            surrogate: None,
            task_due: None,
            task_scroll: 0,
            cal_month: (now().y, now().mo),
            sel_day: now().day(),
            clips: Vec::new(),
            clip_scroll: 0,
            clip_copied: None,
            clip_dropped: None,
            clip_all_at: None,
            clip_ignore: None,
            clip_due: None,
            copied_at: None,
            menu_hold: false,
            shots: shots::Watcher::new(),
            shots_next: Instant::now(),
            shelf_scroll: 0,
            shelf_x: 0.0,
            drag_hover: false,
            drag_text: false,
            thumbs: HashMap::new(),
            drag_expanded: false,
            collapse_at: None,
            panel_entered: false,
            anim_on: false,
            cat_only: false,
            last_frame: n,
            t0: n,
            last_sec: n,
            tick_ms: 0,
            demo: false,
            demo_idle: false,
            shelf_sel: Vec::new(),
            user_hidden: false,
            edit: None,
            demo_switch: None,
            demo_stress: None,
            deferred: None,
            autostart: tray::autostart_enabled(),
            leaving: None,
            trim_at: Some(n + Duration::from_secs(5)),
            anim_stat: (n, 0),
            anim_view_change: false,
            theme_mtime: theme::modified(),
            upd_avail: None,
            upd_busy: false,
            upd_manual: false,
            upd_next: n + Duration::from_secs(20),
            upd_shown: None,
            pet: Pet::new(CAT_SLEEP_AFTER),
            breaks: Default::default(),
            cat_strips: crate::sprites::cats(),
            cat_bmps: Vec::new(),
            props_bmp: None,
            room_bmp: None,
            room_x: 0.45,
            room_goal: None,
            room_rest: n + Duration::from_secs(2),
            room_step: n,
            room_rng: 0x9E37_79B9,
            cat_frame: (Mood::Idle, 0),
            hearts_at: None,
        };
        clip::listen(hwnd);
        Ok(app)
    }

    pub fn start(&mut self) {
        self.set_tick(250);
        if let Ok(v) = std::env::var("PATPET_DEMO") {
            self.demo_view(&v);
            // testing aid: PATPET_MOUSE=x,y (window pixels) pretends the cursor rests there, to see hovers
            if let Some((x, y)) = std::env::var("PATPET_MOUSE").ok().and_then(|m| {
                let (x, y) = m.split_once(',')?;
                Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
            }) {
                self.s.mouse = Some((x / self.gfx.scale, y / self.gfx.scale));
            }
        }
        // the other displays get their islands (demo screenshots stay on the primary one)
        if !self.demo {
            self.sync_displays();
        }
        self.each(|a| a.pop_in());
        self.kick();
    }

    /// Opening transition: start as a thin sliver at the top edge and spring out to size.
    fn pop_in(&mut self) {
        let (tw, _, _, _) = self.target();
        self.s.sw = Spring::new(tw * POP_W);
        self.s.sh = Spring::new(0.0);
        self.s.sr = Spring::new(0.0);
        self.s.view_age = 0.0;
        self.kick();
    }

    /// Closing transition: fold the island back into the top edge, then quit or hide.
    fn leave(&mut self, what: Leave) {
        if self.all_hidden() {
            // nothing on screen to animate
            self.finish_leave(what);
            return;
        }
        if self.leaving.is_none() {
            self.collapse();
            self.toast = None;
            let (tw, _, _, _) = self.target();
            self.leaving = Some((what, Instant::now(), tw * POP_W));
        }
        self.kick();
    }

    pub fn begin_quit(&mut self) {
        self.leave(Leave::Quit);
    }

    fn finish_leave(&mut self, what: Leave) {
        dbg_log(&format!("leave: done ({what:?})"));
        self.leaving = None;
        match what {
            // WM_CLOSE goes through DestroyWindow -> WM_DESTROY, which saves the data
            Leave::Quit => unsafe {
                // stay hidden for the few messages left before the windows go away
                self.user_hidden = true;
                self.each(|a| {
                    a.s.hidden = true;
                    let _ = ShowWindow(a.s.hwnd, SW_HIDE);
                });
                let _ = PostMessageW(self.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            Leave::Hide => {
                self.user_hidden = true;
                self.apply_visibility();
            }
        }
    }

    /// Screenshot/testing aid: PATPET_DEMO=pet|media|lyrics|shelf|tasks|toast|volume|caps|capsoff|stress
    fn demo_view(&mut self, v: &str) {
        self.demo = true;
        match v {
            "lyrics" => {
                self.lyrics_open = true;
                self.expand(Some(Tab::Media));
                self.ensure_lyrics();
            }
            "media" => self.expand(Some(Tab::Media)),
            "shelf" | "shelfall" => {
                if v == "shelfall" {
                    self.shelf_sel = self.data.shelf.iter().take(2).cloned().collect();
                }
                if self.data.shelf.is_empty() {
                    let win = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
                    self.data.shelf = vec![format!(r"{win}\notepad.exe"), format!(r"{win}\win.ini"), format!(r"{win}\System32")];
                }
                self.expand(Some(Tab::Shelf));
            }
            // image previews: a few files from PATPET_DEMO_DIR next to a non-image file
            "shelfimg" => {
                if let Some(dir) = std::env::var_os("PATPET_DEMO_DIR") {
                    let mut imgs: Vec<String> = std::fs::read_dir(dir)
                        .map(|r| r.flatten().map(|e| e.path().to_string_lossy().into_owned()).filter(|p| thumbs::is_image(p)).take(4).collect())
                        .unwrap_or_default();
                    let win = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
                    imgs.push(format!(r"{win}\win.ini"));
                    self.data.shelf = imgs;
                }
                self.expand(Some(Tab::Shelf));
            }
            // text dragged over the island (empty / filled Clipboard tab)
            "cliphover" | "cliphoverfull" => {
                if v == "cliphoverfull" {
                    let n = now().secs;
                    clip::push(&mut self.clips, "Meeting moved to 3 pm".into(), n - 300);
                    clip::push(&mut self.clips, "https://example.com/report".into(), n - 60);
                    self.clip_dropped = Some((Instant::now() + Duration::from_secs(60), "https://example.com/report".into()));
                }
                self.drag_hover = true;
                self.drag_text = true;
                self.expand(Some(Tab::Clipboard));
            }
            // leak check: keeps re-fetching lyrics and opening / closing the island
            "stress" => {
                self.lyrics_open = true;
                self.expand(Some(Tab::Media));
                self.ensure_lyrics();
                self.demo_stress = Some((Instant::now() + Duration::from_millis(1500), 0));
            }
            "slide" => {
                self.expand(Some(Tab::Media));
                self.demo_switch = Some(Instant::now() + Duration::from_millis(1500));
            }
            "tasks" | "tasksday" | "tasksfeb" => {
                // always the samples (demo mode never saves, the real list stays as it is)
                {
                    self.data.tasks.clear();
                    let n = now().secs;
                    let day = n.div_euclid(86400) * 86400;
                    self.data.tasks.push(Task { id: 1, text: "Send the weekly report to the whole team before Friday".into(), due: Some(n + 3600), ..Default::default() });
                    self.data.tasks.push(Task { id: 2, text: "Call the dentist".into(), due: Some(n - 600), ..Default::default() });
                    self.data.tasks.push(Task { id: 3, text: "Buy oat milk".into(), done: true, ..Default::default() });
                    self.data.tasks.push(Task { id: 4, text: "Team meeting".into(), due: Some(day + 86400 + 10 * 3600), ..Default::default() });
                    self.data.tasks.push(Task { id: 5, text: "Submit the assignment".into(), due: Some(day + 5 * 86400 + 23 * 3600 + 59 * 60), ..Default::default() });
                    self.data.tasks.push(Task { id: 6, text: "Pay rent".into(), due: Some(day - 3 * 86400 + 12 * 3600), done: true, ..Default::default() });
                }
                if v == "tasksday" {
                    self.select_day(now().day() + 5);
                }
                // February 2027 starts on a Monday: only 4 weeks of its own
                if v == "tasksfeb" {
                    self.select_day(days_from_civil(2027, 2, 10));
                }
                self.input = "Review pull request".into();
                self.input_focus = true;
                self.adjust_due(0);
                self.expand(Some(Tab::Tasks));
            }
            "clipboard" => {
                let n = now().secs;
                let samples = ["PatPet", "Meeting moved to 3 pm,\nsame room", "https://lrclib.net/docs/api/get-lyrics-by-track-signature-and-duration", "git commit -m \"fix: seam on light themes\""];
                for (i, t) in samples.iter().enumerate() {
                    clip::push(&mut self.clips, t.to_string(), n - 420 * (3 - i as i64));
                }
                self.expand(Some(Tab::Clipboard));
            }
            "line" => {
                self.data.settings.line_mode = true;
            }
            "clipboardempty" => self.expand(Some(Tab::Clipboard)),
            "tasksempty" => {
                self.data.tasks.clear();
                self.expand(Some(Tab::Tasks));
            }
            "pet" => self.cheer(),
            "petroom" | "pethungry" | "petnap" | "petlevel" => {
                // a well kept cat (not whatever data.json holds), then what the view shows
                let c = &mut self.data.pet;
                *c = crate::care::Care { food: 80.0, fun: 70.0, energy: 85.0, clean: 75.0, ..Default::default() };
                c.updated = unix_now();
                c.level = 6;
                c.xp = 70;
                c.room = 2;
                match v {
                    "pethungry" => c.food = 12.0,
                    "petnap" => {
                        c.nap = true;
                        c.energy = 40.0;
                    }
                    _ => {}
                }
                self.sync_care();
                if v == "petlevel" {
                    self.on_levels(1);
                }
                self.expand(Some(Tab::Pet));
            }
            "breakdue" => self.ask_break(),
            "breaktimer" => {
                // a break just started
                self.breaks.start(self.mono_secs());
                self.pet.set_resting(true, Instant::now());
            }
            "petpill" => {
                self.data.pet = crate::care::Care { food: 80.0, fun: 70.0, energy: 85.0, clean: 10.0, ..Default::default() };
                self.data.pet.updated = unix_now();
                self.sync_care();
            }
            "toast" => self.toast(TIcon::G(ic::FOLDER), "Screenshot added to Shelf", "Drag it anywhere from the Shelf", 600.0, SHELF_C, Some(Tab::Shelf)),
            "volume" => {
                self.hud(K_VOL, ic::VOL, 0.64, WHITE);
                if let Some(t) = &mut self.toast {
                    t.until = Instant::now() + Duration::from_secs(600);
                }
            }
            "caps" => self.toast(TIcon::Txt("⇪".into()), "Caps Lock on", "", 600.0, GREEN, None),
            "capsoff" => self.toast(TIcon::Txt("⇪".into()), "Caps Lock off", "", 600.0, WHITE, None),
            "tws" => self.toast(TIcon::G(ic::HEADPHONE), "Headphones connected", "Galaxy Buds2 Pro", 600.0, WHITE, None),
            "copied" => self.toast(TIcon::G(ic::CLIP), "Text copied", "", 600.0, WHITE, None),
            "charging" => self.toast(TIcon::Battery(0.64, true), "Charging", "64% · plugged in", 600.0, GREEN, None),
            "idle" => self.demo_idle = true,
            "edittask" => {
                self.input = "Submit assignment".into();
                self.expand(Some(Tab::Tasks));
                self.edit = Some(Edit { field: Field::TaskTime, buf: "1h30".into(), fresh: false });
            }
            // updater test: with PATPET_UPDATE_FROM=<older version>, checks and installs right away
            "update" => {
                self.upd_busy = true;
                update::check(true);
            }
            "taskremind" => self.toast(TIcon::G(ic::TASKS), "Task due in 1 hour", "Send the weekly report · 14:30", 600.0, TASK_C, Some(Tab::Tasks)),
            "taskpill" => {
                self.demo_idle = true;
                let n = now().secs;
                self.data.tasks = vec![Task { id: 1, text: "Call the dentist".into(), due: Some(n + 8 * 60 + 30), ..Default::default() }];
            }
            _ => {}
        }
    }

    pub fn save_now(&mut self) {
        if self.demo {
            return;
        }
        store::save(&self.data);
        self.dirty = false;
    }

    fn set_tick(&mut self, ms: u32) {
        if self.tick_ms != ms {
            self.tick_ms = ms;
            unsafe { SetTimer(self.hwnd, TIMER_TICK, ms, None) };
        }
    }

    fn kick(&mut self) {
        // anything else asking for frames may change more than the cat
        self.cat_only = false;
        if !self.anim_on {
            self.anim_on = true;
            self.last_frame = Instant::now();
            self.anim_stat = (self.last_frame, 0);
            if crate::vsync::available() {
                // paced to the display's refresh rate
                crate::vsync::run(true);
            } else {
                unsafe { SetTimer(self.hwnd, TIMER_ANIM, 15, None) };
            }
        }
    }

    fn send(&self, c: MediaCmd) {
        let _ = self.media_tx.send(c);
    }

    fn media_pos(&self) -> f64 {
        match &self.media {
            Some(m) => {
                let mut p = m.pos;
                if m.playing {
                    p += m.at.elapsed().as_secs_f64();
                }
                if m.dur > 0.0 {
                    p.min(m.dur)
                } else {
                    p
                }
            }
            None => 0.0,
        }
    }

    fn cur_line(&self) -> Option<usize> {
        let Lyr::Found(v) = &self.lyr else { return None };
        let p = self.media_pos() + 0.25;
        let i = v.partition_point(|l| l.t <= p);
        if i == 0 {
            None
        } else {
            Some(i - 1)
        }
    }

    fn ensure_lyrics(&mut self) {
        if let Some(m) = &self.media {
            if self.lyr_key != m.key {
                self.lyr_key = m.key;
                self.lyr = Lyr::Loading;
                self.lyr_scroll = 0.0;
                lyrics::fetch(m.key, m.title.clone(), m.artist.clone(), m.album.clone(), m.dur);
            }
        }
    }

    fn cur_art(&self) -> Option<&ID2D1Bitmap> {
        let key = self.media.as_ref()?.key;
        if key == self.art_key {
            self.art.as_ref()
        } else {
            None
        }
    }

    fn accent(&self) -> C {
        if self.cur_art().is_some() {
            self.art_color
        } else {
            MEDIA_C
        }
    }

    // -------------------------------------------------------------- toasts

    fn toast(&mut self, icon: TIcon, title: &str, sub: &str, secs: f32, accent: C, tab: Option<Tab>) {
        self.toast_seq += 1;
        self.toast = Some(Toast {
            id: self.toast_seq,
            tap: Tap::None,
            kind: K_INFO,
            icon,
            title: title.into(),
            sub: sub.into(),
            level: None,
            accent,
            until: Instant::now() + Duration::from_secs_f32(secs),
            tab,
        });
        self.kick();
    }

    fn hud(&mut self, kind: u8, ch: char, level: f32, accent: C) {
        if self.expanded {
            return;
        }
        let until = Instant::now() + Duration::from_millis(1600);
        if let Some(t) = &mut self.toast {
            if t.kind == kind {
                t.level = Some(level);
                t.icon = TIcon::G(ch);
                t.until = until;
                self.kick();
                return;
            }
        }
        self.toast_seq += 1;
        self.toast = Some(Toast {
            id: self.toast_seq,
            tap: Tap::None,
            kind,
            icon: TIcon::G(ch),
            title: String::new(),
            sub: String::new(),
            level: Some(level),
            accent,
            until,
            tab: None,
        });
        self.kick();
    }

    // -------------------------------------------------------------- expand

    fn expand(&mut self, tab: Option<Tab>) {
        let t = tab.unwrap_or(if self.media.is_some() {
            Tab::Media
        } else if self.tab == Tab::Media {
            Tab::Media
        } else {
            self.tab
        });
        self.tab = t;
        // the panel opens on this display (closing on another one if it was open there)
        if !self.expanded || self.home != self.s.hwnd {
            self.panel_entered = self.cursor_inside();
        }
        self.home = self.s.hwnd;
        self.expanded = true;
        self.toast = None;
        self.collapse_at = None;
        unsafe {
            let _ = SetForegroundWindow(self.home);
        }
        if self.tab == Tab::Media && self.lyrics_open {
            self.ensure_lyrics();
        }
        self.kick();
    }

    fn collapse(&mut self) {
        self.commit_edit();
        self.shelf_sel.clear();
        self.expanded = false;
        self.input_focus = false;
        self.drag_expanded = false;
        system::watch_clicks(false);
        self.collapse_at = None;
        let line_mode = self.data.settings.line_mode;
        let home = self.home;
        self.on(home, |a| {
            a.s.press = None;
            if line_mode {
                // The cursor was over the open panel (Esc) or just clicked outside it: that is
                // not a hover on the line, so close straight to the line. A cursor really resting
                // on the line still opens it with its next movement.
                a.s.mouse = None;
                a.s.tracking = false;
                a.s.hover_on = false;
                a.s.no_hover_until = Some(Instant::now() + Duration::from_millis(350));
            }
        });
        self.kick();
    }

    // -------------------------------------------------------------- layout

    fn target(&self) -> (f32, f32, f32, ViewKey) {
        if let Some((_, _, w)) = self.leaving {
            // keep the current view so its content simply fades while the shape sinks away
            return (w, 0.0, 0.0, self.s.view);
        }
        if self.expanded && self.s.hwnd == self.home {
            let lyr = self.tab == Tab::Media && self.lyrics_open && self.media.is_some();
            let (w, h) = match self.tab {
                Tab::Pet => (500.0, 264.0),
                Tab::Media if lyr => (760.0, 214.0),
                Tab::Media => (500.0, 190.0),
                Tab::Shelf => (500.0, 196.0),
                Tab::Tasks => (500.0, 250.0),
                Tab::Clipboard => (500.0, 250.0),
            };
            return (w, h, 30.0, ViewKey::Expanded(self.tab, lyr));
        }
        if !self.s.hidden {
            if let Some(t) = &self.toast {
                if t.level.is_some() {
                    return (320.0, 40.0, 16.0, ViewKey::Toast(t.id));
                }
                if t.sub.is_empty() {
                    // one-liners (Caps Lock): a small pill hugging the text
                    let w = SHORT_PAD + SHORT_ICON + 8.0 + self.gfx.measure(&t.title, SHORT_SIZE, 600) + SHORT_PAD + 4.0;
                    return (w, SHORT_H, SHORT_H / 2.0, ViewKey::Toast(t.id));
                }
                let text = self.gfx.measure(&t.title, TWO_TITLE, 600).max(self.gfx.measure(&t.sub, TWO_SUB, 400));
                let cat = self.cat_size() + 6.0;
                let w = (TWO_PAD + TWO_ICON + TWO_GAP + text + TWO_PAD + 8.0 + cat).clamp(150.0, TWO_MAX_W + cat);
                return (w, TWO_H, 20.0, ViewKey::Toast(t.id));
            }
        }
        if self.data.settings.line_mode && !self.s.hover_on && !self.drag_here() && !self.s.hidden {
            return (LINE_W, LINE_H, LINE_H / 2.0, ViewKey::Line);
        }
        let (live, sec) = self.lives();
        // hover growth is added on top in render(), driven by its own spring
        let w = self.compact_width(live) + self.segment_width(sec);
        (w, PILL_H, 13.0, ViewKey::Compact(live))
    }

    /// (primary activity, optional secondary activity shown at the right end of the island)
    fn lives(&self) -> (Live, Option<Live>) {
        let mut v = Vec::with_capacity(3);
        if self.breaks.resting() {
            v.push(Live::Break);
        }
        if self.media.is_some() {
            v.push(Live::Media);
        }
        if self.task_soon().is_some() {
            v.push(Live::Task);
        }
        (v.first().copied().unwrap_or(Live::Idle), v.get(1).copied())
    }

    /// The nearest unfinished task due within the next few minutes (or just overdue).
    fn task_soon(&self) -> Option<(&Task, i64)> {
        let n = now().secs;
        self.data
            .tasks
            .iter()
            .filter(|t| !t.done)
            .filter_map(|t| t.due.map(|d| (t, d - n)))
            .filter(|(_, left)| *left <= TASK_SOON_MIN * 60 && *left > -10 * 60)
            .min_by_key(|(_, left)| *left)
    }

    fn task_line(&self) -> Option<(String, String, bool)> {
        let (t, left) = self.task_soon()?;
        let due = if left <= 0 {
            "now".to_string()
        } else {
            format!("in {}m", (left + 59) / 60)
        };
        Some((t.text.clone(), due, left <= 0))
    }

    fn compact_width(&self, live: Live) -> f32 {
        match live {
            // the cat sits in the middle of the idle notch
            Live::Idle => IDLE_W,
            _ => self.cat_slot() + self.activity_width(live),
        }
    }

    /// Width the closed island needs for an activity, without the cat.
    fn activity_width(&self, live: Live) -> f32 {
        let g = &self.gfx;
        match live {
            Live::Idle => 0.0,
            // art | clock | equalizer — must mirror draw_compact
            Live::Media => 7.0 + (PILL_H - 10.0) + 6.0 + g.measure_clock("88:88", CLOCK_SIZE, 600) + 24.0 + 6.0 + 18.0 + 14.0,
            Live::Task => {
                let (text, due, _) = self.task_line().unwrap_or_default();
                12.0 + 16.0 + 8.0 + g.measure(&text, 12.5, 500).clamp(30.0, 170.0) + 10.0 + g.measure(&due, 12.0, 600) + 14.0
            }
            // "Break" | 4:59 — must mirror draw_compact
            Live::Break => 10.0 + g.measure(BREAK_LABEL, 12.5, 600) + 12.0 + g.measure_clock("8:88", CLOCK_SIZE, 600) + 14.0,
        }
    }


    // -------------------------------------------------------------- frame

    /// One animation frame for every island; returns true once everything is at rest.
    pub fn frame(&mut self) -> bool {
        let n = Instant::now();
        let dt = (n - self.last_frame).as_secs_f32().min(1.0 / 30.0);
        self.last_frame = n;
        self.anim_stat.1 += 1;
        // shelf: slide toward the scroll position (ease-out)
        self.shelf_scroll = self.shelf_scroll.min(self.shelf_max());
        let target = self.shelf_scroll as f32;
        let d = target - self.shelf_x;
        let shelf_moving = d.abs() > 0.002;
        if shelf_moving {
            self.shelf_x += d * (1.0 - (-dt * 13.0).exp());
        } else {
            self.shelf_x = target;
        }
        let mut lyr_moving = false;
        if let Some(c) = self.cur_line() {
            let t = c as f32;
            let d = t - self.lyr_scroll;
            if d.abs() > 6.0 {
                self.lyr_scroll = t;
            } else {
                self.lyr_scroll += d * (dt * 10.0).min(1.0);
            }
            lyr_moving = d.abs() > 0.004;
        }
        // every island moves on a frame and is drawn
        let mut settled = !lyr_moving && !shelf_moving && !self.hearts_on();
        let mut folded = true;
        self.each(|a| {
            let (rest, gone) = a.frame_surface(n, dt);
            settled &= rest;
            folded &= gone;
        });
        self.cat_only = false;
        if let Some((what, since, _)) = self.leaving {
            if folded || since.elapsed() >= LEAVE_MAX {
                self.finish_leave(what);
                return true;
            }
        }
        if settled {
            self.each(|a| {
                a.s.sw.snap();
                a.s.sh.snap();
                a.s.sr.snap();
            });
            // the animation's layer bitmaps are not needed while resting: let them go now
            self.gfx.release_layers();
            self.anim_on = false;
            crate::vsync::run(false);
            let (t0, frames) = self.anim_stat;
            let view_changed = std::mem::take(&mut self.anim_view_change);
            let secs = n.duration_since(t0).as_secs_f32();
            if frames > 10 {
                dbg_log(&format!("anim: {frames} frames in {:.0} ms ({:.0} fps)", secs * 1000.0, frames as f32 / secs));
                // a real animation (hover, notice, collapse) paged things back in: trim again
                // a little after the island comes to rest (single redraws don't count)
                // (a hover alone doesn't count: the next hover would stutter paging it back in)
                if view_changed && !self.expanded && self.toast.is_none() && self.leaving.is_none() {
                    self.trim_at = Some(n + Duration::from_secs(3));
                }
            }
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_ANIM);
            }
        }
        settled
    }

    /// One frame of the island being worked on: its shape springs toward what it should show,
    /// and it is drawn. Returns (at rest, folded away into the top edge).
    fn frame_surface(&mut self, n: Instant, dt: f32) -> (bool, bool) {
        let (tw, th, tr, key) = self.target();
        let hover_still = self.step_hover(n, dt, key);
        self.s.view_wh = Some((tw, th));
        if key != self.s.view {
            self.anim_view_change = true;
            match (self.s.view, key) {
                // switching feature while open: slide instead of fade
                (ViewKey::Expanded(a, al), ViewKey::Expanded(b, _)) if a != b && self.s.view_age > 0.05 => {
                    let idx = |t: Tab| TABS.iter().position(|x| *x == t).unwrap_or(0) as f32;
                    self.s.slide = Some(Slide {
                        from_tab: a,
                        from_lyr: al,
                        from_w: self.s.sw.t,
                        from_h: self.s.sh.t,
                        dir: (idx(b) - idx(a)).signum(),
                        t: 0.0,
                    });
                }
                _ => {
                    self.s.slide = None;
                    self.s.view_age = 0.0;
                }
            }
            if key == ViewKey::Line {
                self.s.line_from_pill = matches!(self.s.view, ViewKey::Compact(_));
            }
            self.s.pill_from_line = matches!(key, ViewKey::Compact(_)) && self.s.view == ViewKey::Line && self.s.sh.x < PILL_H;
            self.s.view = key;
        }
        if let Some(sl) = &mut self.s.slide {
            sl.t += dt / SLIDE_SECS;
            if sl.t >= 1.0 {
                self.s.slide = None;
            }
        }
        self.s.sw.t = tw;
        self.s.sh.t = th;
        self.s.sr.t = tr;
        // line mode, between the line and the pill (not a panel or notice closing to the line)
        let line_pill = self.data.settings.line_mode
            && self.leaving.is_none()
            && match self.s.view {
                ViewKey::Line => self.s.line_from_pill,
                ViewKey::Compact(_) => true,
                _ => false,
            };
        let mut rem = dt;
        while rem > 0.0 {
            let s = rem.min(1.0 / 240.0);
            if line_pill {
                let (ww, hw) = if self.s.view == ViewKey::Line {
                    (LINE_SPRING_W, LINE_RISE)
                } else {
                    (LINE_DROP_WIDEN, LINE_DROP_FALL)
                };
                self.s.sw.step_fast(s, ww);
                self.s.sh.step_fast(s, hw);
                self.s.sr.step_fast(s, hw);
            } else {
                self.s.sw.step(s);
                self.s.sh.step(s);
                self.s.sr.step(s);
            }
            rem -= s;
        }
        self.s.view_age += dt;
        // closed ↔ open colors: ease toward the open colors while expanded
        let open_t = if matches!(self.s.view, ViewKey::Expanded(..)) { 1.0 } else { 0.0 };
        let open_moving = (open_t - self.s.open_k).abs() > 0.003;
        self.s.open_k = if open_moving { self.s.open_k + (open_t - self.s.open_k) * (1.0 - (-dt * 11.0).exp()) } else { open_t };
        self.gfx.set_open(self.s.open_k);
        // testing aid: with PATPET_DEMO + PATPET_DUMP=<file.bmp>, save the primary display's
        // settled frame and quit
        let dump = std::env::var("PATPET_DUMP").ok().filter(|_| self.s.hwnd == self.hwnd);
        // a resting pill where only the cat moved on: redraw just the cat
        let part = self.cat_only
            && dump.is_none()
            && matches!(self.s.view, ViewKey::Compact(_))
            && self.s.sw.done()
            && self.s.sh.done()
            && self.s.sr.done()
            && self.s.view_age > 0.35
            && self.s.slide.is_none()
            && !open_moving
            && hover_still
            && !self.s.marquee_on.get();
        if !self.s.hidden || dump.is_some() {
            let t = Instant::now();
            self.render(part);
            stats::add(&stats::RENDERS, &stats::RENDER_US, t);
        }
        if let Some(path) = dump {
            if self.s.view_age > 0.3 && self.s.slide.is_none() && self.s.sw.done() && self.s.sh.done() {
                self.gfx.dump_bmp(&path, self.s.win.2, self.s.win.3);
                crate::quit_now();
            }
        }
        let rest = self.s.sw.done()
            && self.s.sh.done()
            && self.s.sr.done()
            && self.s.view_age > 0.35
            && self.s.slide.is_none()
            && !open_moving
            && hover_still
            && !self.s.marquee_on.get();
        (rest, self.s.hidden || self.s.sh.x <= 0.5)
    }

    /// Advance the hover spring. Returns true once hover is at rest (nothing pending).
    fn step_hover(&mut self, n: Instant, dt: f32, key: ViewKey) -> bool {
        let compact = matches!(key, ViewKey::Compact(_) | ViewKey::Line) && self.leaving.is_none() && !self.s.hidden;
        // in line mode, stay open a little longer once the mouse leaves, so it does not flicker
        let out_delay = if self.data.settings.line_mode { LINE_OUT } else { HOVER_OUT };
        // just closed in line mode: the panel is still shrinking under the cursor, no hover yet
        let settling = self.s.no_hover_until.map(|t| n < t).unwrap_or(false);
        // the island's own menu took the mouse away: it stays as it is until the menu is gone
        let menu_here = self.menu_hold && self.s.hwnd == self.menu_on;
        let raw = compact && (menu_here || (!settling && (self.s.mouse.is_some() || self.drag_here())));
        // a cursor waiting out the settling time: keep frames coming so it is noticed afterwards
        let mut pending = compact && settling && self.s.mouse.is_some();
        if !compact {
            self.s.hover_on = false;
        } else if raw && !self.s.hover_on {
            // dragging files onto the island reacts at once
            let since = self.s.hover_in.map(|t| n.duration_since(t)).unwrap_or(HOVER_IN);
            if self.drag_here() || since >= HOVER_IN {
                self.s.hover_on = true;
            } else {
                pending = true;
            }
        } else if !raw && self.s.hover_on {
            let since = self.s.hover_out.map(|t| n.duration_since(t)).unwrap_or(out_delay);
            if since >= out_delay {
                self.s.hover_on = false;
            } else {
                pending = true;
            }
        }
        let t = if self.s.hover_on { 1.0 } else { 0.0 };
        let w = if self.data.settings.line_mode {
            LINE_SPRING_W
        } else if self.s.hover_on {
            HOVER_W_IN
        } else {
            HOVER_W_OUT
        };
        let mut rem = dt;
        while rem > 0.0 {
            let s = rem.min(1.0 / 240.0);
            let a = w * w * (t - self.s.hov) - 2.0 * w * self.s.hov_v;
            self.s.hov_v += a * s;
            self.s.hov += self.s.hov_v * s;
            rem -= s;
        }
        let rest = (self.s.hov - t).abs() < 0.004 && self.s.hov_v.abs() < 0.05;
        if rest {
            self.s.hov = t;
            self.s.hov_v = 0.0;
        }
        rest && !pending
    }

    /// Draw the island and show it. `cat_part`: nothing but the cat changed since the last
    /// frame, so only the cat's square is drawn and sent (when the window is still the same).
    fn render(&mut self, cat_part: bool) {
        self.sync_thumbs();
        self.sync_cat();
        let s = self.gfx.scale;
        let w = (self.s.sw.x + HOVER_W * self.s.hov).max(1.0);
        let h = (self.s.sh.x + HOVER_H * self.s.hov).max(0.0);
        // extra room on both sides for the concave "shoulders" where the notch meets the screen edge
        let wpx = ((w + 2.0 * SHOULDER) * s).ceil() as i32 + 2;
        // Line mode: an invisible strip at the very top, the line plus about 1 cm each side,
        // catches the mouse. It stays while the pill is open too: a cursor beside a narrow pill
        // that opened it is still "on" PatPet, so the pill does not close under it, bring the
        // line (and its strip) back under the cursor and open again, over and over.
        let strip = self.data.settings.line_mode
            && matches!(self.s.view, ViewKey::Line | ViewKey::Compact(_))
            && self.leaving.is_none()
            && !self.s.hidden;
        let strip_w = LINE_W + 2.0 * LINE_HIT_PAD;
        let wpx = if strip { wpx.max((strip_w * s).ceil() as i32 + 2) } else { wpx };
        let hpx = (h * s).ceil() as i32 + 1;
        let left = self.s.mon_cx - wpx / 2;
        let top = self.s.mon_top;
        self.s.screen_cx = (wpx / 2) as f32 / s;
        let part = match self.s.cat_rect.get() {
            Some(c) if cat_part && self.s.win == (left, top, wpx, hpx) => {
                // the cat sits on whole pixels and is drawn inside its square only
                let p = RECT {
                    left: ((c.x * s).floor() as i32).max(0),
                    top: ((c.y * s).floor() as i32).max(0),
                    right: ((c.right() * s).ceil() as i32).min(wpx),
                    bottom: ((c.bottom() * s).ceil() as i32).min(hpx),
                };
                (p.right > p.left && p.bottom > p.top).then_some(p)
            }
            _ => None,
        };
        if part.is_some() {
            stats::PARTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if !self.gfx.begin(wpx, hpx, part) {
            return;
        }
        // Whole pixels for the island's left and right edges: a side falling between pixels
        // is drawn half transparent, which shows as a dark line on light island colors.
        let wi = ((w * s).round() as i32).clamp(1, wpx);
        let xi = (wpx - wi) / 2;
        let island = R::new(xi as f32 / s, 0.0, wi as f32 / s, h);
        let mut ui = Ui { hits: Vec::with_capacity(32), mouse: self.s.mouse };
        if strip {
            // alpha 1/255: invisible, but the window still receives the mouse there
            let sh = (LINE_H + 1.0).min(hpx as f32 / s);
            let sx = (wpx as f32 / s - strip_w) / 2.0;
            self.gfx.rect(R::new(sx, 0.0, strip_w, sh), C(0.0, 0.0, 0.0, 1.0 / 255.0));
        }
        if let (Some(_), Some(cat)) = (part, self.s.cat_rect.get()) {
            // just the island's background behind the cat, and the cat (the hit areas stay)
            let g = &self.gfx;
            let rad = self.s.sr.x.clamp(0.0, island.h / 2.0);
            g.rrect_bg(R::new(island.x, island.y - rad, island.w, island.h + rad), rad, island.y, island.bottom());
            self.draw_cat_in(g, cat);
            ui.hits = std::mem::take(&mut self.s.hits);
        } else {
            self.s.marquee_on.set(false);
            self.draw(&self.gfx, &mut ui, island);
            if !self.s.marquee_on.get() {
                // nothing long under the mouse any more: the next one starts from its beginning
                self.s.marquee_at.set(None);
            }
        }
        let t = Instant::now();
        self.gfx.end(self.s.hwnd, left, top, wpx, hpx);
        stats::END_US.fetch_add(t.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
        self.s.win = (left, top, wpx, hpx);
        self.s.isl_px = (
            left + (island.x * s) as i32,
            top,
            (island.w * s).ceil() as i32 + 1,
            hpx,
        );
        self.s.hits = ui.hits;
    }

    fn draw(&self, g: &Gfx, ui: &mut Ui, isl: R) {
        if isl.h < 0.5 {
            return;
        }
        let rad = self.s.sr.x.clamp(0.0, isl.h / 2.0);
        // Attached to the top edge like a notch: extend the shape upward by its radius so
        // only the bottom corners are rounded, then add the concave shoulders.
        let shape = R::new(isl.x, isl.y - rad, isl.w, isl.h + rad);
        let (top, bottom) = (isl.y, isl.y + isl.h);
        g.rrect_bg(shape, rad, top, bottom);
        let f = SHOULDER.min(isl.h * 0.5);
        g.shoulder_bg(isl.x, isl.y, f, true, top, bottom);
        g.shoulder_bg(isl.right(), isl.y, f, false, top, bottom);
        // Line mode: the pill comes down out of (and goes back up into) the 5 px line, its content
        // sliding with the shape's lower edge and clipped by it, as if it was above the screen.
        let sliding = self.data.settings.line_mode
            && match self.s.view {
                ViewKey::Line => self.s.line_from_pill,
                ViewKey::Compact(_) => self.s.pill_from_line,
                _ => false,
            };
        // Whole pixels, and already home while the shape creeps its last pixel: the text must not
        // change its blur on the way or hop into place when the spring finally snaps to rest.
        let px = |v: f32| (v * self.gfx.scale).round() / self.gfx.scale;
        let drop_y = if sliding {
            px(-PILL_H * (1.0 - ((self.s.sh.x - LINE_H) / (PILL_H - 1.0 - LINE_H)).clamp(0.0, 1.0)))
        } else {
            0.0
        };
        let mut alpha = if self.s.view == ViewKey::Line {
            // nothing of the pill left to show at the line; the open island or a notice closing
            // straight to the line must not flash the pill's content on the way
            if sliding && drop_y > -PILL_H + 0.01 {
                1.0
            } else {
                0.0
            }
        } else if sliding {
            1.0
        } else if self.data.settings.line_mode && matches!(self.s.view, ViewKey::Compact(_)) {
            smooth(self.s.view_age / 0.2)
        } else {
            smooth((self.s.view_age - 0.05) / 0.22)
        };
        if let Some((_, since, _)) = self.leaving {
            // content goes first, then the black shape follows it into the edge
            alpha *= 1.0 - smooth(since.elapsed().as_secs_f32() / 0.12);
        }
        if alpha <= 0.01 {
            return;
        }
        // The size that came with `view`. Asking target() again here could answer for a newer
        // view: when the hover ends mid-frame it already says "line" while `view` is still the
        // pill, and the pill's content would be squeezed into a 5 px box for one frame (text
        // jumping up and cut off just before the island closes).
        let (tw, th) = self.s.view_wh.unwrap_or_else(|| {
            let (w, h, _, _) = self.target();
            (w, h)
        });
        let c = match self.s.view {
            ViewKey::Line => {
                // still rising from the pill: lay its content out at the pill's size
                let (live, sec) = self.lives();
                let cw = self.compact_width(live) + self.segment_width(sec);
                R::new(px(self.s.screen_cx - cw / 2.0), isl.y + drop_y, cw, PILL_H)
            }
            // the shape's width (hover) moves its center by a pixel now and then: not the content
            ViewKey::Compact(_) if self.data.settings.line_mode => R::new(px(self.s.screen_cx - tw / 2.0), isl.y + drop_y, tw, th),
            _ => R::new(isl.cx() - tw / 2.0, isl.y, tw, th),
        };
        // Open and at rest, the content sits inside the shape clear of its rounded corners, so a
        // plain rectangle clip will do: no island-sized layer bitmap (about 2 MB with lyrics).
        // While anything moves or fades, clip to the real shape.
        let at_rest = matches!(self.s.view, ViewKey::Expanded(..))
            && alpha >= 0.999
            && self.s.slide.is_none()
            && self.leaving.is_none()
            && self.s.sw.done()
            && self.s.sh.done()
            && self.s.sr.done();
        if at_rest {
            g.push_rr(shape, 0.0, 1.0);
        } else {
            g.push_rr(shape, rad, alpha);
        }
        match self.s.view {
            ViewKey::Compact(live) => self.draw_compact(g, ui, c, live),
            ViewKey::Line => self.draw_compact(g, ui, c, self.lives().0),
            ViewKey::Toast(_) => {
                if let Some(t) = &self.toast {
                    self.draw_toast(g, ui, c, t);
                }
            }
            ViewKey::Expanded(tab, lyr) => self.draw_expanded(g, ui, c, tab, lyr),
        }
        g.pop();
    }

    // -------------------------------------------------------------- compact

    fn draw_art(&self, g: &Gfx, r: R, rad: f32) {
        match self.cur_art() {
            Some(b) => g.bitmap(b, r, rad),
            None => {
                g.rrect(r, rad, SURF_H);
                g.icon(ic::MUSIC, r, (r.h * 0.45).max(9.0), T2);
            }
        }
    }

    /// Thin progress line hugging the bottom edge (clipped by the rounded corners).
    fn bottom_progress(&self, g: &Gfx, c: R, f: f32, col: C) {
        if f > 0.0 {
            g.rect(R::new(c.x, c.bottom() - 2.0, c.w * f.clamp(0.0, 1.0), 2.0), col);
        }
    }

    fn draw_eq(&self, g: &Gfx, x0: f32, cy: f32, playing: bool, acc: C) {
        let (bw, gap, maxh) = (3.0, 2.0, 13.0);
        let t = self.t0.elapsed().as_secs_f32();
        for i in 0..4 {
            let fi = i as f32;
            let h = if playing {
                let v = ((t * (5.0 + fi * 1.7) + fi * 1.3).sin() * 0.5 + 0.5) * 0.72 + 0.28;
                maxh * v
            } else {
                3.0
            };
            g.rrect(R::new(x0 + fi * (bw + gap), cy - h / 2.0, bw, h), 1.5, acc);
        }
    }

    /// Width of the secondary-activity segment at the right end of the closed island.
    fn segment_width(&self, sec: Option<Live>) -> f32 {
        let g = &self.gfx;
        match sec {
            Some(Live::Task) => {
                let due = self.task_line().map(|t| t.1).unwrap_or_default();
                12.0 + 14.0 + 5.0 + g.measure(&due, 12.0, 600) + 14.0
            }
            _ => 0.0,
        }
    }

    fn draw_segment(&self, g: &Gfx, ui: &mut Ui, r: R, sec: Live) {
        // thin divider between the two activities
        g.line(r.x, r.cy() - 8.0, r.x, r.cy() + 8.0, 1.0, WHITE.a(0.16));
        match sec {
            Live::Task => {
                let Some((_, due, now_due)) = self.task_line() else { return };
                let col = if now_due { RED } else { TASK_C };
                g.icon(ic::TASKS, R::new(r.x + 10.0, r.y, 18.0, r.h), 12.0, col);
                g.text(&due, R::new(r.x + 31.0, r.y, r.w - 31.0, r.h), 12.0, 600, col, Al::L);
                ui.hit(r, Act::Tab(Tab::Tasks));
            }
            _ => {}
        }
    }

    fn draw_compact(&self, g: &Gfx, ui: &mut Ui, full: R, live: Live) {
        // the secondary activity (if any) takes a segment at the right end; the primary gets the rest
        let sec = self.lives().1;
        let seg = self.segment_width(sec);
        if let Some(sec) = sec {
            self.draw_segment(g, ui, R::new(full.right() - seg, full.y, seg, full.h), sec);
        }
        let c = R::new(full.x, full.y, full.w - seg, full.h);
        let cs = self.cat_size();
        if live == Live::Idle {
            // idle: just the cat, sitting in the middle of the notch
            self.draw_cat(g, c.cx() - cs / 2.0, c.bottom() - 1.0);
            return;
        }
        self.draw_cat(g, c.x + 6.0, c.bottom() - 1.0);
        let c = R::new(c.x + self.cat_slot(), c.y, c.w - self.cat_slot(), c.h);
        match live {
            Live::Idle => {}
            Live::Media => {
                let Some(m) = &self.media else { return };
                let acc = self.accent();
                let s = c.h - 10.0;
                self.draw_art(g, R::new(c.x + 7.0, c.y + 5.0, s, s), 6.0);
                let eq_x = c.right() - 14.0 - 18.0;
                self.draw_eq(g, eq_x, c.cy(), m.playing, g.theme.spectrum.unwrap_or(acc));
                // clock in the middle, between the artwork and the equalizer
                let area = R::new(c.x + 7.0 + s + 6.0, c.y, (eq_x - 6.0) - (c.x + 7.0 + s + 6.0), c.h);
                g.clock(&hm(now().secs), area, CLOCK_SIZE, 600, WHITE, Al::C);
                if m.dur > 0.0 {
                    self.bottom_progress(g, full, (self.media_pos() / m.dur) as f32, g.theme.audio_bar.unwrap_or(acc).a(0.85));
                }
            }
            Live::Task => {
                let Some((text, due, now_due)) = self.task_line() else { return };
                let col = if now_due { RED } else { TASK_C };
                g.icon(ic::TASKS, R::new(c.x + 10.0, c.y, 20.0, c.h), 13.0, col);
                let dw = g.measure(&due, 12.0, 600);
                g.text(&due, R::new(c.right() - 14.0 - dw, c.y, dw + 2.0, c.h), 12.0, 600, col, Al::R);
                g.text(&text, R::new(c.x + 36.0, c.y, c.w - 36.0 - dw - 24.0, c.h), 12.5, 500, WHITE.a(0.9), Al::L);
            }
            Live::Break => {
                let left = self.breaks.rest_left(self.mono_secs()).unwrap_or(0);
                let t = format!("{}:{:02}", left / 60, left % 60);
                g.text(BREAK_LABEL, R::new(c.x + 10.0, c.y, c.w, c.h), 12.5, 600, HEART_C, Al::L);
                g.clock(&t, R::new(c.x, c.y, c.w - 14.0, c.h), CLOCK_SIZE, 600, WHITE, Al::R);
                let done = 1.0 - left as f32 / crate::breaks::REST_SECS as f32;
                self.bottom_progress(g, full, done, HEART_C.a(0.85));
            }
        }
    }

    // -------------------------------------------------------------- toast

    fn draw_ticon(&self, g: &Gfx, icon: &TIcon, r: R, acc: C) {
        match icon {
            TIcon::G(ch) => g.icon(*ch, r, 17.0, acc),
            TIcon::Txt(s) => g.text(s, r, 16.0, 700, acc, Al::C),
            TIcon::Battery(f, ch) => draw_battery(g, r.cx() - 12.0, r.cy(), 22.0, 12.0, *f, *ch),
            TIcon::Prop(i) => self.draw_prop(g, *i, R::new(r.cx() - 9.0, r.cy() - 9.0, 18.0, 18.0)),
        }
    }

    fn draw_toast(&self, g: &Gfx, ui: &mut Ui, c: R, t: &Toast) {
        if let Some(lv) = t.level {
            ui.hit(c, Act::Toast);
            g.icon(
                if let TIcon::G(ch) = t.icon { ch } else { ic::VOL },
                R::new(c.x + 12.0, c.y, 26.0, c.h),
                15.0,
                WHITE,
            );
            let bar = R::new(c.x + 46.0, c.cy() - 3.0, c.w - 46.0 - 58.0, 6.0);
            g.rrect(bar, 3.0, WHITE.a(0.16));
            // the volume bar follows theme.conf `audio_bar` (brightness keeps its own color)
            let bar_c = if t.kind == K_VOL { g.theme.audio_bar.unwrap_or(t.accent) } else { t.accent };
            g.rrect(R::new(bar.x, bar.y, (bar.w * lv.clamp(0.0, 1.0)).max(6.0), bar.h), 3.0, bar_c);
            g.text(&format!("{}%", (lv * 100.0).round() as i32), R::new(c.right() - 54.0, c.y, 40.0, c.h), 13.0, 600, WHITE, Al::R);
            if t.kind == K_VOL {
                ui.hit(R::new(bar.x - 6.0, c.y, bar.w + 12.0, c.h), Act::VolBar(bar));
            }
            return;
        }
        ui.hit(c, Act::Toast);
        if t.sub.is_empty() {
            let ir = R::new(c.x + SHORT_PAD, c.y, SHORT_ICON, c.h);
            match &t.icon {
                TIcon::Txt(s) => g.text(s, ir, 15.0, 700, t.accent, Al::C),
                other => self.draw_ticon(g, other, ir, t.accent),
            }
            let tx = ir.right() + 8.0;
            g.text(&t.title, R::new(tx, c.y, c.right() - tx - SHORT_PAD + 4.0, c.h), SHORT_SIZE, 600, WHITE, Al::L);
            return;
        }
        let rad = TWO_ICON / 2.0;
        let cx = c.x + TWO_PAD + rad;
        g.circle(cx, c.cy(), rad, t.accent.a(0.18));
        let ir = R::new(cx - rad, c.cy() - rad, TWO_ICON, TWO_ICON);
        match &t.icon {
            TIcon::G(ch) => g.icon(*ch, ir, 14.0, t.accent),
            TIcon::Txt(s) => g.text(s, ir, 12.5, 700, t.accent, Al::C),
            TIcon::Battery(f, ch) => draw_battery(g, ir.cx() - 10.0, ir.cy(), 18.0, 10.0, *f, *ch),
            TIcon::Prop(i) => self.draw_prop(g, *i, R::new(ir.cx() - 8.0, ir.cy() - 8.0, 16.0, 16.0)),
        }
        let cat = self.cat_size();
        self.draw_cat(g, c.right() - TWO_PAD - cat, c.bottom() - 4.0);
        let tx = ir.right() + TWO_GAP;
        let tw = c.right() - tx - TWO_PAD - cat - 6.0;
        let mid = c.cy();
        g.text(&t.title, R::new(tx, mid - 17.0, tw, 18.0), TWO_TITLE, 600, WHITE, Al::L);
        g.text(&t.sub, R::new(tx, mid + 1.0, tw, 16.0), TWO_SUB, 400, T2, Al::L);
    }

    // -------------------------------------------------------------- expanded

    fn draw_expanded(&self, g: &Gfx, ui: &mut Ui, c: R, tab: Tab, lyr: bool) {
        let inner = R::new(c.x + 18.0, c.y + 12.0, c.w - 36.0, c.h - 26.0);
        // tab bar
        for (i, t) in TABS.iter().enumerate() {
            let b = R::new(inner.x + i as f32 * 38.0, inner.y, 34.0, 28.0);
            let hv = ui.hit(b, Act::Tab(*t));
            let active = *t == tab;
            if active {
                g.rrect(b, 10.0, surf_active(g));
            } else if hv {
                g.rrect(b, 10.0, surf(g, true));
            }
            if *t == Tab::Pet {
                // the cat itself, sitting in its tab
                let k = if active { 1.0 } else if hv { 0.85 } else { 0.55 };
                self.draw_cat_frame(g, Mood::Idle, 0, R::new(b.cx() - 11.0, b.cy() - 12.0, 22.0, 22.0), k);
                continue;
            }
            let col = if active { active_icon(g, tab_color(*t)) } else if hv { WHITE } else { T3 };
            draw_tab_icon(g, *t, b, col);
        }
        // right: battery, date, clock
        let n = now();
        let mut x = inner.right();
        let clock = hm(n.secs);
        let cw = g.measure_clock(&clock, 13.5, 600);
        g.clock(&clock, R::new(x - cw - 2.0, inner.y, cw + 2.0, 28.0), 13.5, 600, WHITE, Al::R);
        x -= cw + 10.0;
        let date = n.date_label();
        let dw = g.measure(&date, 12.0, 500);
        g.text(&date, R::new(x - dw - 2.0, inner.y, dw + 2.0, 28.0), 12.0, 500, T2, Al::R);
        x -= dw + 16.0;
        if let Some((ac, pct)) = self.power {
            g.text(&format!("{pct}%"), R::new(x - 36.0, inner.y, 36.0, 28.0), 12.0, 500, T3, Al::R);
            x -= 42.0;
            draw_battery(g, x - 24.0, inner.y + 14.0, 22.0, 11.0, pct as f32 / 100.0, ac);
            x -= 24.0;
        }
        // the cat peeks out between the tabs and the clock; click it to pet it
        let tabs_end = inner.x + TABS.len() as f32 * 38.0 - 4.0;
        let cs = self.cat_size();
        if tab != Tab::Pet && x - tabs_end >= cs + 4.0 {
            let r = self.draw_cat(g, (tabs_end + x - cs) / 2.0, inner.y + 28.0);
            ui.hit(r, Act::Pet);
        }
        match &self.s.slide {
            None => self.draw_body(g, ui, c, tab, lyr),
            Some(sl) => {
                // Slide between features: the old page leaves to one side while the new one
                // comes in from the other. Hits are suppressed until the slide finishes.
                let p = 1.0 - (1.0 - sl.t.clamp(0.0, 1.0)).powi(3);
                // pages sit side by side (carousel), so they never overlap
                let dist = (c.w + sl.from_w) / 2.0 + 24.0;
                let mut dummy = Ui { hits: Vec::new(), mouse: None };
                let old_c = R::new(c.cx() - sl.from_w / 2.0, c.y, sl.from_w, sl.from_h);
                let all = R::new(-1e4, -1e4, 2e4, 2e4);
                g.translate(-sl.dir * dist * p, 0.0);
                g.push_rr(all, 0.0, (1.0 - p * 1.2).max(0.0));
                self.draw_body(g, &mut dummy, old_c, sl.from_tab, sl.from_lyr);
                g.pop();
                g.translate(sl.dir * dist * (1.0 - p), 0.0);
                g.push_rr(all, 0.0, (0.25 + p).min(1.0));
                self.draw_body(g, &mut dummy, c, tab, lyr);
                g.pop();
                g.translate(0.0, 0.0);
            }
        }
    }

    fn draw_body(&self, g: &Gfx, ui: &mut Ui, c: R, tab: Tab, lyr: bool) {
        let inner = R::new(c.x + 18.0, c.y + 12.0, c.w - 36.0, c.h - 26.0);
        let body = R::new(inner.x, inner.y + 42.0, inner.w, inner.h - 42.0);
        match tab {
            Tab::Pet => self.draw_pet(g, ui, body),
            Tab::Media => self.draw_media(g, ui, body, lyr),
            Tab::Shelf => self.draw_shelf(g, ui, body),
            Tab::Tasks => self.draw_tasks(g, ui, body),
            Tab::Clipboard => self.draw_clipboard(g, ui, body),
        }
    }

    fn draw_media(&self, g: &Gfx, ui: &mut Ui, c: R, split: bool) {
        let Some(m) = &self.media else {
            g.icon(ic::MUSIC, R::new(c.x, c.y + 6.0, c.w, 40.0), 26.0, T3);
            g.text("Nothing playing", R::new(c.x, c.y + 50.0, c.w, 22.0), 15.0, 600, WHITE, Al::C);
            g.text(
                "Play something in Spotify, Apple Music or your browser",
                R::new(c.x, c.y + 72.0, c.w, 18.0),
                12.0,
                400,
                T3,
                Al::C,
            );
            return;
        };
        let acc = self.accent();
        let pos = self.media_pos();
        let left_w = if split { 320.0 } else { c.w };
        let (art, col) = if split {
            (R::new(c.x, c.y, 64.0, 64.0), R::new(c.x + 78.0, c.y + 4.0, left_w - 78.0, 60.0))
        } else {
            (R::new(c.x, c.y + 2.0, 100.0, 100.0), R::new(c.x + 116.0, c.y, c.w - 116.0, c.h))
        };
        self.draw_art(g, art, if split { 12.0 } else { 16.0 });
        if !split && !m.app.is_empty() {
            g.text(&m.app, R::new(art.x, art.bottom() + 4.0, art.w, 16.0), 11.0, 500, T3, Al::C);
        }
        // title / artist + lyrics toggle
        let lb = R::new(col.right() - 32.0, col.y - 2.0, 32.0, 28.0);
        let hv = ui.hit(lb, Act::Lyrics);
        if self.lyrics_open || hv {
            g.rrect(lb, 10.0, if self.lyrics_open { g.theme.button_active.unwrap_or(acc.a(0.22)) } else { surf(g, false) });
        }
        g.icon(ic::LYRICS, lb, 14.0, if self.lyrics_open { active_icon(g, acc) } else { T2 });
        let tw = col.w - 40.0;
        g.text(&m.title, R::new(col.x, col.y, tw, 22.0), 16.0, 600, WHITE, Al::L);
        let sub = if split && !m.app.is_empty() && !m.artist.is_empty() {
            format!("{} · {}", m.artist, m.app)
        } else if m.artist.is_empty() {
            m.app.clone()
        } else {
            m.artist.clone()
        };
        g.text(&sub, R::new(col.x, col.y + 22.0, tw, 18.0), 13.0, 400, T2, Al::L);

        // progress
        let (bx, bw, by) = if split { (c.x, left_w, c.y + 84.0) } else { (col.x, col.w, col.y + 56.0) };
        let bar = R::new(bx, by, bw, 4.0);
        let hit = R::new(bx, by - 8.0, bw, 20.0);
        let hv = m.can_seek && m.dur > 0.0 && ui.hit(hit, Act::Seek(bar));
        let bar = if hv { R::new(bx, by - 1.0, bw, 6.0) } else { bar };
        g.rrect(bar, bar.h / 2.0, WHITE.a(0.15));
        let f = if m.dur > 0.0 { (pos / m.dur) as f32 } else { 0.0 };
        if f > 0.0 {
            let bc = g.theme.audio_bar.unwrap_or(acc);
            g.rrect(R::new(bar.x, bar.y, (bar.w * f).max(bar.h), bar.h), bar.h / 2.0, if hv { bc } else { bc.a(0.9) });
        }
        if hv {
            // dot marking how far the song is
            let dx = bar.x + bar.w * f.clamp(0.0, 1.0);
            g.circle(dx, bar.cy() + 0.5, 7.5, BLACK.a(0.25));
            g.circle(dx, bar.cy(), 7.0, PURE_WHITE);
            // time under the cursor, in a small dark tooltip above the bar
            if let Some((mx, _)) = ui.mouse {
                let p = ((mx - bar.x) / bar.w).clamp(0.0, 1.0) as f64 * m.dur;
                let t = mmss(p);
                let w = g.measure(&t, 11.5, 600) + 16.0;
                let tx = (mx - w / 2.0).clamp(bx - 6.0, bx + bw - w + 6.0);
                let tip = R::new(tx, by - 30.0, w, 22.0);
                g.rrect(tip, 6.0, C(0.17, 0.17, 0.18, 0.96));
                g.text(&t, tip, 11.5, 600, PURE_WHITE, Al::C);
            }
        }
        g.text(&mmss(pos), R::new(bx, by + 8.0, 60.0, 14.0), 11.0, 500, T3, Al::L);
        if m.dur > 0.0 {
            g.text(&format!("-{}", mmss(m.dur - pos)), R::new(bx + bw - 60.0, by + 8.0, 60.0, 14.0), 11.0, 500, T3, Al::R);
        }

        // controls
        let cy = by + 40.0;
        let cx = bx + bw / 2.0;
        let pr = R::new(cx - 20.0, cy - 19.0, 40.0, 38.0);
        let hv = ui.hit(pr, Act::PlayPause);
        g.circle(pr.cx(), pr.cy(), 19.0, if hv { WHITE.a(0.22) } else { WHITE.a(0.12) });
        g.icon(if m.playing { ic::PAUSE } else { ic::PLAY }, pr, 17.0, WHITE);
        icon_btn(g, ui, R::new(cx - 72.0, cy - 17.0, 40.0, 34.0), ic::PREV, 15.0, WHITE, Act::Prev, false);
        icon_btn(g, ui, R::new(cx + 32.0, cy - 17.0, 40.0, 34.0), ic::NEXT, 15.0, WHITE, Act::Next, false);

        if split {
            let dx = c.x + left_w + 18.0;
            g.line(dx, c.y + 4.0, dx, c.bottom() - 4.0, 1.0, WHITE.a(0.1));
            let area = R::new(dx + 18.0, c.y - 6.0, c.right() - dx - 18.0, c.h + 10.0);
            self.draw_lyrics(g, ui, area, acc);
        }
    }

    fn draw_lyrics(&self, g: &Gfx, ui: &mut Ui, a: R, acc: C) {
        match &self.lyr {
            Lyr::Found(v) => {
                g.push_rr(a, 0.0, 1.0);
                let cur = self.cur_line();
                let lh = 27.0;
                let cy = a.cy();
                let s = self.lyr_scroll;
                let lo = (s.floor() as i64 - 4).max(0) as usize;
                let hi = ((s.ceil() as i64 + 5).max(0) as usize).min(v.len());
                for (i, l) in v.iter().enumerate().take(hi).skip(lo) {
                    let y = cy + (i as f32 - s) * lh - lh / 2.0;
                    if y + lh < a.y || y > a.bottom() {
                        continue;
                    }
                    let dist = (i as f32 - s).abs();
                    let is_cur = Some(i) == cur;
                    let lr = R::new(a.x, y, a.w, lh);
                    let hv = ui.hit(lr, Act::LyricLine(l.t));
                    let col = if is_cur {
                        WHITE
                    } else if hv {
                        g.theme.lyrics_hover.unwrap_or(acc).a(0.9)
                    } else {
                        WHITE.a((0.42 - dist * 0.07).max(0.12))
                    };
                    let (size, wt) = if is_cur { (16.0, 700) } else { (14.0, 500) };
                    g.text(&l.text, lr, size, wt, col, Al::L);
                }
                g.pop();
            }
            Lyr::Loading | Lyr::None => {
                g.text("Searching for synced lyrics…", a, 13.0, 500, T3, Al::C);
            }
            Lyr::Missing => {
                g.text("No synced lyrics found for this track", R::new(a.x, a.cy() - 18.0, a.w, 20.0), 13.0, 600, T2, Al::C);
                g.text("Lyrics come from lrclib.net", R::new(a.x, a.cy() + 2.0, a.w, 18.0), 11.5, 400, T3, Al::C);
            }
        }
    }

    fn draw_shelf(&self, g: &Gfx, ui: &mut Ui, c: R) {
        let files = &self.data.shelf;
        if files.is_empty() {
            let r = c.inset(2.0, 2.0);
            g.rrect(r, 16.0, if self.drag_hover { SHELF_C.a(0.12) } else { WHITE.a(0.03) });
            g.rrect_stroke(r, 16.0, if self.drag_hover { SHELF_C } else { WHITE.a(0.22) }, 1.5, true);
            g.icon(ic::FOLDER, R::new(r.x, r.y + 18.0, r.w, 34.0), 24.0, if self.drag_hover { SHELF_C } else { T2 });
            g.text(
                if self.drag_hover { "Release to add to the shelf" } else { "Drop files here" },
                R::new(r.x, r.y + 56.0, r.w, 22.0),
                14.0,
                600,
                WHITE,
                Al::C,
            );
            g.text("Keep files handy, then drag them out into chats, emails or folders", R::new(r.x, r.y + 78.0, r.w, 18.0), 11.5, 400, T3, Al::C);
            return;
        }
        let (tw, th, gap) = (SHELF_TW, SHELF_TH, SHELF_GAP);
        let per = shelf_per(c.w);
        // tiles slide sideways when scrolling: draw every tile that is at least partly
        // visible, clipped to the shelf row; only fully visible tiles take clicks
        let pos = self.shelf_x.clamp(0.0, files.len().saturating_sub(per) as f32);
        let first = pos.floor() as usize;
        let row = R::new(c.x, c.y - 2.0, (per as f32) * (tw + gap) - gap, th + 4.0);
        g.push_rr(row, 14.0, 1.0);
        for (k, path) in files.iter().enumerate().skip(first).take(per + 1) {
            let off = k as f32 - pos;
            let t = R::new(c.x + off * (tw + gap), c.y, tw, th);
            let whole = off > -0.02 && off < per as f32 - 0.98;
            // fade tiles as they slide out of the row
            let fade = if whole { 1.0 } else { (1.0 - (off.min(0.0).abs()).max((off - (per as f32 - 1.0)).max(0.0))).clamp(0.0, 1.0) };
            if fade <= 0.01 {
                continue;
            }
            let layered = fade < 0.999;
            if layered {
                g.push_rr(R::new(-1e4, -1e4, 2e4, 2e4), 0.0, fade);
            }
            let hv = if whole { ui.hit(t, Act::ShelfItem(k)) } else { false };
            let sel = self.shelf_sel.contains(path);
            if sel {
                g.rrect(t, 14.0, if hv { SHELF_C.a(0.28) } else { SHELF_C.a(0.16) });
                g.rrect_stroke(t.inset(0.75, 0.75), 13.5, SHELF_C.a(0.85), 1.5, false);
            } else {
                g.rrect(t, 14.0, surf(g, hv));
            }
            let name = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(path.clone());
            let is_dir = std::path::Path::new(path).is_dir();
            let ext = std::path::Path::new(path)
                .extension()
                .map(|e| e.to_string_lossy().to_uppercase())
                .unwrap_or_default();
            let badge = R::new(t.cx() - 17.0, t.y + 10.0, 34.0, 40.0);
            if let Some(Some(b)) = self.thumbs.get(path) {
                let pr = R::new(t.x + 10.0, t.y + 8.0, t.w - 20.0, 45.0);
                g.bitmap(b, pr, 7.0);
                g.rrect_stroke(pr.inset(0.5, 0.5), 6.5, WHITE.a(0.14), 1.0, false);
                // the file type, small, in the corner
                let e: String = ext.chars().take(4).collect();
                if !e.is_empty() {
                    let lw = g.measure(&e, 8.5, 700) + 8.0;
                    let lr = R::new(pr.right() - lw - 3.0, pr.bottom() - 15.0, lw, 12.0);
                    g.rrect(lr, 4.0, ext_color(&ext).a(0.92));
                    g.text(&e, lr, 8.5, 700, WHITE, Al::C);
                }
            } else if is_dir {
                g.icon(ic::FOLDER, badge, 28.0, SHELF_C);
            } else {
                let bc = ext_color(&ext);
                g.rrect(badge, 7.0, bc.a(0.9));
                g.rrect(R::new(badge.right() - 10.0, badge.y, 10.0, 10.0), 3.0, WHITE.a(0.35));
                let e: String = ext.chars().take(4).collect();
                if e.is_empty() {
                    g.icon(ic::DOC, badge, 16.0, WHITE);
                } else {
                    g.text(&e, R::new(badge.x, badge.y + 14.0, badge.w, 20.0), 10.0, 700, WHITE, Al::C);
                }
            }
            g.text_wrap(&name, R::new(t.x + 6.0, t.y + 56.0, t.w - 12.0, 32.0), 11.0, 500, WHITE.a(0.85), Al::C);
            // selection circle: always shown in selection mode, otherwise on hover
            if hv || sel || !self.shelf_sel.is_empty() {
                let cr = R::new(t.x + 4.0, t.y + 4.0, 18.0, 18.0);
                let ch = ui.hit(cr, Act::ShelfCheck(k));
                if sel {
                    g.circle(cr.cx(), cr.cy(), 9.0, SHELF_C);
                    g.icon(ic::CHECK, cr, 9.0, BLACK);
                } else {
                    g.circle(cr.cx(), cr.cy(), 9.0, BLACK.a(0.6));
                    g.circle_stroke(cr.cx(), cr.cy(), 8.0, if ch { WHITE } else { WHITE.a(0.55) }, 1.3);
                }
            }
            if hv {
                let xr = R::new(t.right() - 22.0, t.y + 4.0, 18.0, 18.0);
                let xh = ui.hit(xr, Act::ShelfRemove(k));
                g.circle(xr.cx(), xr.cy(), 9.0, if xh { RED } else { BLACK.a(0.7) });
                g.icon(ic::CLOSE, xr, 8.0, WHITE);
            }
            if layered {
                g.pop();
            }
        }
        g.pop();
        // scroll position: a thin bar under the row when not everything fits
        if files.len() > per {
            let track = R::new(row.x, c.y + th + 3.0, row.w, 2.0);
            g.rrect(track, 1.0, WHITE.a(0.12));
            let span = files.len() as f32;
            let tw2 = track.w * per as f32 / span;
            let tx = track.x + (track.w - tw2) * (pos / (span - per as f32)).clamp(0.0, 1.0);
            g.rrect(R::new(tx, track.y, tw2, 2.0), 1.0, WHITE.a(0.55));
        }
        let fy = c.y + th + 8.0;
        let nsel = files.iter().filter(|f| self.shelf_sel.contains(f)).count();
        let info = if nsel > 0 {
            format!("{nsel} selected · drag a selected file to take them all")
        } else if files.len() > per {
            format!("{} items · scroll for more · Ctrl+click to select", files.len())
        } else {
            format!("{} item{} · click to open · Ctrl+click to select", files.len(), if files.len() == 1 { "" } else { "s" })
        };
        g.text(&info, R::new(c.x, fy, c.w - 200.0, 20.0), 11.5, 400, if nsel > 0 { SHELF_C.mix(WHITE, 0.4) } else { T3 }, Al::L);
        // press and drag this to carry every file on the shelf at once
        let dr = R::new(c.right() - 74.0 - 8.0 - 100.0, fy - 2.0, 100.0, 24.0);
        let hv = ui.hit(dr, Act::ShelfAll);
        if nsel > 0 {
            let ac = accent_btn(g, SHELF_C);
            g.rrect(dr, 12.0, if hv { ac.mix(WHITE, 0.2) } else { ac });
            g.text("Deselect", dr, 11.5, 700, accent_fg(g), Al::C);
        } else {
            g.rrect(dr, 12.0, if hv { SHELF_C.a(0.35) } else { SHELF_C.a(0.18) });
            g.text(&format!("Drag all ({})", files.len()), dr, 11.5, 600, SHELF_C.mix(WHITE, 0.35), Al::C);
        }
        let cr = R::new(c.right() - 74.0, fy - 2.0, 74.0, 24.0);
        let hv = ui.hit(cr, Act::ShelfClear);
        g.rrect(cr, 12.0, if hv { RED.a(0.25) } else { surf(g, false) });
        g.text("Clear all", cr, 11.5, 600, if hv { RED } else { btn_fg(g, T2) }, Al::C);
        if self.drag_hover {
            g.rrect_stroke(c.inset(-4.0, -4.0), 16.0, SHELF_C, 1.5, true);
        }
    }

    /// Ask for previews of the image tiles in view (and the next page, so scrolling finds
    /// them ready); forget the ones no longer on the Shelf.
    fn sync_thumbs(&mut self) {
        if !self.expanded || self.tab != Tab::Shelf {
            return;
        }
        let shelf = &self.data.shelf;
        if self.thumbs.len() > shelf.len() || self.thumbs.keys().any(|k| !shelf.contains(k)) {
            self.thumbs.retain(|k, _| shelf.contains(k));
        }
        let per = shelf_per(SHELF_BODY_W);
        let first = (self.shelf_x.max(0.0).floor() as usize).saturating_sub(per);
        for p in shelf.iter().skip(first).take(per * 3 + 1) {
            if thumbs::is_image(p) && !self.thumbs.contains_key(p) {
                self.thumbs.insert(p.clone(), None);
                thumbs::request(p);
            }
        }
    }

    /// (text, caret visible) when `f` is being edited.
    fn edit_of(&self, f: Field) -> Option<(&str, bool)> {
        let e = self.edit.as_ref().filter(|e| e.field == f)?;
        Some((e.buf.as_str(), (self.t0.elapsed().as_millis() / 530) % 2 == 0))
    }

    fn start_edit(&mut self, f: Field) {
        if self.edit.as_ref().map(|e| e.field) == Some(f) {
            return;
        }
        self.commit_edit();
        let buf = match f {
            Field::TaskTime => self.draft_due().map(hm).unwrap_or_default(),
        };
        self.edit = Some(Edit { field: f, buf, fresh: true });
        self.input_focus = false;
        unsafe {
            let _ = SetForegroundWindow(self.home);
            let _ = SetFocus(self.home);
        }
    }

    /// Apply the typed value. Invalid input leaves the old value untouched.
    fn commit_edit(&mut self) {
        let Some(e) = self.edit.take() else { return };
        let t = e.buf.trim().to_string();
        match e.field {
            Field::TaskTime => {
                if let Some(v) = parse_due_on(&t, now().secs, self.sel_day) {
                    self.task_due = v;
                    self.follow_due();
                }
            }
        }
        self.kick();
    }

    /// Furthest the shelf can scroll (first visible tile index).
    fn shelf_max(&self) -> usize {
        self.data.shelf.len().saturating_sub(shelf_per(SHELF_BODY_W))
    }

    fn toggle_sel(&mut self, i: usize) {
        if let Some(p) = self.data.shelf.get(i).cloned() {
            if let Some(pos) = self.shelf_sel.iter().position(|x| *x == p) {
                self.shelf_sel.remove(pos);
            } else {
                self.shelf_sel.push(p);
            }
        }
    }

    fn selected_paths(&self) -> Vec<String> {
        self.data.shelf.iter().filter(|p| self.shelf_sel.contains(p)).cloned().collect()
    }

    fn draw_clipboard(&self, g: &Gfx, ui: &mut Ui, c: R) {
        let n = now().secs;
        let drop = self.drag_hover && self.drag_text;
        if self.clips.is_empty() {
            if drop {
                let r = c.inset(2.0, 2.0);
                g.rrect(r, 16.0, CLIP_C.a(0.12));
                g.rrect_stroke(r, 16.0, CLIP_C, 1.5, true);
            }
            // icon 34 + gap 6 + title 22 + subtitle 18, centered in the body
            let y = c.y + ((c.h - 80.0) / 2.0).max(0.0);
            g.icon(ic::CLIP, R::new(c.x, y, c.w, 34.0), 24.0, if drop { CLIP_C } else { T3 });
            let title = if drop { "Release to save the text" } else { "Nothing copied yet" };
            g.text(title, R::new(c.x, y + 40.0, c.w, 22.0), 14.0, 600, if drop { WHITE } else { T2 }, Al::C);
            g.text("Text you copy or drop on PatPet shows up here. Click an entry to copy it again.", R::new(c.x, y + 62.0, c.w, 18.0), 11.5, 400, T3, Al::C);
            return;
        }
        // header: count + Clear all
        let count = self.clips.len();
        let cr = R::new(c.right() - 74.0, c.y, 74.0, 24.0);
        let ar = R::new(cr.x - 8.0 - 84.0, c.y, 84.0, 24.0);
        // the long header when it fits beside the buttons, the short one otherwise
        let what = if count == 1 { "entry" } else { "entries" };
        let room = ar.x - c.x - 10.0;
        let long = format!("{count} {what} · click to copy again · kept until PatPet closes");
        let head = if g.measure(&long, 11.5, 400) <= room { long } else { format!("{count} {what} · click to copy again") };
        g.text(&head, R::new(c.x + 2.0, c.y, room, 24.0), 11.5, 400, T3, Al::L);
        let done = self.clip_all_at.map(|t| t.elapsed() < Duration::from_millis(1400)).unwrap_or(false);
        let hv = ui.hit(ar, Act::ClipCopyAll);
        g.rrect(ar, 12.0, if hv || done { CLIP_C.a(0.3) } else { CLIP_C.a(0.16) });
        if done {
            let tw = g.measure("Copied", 11.5, 600);
            let x = ar.cx() - (tw + 14.0) / 2.0;
            g.icon(ic::CHECK, R::new(x, ar.y, 10.0, ar.h), 10.0, CLIP_C.mix(WHITE, 0.35));
            g.text("Copied", R::new(x + 14.0, ar.y, tw + 2.0, ar.h), 11.5, 600, CLIP_C.mix(WHITE, 0.35), Al::L);
        } else {
            g.text("Copy all", ar, 11.5, 600, CLIP_C.mix(WHITE, 0.35), Al::C);
        }
        let hv = ui.hit(cr, Act::ClipClear);
        g.rrect(cr, 12.0, if hv { RED.a(0.25) } else { surf(g, false) });
        g.text("Clear all", cr, 11.5, 600, if hv { RED } else { btn_fg(g, T2) }, Al::C);

        // list
        let ly = c.y + 32.0;
        let rh = 30.0;
        let rows = ((c.bottom() - ly) / rh).floor().max(1.0) as usize;
        let start = self.clip_scroll.min(count.saturating_sub(rows));
        let copied = self.clip_copied.as_ref().filter(|(t, _)| t.elapsed() < Duration::from_millis(1400)).map(|(_, s)| s.as_str());
        let dropped = self.clip_dropped.as_ref().filter(|(t, _)| t.elapsed() < Duration::from_millis(1400)).map(|(_, s)| s.as_str());
        for (k, item) in self.clips.iter().enumerate().skip(start).take(rows) {
            let r = R::new(c.x, ly + (k - start) as f32 * rh, c.w, rh - 2.0);
            // the whole row copies; the x on top of it removes (registered last, so it wins)
            let hv = ui.hit(r, Act::ClipCopy(k));
            g.rrect(r, 9.0, if hv { WHITE.a(0.08) } else { WHITE.a(0.03) });
            let right = 64.0;
            // keyed by position, with a high bit so it never matches a task id
            self.marquee_text(g, (1 << 62) | k as u64, hv, &clip::preview(&item.text), R::new(r.x + 12.0, r.y, r.w - 12.0 - right, r.h), 13.0, 400, WHITE);
            let mark = if copied == Some(item.text.as_str()) {
                Some("Copied")
            } else if dropped == Some(item.text.as_str()) {
                Some("Saved")
            } else {
                None
            };
            if let Some(m) = mark {
                g.icon(ic::CHECK, R::new(r.right() - 62.0, r.y, 16.0, r.h), 10.0, CLIP_C);
                g.text(m, R::new(r.right() - 46.0, r.y, 42.0, r.h), 11.5, 600, CLIP_C, Al::L);
            } else if hv {
                let xr = R::new(r.right() - 30.0, r.y + 2.0, 26.0, r.h - 4.0);
                let xh = ui.hit(xr, Act::ClipDel(k));
                if xh {
                    g.rrect(xr, 8.0, RED.a(0.2));
                }
                g.icon(ic::CLOSE, xr, 9.0, if xh { RED } else { T2 });
            } else {
                g.text(&ago(n - item.at), R::new(r.right() - right, r.y, right - 10.0, r.h), 11.5, 500, T3, Al::R);
            }
        }
        // a hint that there is more below / above
        if count > rows {
            let shown = format!("{}-{} of {count}", start + 1, (start + rows).min(count));
            g.text(&shown, R::new(c.x, c.bottom() - 2.0, c.w - 4.0, 14.0), 10.5, 500, T3, Al::R);
        }
        if drop {
            g.rrect_stroke(c.inset(-4.0, -4.0), 16.0, CLIP_C, 1.5, true);
        }
    }

    /// One line of text in `r`, cut with "…" as usual. Under the mouse, a line too long to fit
    /// slides left until its end shows, waits, and slides back. Returns how far it is slid.
    #[allow(clippy::too_many_arguments)]
    fn marquee_text(&self, g: &Gfx, key: u64, hover: bool, s: &str, r: R, size: f32, weight: u32, c: C) -> f32 {
        let tw = g.measure(s, size, weight);
        let over = tw - r.w;
        if !hover || over <= 1.0 {
            g.text(s, r, size, weight, c, Al::L);
            return 0.0;
        }
        let now = Instant::now();
        let start = match self.s.marquee_at.get() {
            Some((k, t)) if k == key => t,
            _ => {
                self.s.marquee_at.set(Some((key, now)));
                now
            }
        };
        self.s.marquee_on.set(true);
        // a short wait first (just passing by doesn't start it), ~45 px a second, a pause at the end
        const WAIT: f32 = 0.6;
        const HOLD: f32 = 1.4;
        let dist = over + 8.0;
        let go = (dist / 45.0).max(0.5);
        let t = now.duration_since(start).as_secs_f32() % (WAIT + go + HOLD + go);
        let f = if t < WAIT {
            0.0
        } else if t < WAIT + go {
            smooth((t - WAIT) / go)
        } else if t < WAIT + go + HOLD {
            1.0
        } else {
            1.0 - smooth((t - WAIT - go - HOLD) / go)
        };
        if f <= 0.0 {
            g.text(s, r, size, weight, c, Al::L);
            return 0.0;
        }
        let off = dist * f;
        g.push_rr(r, 0.0, 1.0);
        g.text(s, R::new(r.x - off, r.y, tw + 20.0, r.h), size, weight, c, Al::L);
        g.pop();
        off
    }

    /// Tasks shown for the picked day: the ones due that day; on today also the ones without
    /// a time and the unfinished ones from earlier days.
    fn day_tasks(&self) -> Vec<&Task> {
        let today = now().day();
        let sel = self.sel_day;
        let mut v: Vec<&Task> = self
            .data
            .tasks
            .iter()
            .filter(|t| match t.due {
                Some(d) => {
                    let dd = d.div_euclid(86400);
                    dd == sel || (sel == today && dd < today && !t.done)
                }
                None => sel == today,
            })
            .collect();
        v.sort_by_key(|t| (t.done, t.due.is_none(), t.due.unwrap_or(0), t.id));
        v
    }

    fn draw_calendar(&self, g: &Gfx, ui: &mut Ui, r: R, today: i64, now_s: i64) {
        let (y, m) = self.cal_month;
        // header: month (click: back to today) and the arrows
        let title = format!("{} {}", MONTHS[(m - 1).clamp(0, 11) as usize], y);
        let tw = g.measure(&title, 12.5, 600);
        let tr = R::new(r.x, r.y, tw + 10.0, 22.0);
        let th = ui.hit(tr, Act::CalToday);
        g.text(&title, R::new(r.x + 4.0, r.y, tw + 4.0, 22.0), 12.5, 600, if th { TASK_C } else { WHITE }, Al::L);
        let nr = R::new(r.right() - 22.0, r.y, 22.0, 22.0);
        let pr = R::new(nr.x - 24.0, r.y, 22.0, 22.0);
        icon_btn(g, ui, pr, ic::LEFT, 9.0, T2, Act::CalMonth(-1), false);
        icon_btn(g, ui, nr, ic::RIGHT, 9.0, T2, Act::CalMonth(1), false);
        // weekdays, Monday first
        let cw = r.w / 7.0;
        let wy = r.y + 25.0;
        for (i, d) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
            g.text(d, R::new(r.x + i as f32 * cw, wy, cw, 12.0), 9.5, 600, T3, Al::C);
        }
        // which days have tasks: (unfinished, overdue)
        let mut marks: HashMap<i64, (bool, bool)> = HashMap::new();
        for t in &self.data.tasks {
            if let Some(d) = t.due {
                let e = marks.entry(d.div_euclid(86400)).or_insert((false, false));
                if !t.done {
                    e.0 = true;
                    e.1 |= d <= now_s;
                }
            }
        }
        // the days: always 6 full rows (42 days), the gaps filled with the days before and
        // after, so every month looks the same and the grid never jumps
        let first = days_from_civil(y, m, 1);
        let start = first - weekday(first);
        let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        let next = days_from_civil(ny, nm, 1);
        let gy = wy + 14.0;
        let rh = ((r.bottom() - gy) / 6.0).min(30.0);
        let rad = (rh / 2.0 - 1.0).min(cw / 2.0 - 1.5);
        for k in 0..42 {
            let day = start + k;
            let cell = R::new(r.x + (k % 7) as f32 * cw, gy + (k / 7) as f32 * rh, cw, rh);
            let (cx, cy) = (cell.cx(), cell.cy());
            let hv = ui.hit(cell, Act::CalDay(day));
            let sel = day == self.sel_day;
            let in_month = day >= first && day < next;
            if sel {
                g.circle(cx, cy, rad, accent_btn(g, TASK_C));
            } else if hv {
                g.circle(cx, cy, rad, surf(g, true));
            }
            if day == today && !sel {
                g.circle_stroke(cx, cy, rad - 0.6, TASK_C.a(0.85), 1.2);
            }
            let fg = if sel {
                accent_fg(g)
            } else if day == today {
                TASK_C
            } else if !in_month {
                WHITE.a(0.22)
            } else if day < today {
                T2
            } else {
                WHITE
            };
            let (_, _, dn) = civil_from_days(day);
            let bold = if sel || day == today { 700 } else { 500 };
            let nr = R::new(cell.x, cy - 9.0, cell.w, 14.0);
            if !in_month && !sel && day != today {
                // neighbouring months stay clearly fainter (not lifted by text_contrast)
                g.text_dim(&dn.to_string(), nr, 11.0, 500, fg, Al::C);
            } else {
                g.text(&dn.to_string(), nr, 11.0, bold, fg, Al::C);
            }
            if let Some(&(open, over)) = marks.get(&day) {
                let col = if sel {
                    accent_fg(g)
                } else if over {
                    RED
                } else if open {
                    TASK_C
                } else {
                    T3
                };
                g.circle(cx, cy + rad - 3.6, 1.6, if in_month || sel { col } else { col.a(col.3 * 0.5) });
            }
        }
    }

    fn draw_tasks(&self, g: &Gfx, ui: &mut Ui, c: R) {
        let n = now();
        let today = n.day();
        // calendar on the left, the picked day on the right
        let cal = R::new(c.x, c.y, 196.0, c.h);
        self.draw_calendar(g, ui, cal, today, n.secs);
        g.line(cal.right() + 10.0, c.y + 2.0, cal.right() + 10.0, c.bottom() - 2.0, 1.0, WHITE.a(0.1));
        let c = R::new(cal.right() + 20.0, c.y, c.right() - cal.right() - 20.0, c.h);

        // input row: the text and the add button
        let field = R::new(c.x, c.y, c.w - 32.0 - 6.0, 30.0);
        let hv = ui.hit(field, Act::TaskField);
        g.rrect(field, 10.0, if self.input_focus { WHITE.a(0.12) } else { surf(g, hv) });
        if self.input_focus {
            g.rrect_stroke(field, 10.0, TASK_C.a(0.6), 1.2, false);
        }
        let inner = field.inset(12.0, 0.0);
        if self.input.is_empty() && !self.input_focus {
            g.text("Add a task…", inner, 13.0, 400, T3, Al::L);
        } else {
            let tw = g.measure(&self.input, 13.0, 400);
            let off = (tw - inner.w + 4.0).max(0.0);
            g.push_rr(inner, 0.0, 1.0);
            g.text(&self.input, R::new(inner.x - off, inner.y, tw + 50.0, inner.h), 13.0, 400, WHITE, Al::L);
            if self.input_focus && (self.t0.elapsed().as_millis() / 530) % 2 == 0 {
                let cx = inner.x - off + tw + 1.0;
                g.line(cx, inner.y + 8.0, cx, inner.bottom() - 8.0, 1.5, TASK_C);
            }
            g.pop();
        }
        let ar = R::new(c.right() - 32.0, c.y, 32.0, 30.0);
        let ah = ui.hit(ar, Act::TaskAdd);
        let ac = accent_btn(g, TASK_C);
        g.rrect(ar, 10.0, if ah { ac } else { ac.a(0.85) });
        g.icon(ic::ADD, ar, 12.0, accent_fg(g));

        // second row: the picked day, and the time a new task gets on it
        let hy = c.y + 36.0;
        let tr = R::new(c.right() - 96.0, hy, 96.0, 26.0);
        g.rrect(tr, 9.0, surf(g, false));
        let mr = R::new(tr.x, tr.y, 24.0, 26.0);
        let pr = R::new(tr.right() - 24.0, tr.y, 24.0, 26.0);
        let mid = R::new(tr.x + 24.0, tr.y, tr.w - 48.0, 26.0);
        let mh = ui.hit(mr, Act::TaskTime(-900));
        let ph = ui.hit(pr, Act::TaskTime(900));
        let th = ui.hit(mid, Act::Edit(Field::TaskTime));
        g.icon(ic::MINUS, mr, 9.0, if mh { WHITE } else { T2 });
        g.icon(ic::ADD, pr, 9.0, if ph { WHITE } else { T2 });
        match (self.edit_of(Field::TaskTime), self.draft_due()) {
            (Some(e), _) => value_box(g, mid, "", Some(e), false, 11.5),
            (None, Some(d)) => {
                let label = if d.div_euclid(86400) == self.sel_day { hm(d) } else { due_label(d, today) };
                g.text(&label, mid, 11.5, 600, if th { WHITE } else { TASK_C }, Al::C);
            }
            (None, None) => g.text("No time", mid, 11.0, 500, if th { WHITE } else { T3 }, Al::C),
        }
        let sel = self.sel_day;
        let head = match sel - today {
            0 => format!("Today · {}", day_label(sel)),
            1 => format!("Tomorrow · {}", day_label(sel)),
            -1 => format!("Yesterday · {}", day_label(sel)),
            _ => {
                let (y, _, _) = civil_from_days(sel);
                if y == n.y {
                    day_label(sel)
                } else {
                    format!("{} {y}", day_label(sel))
                }
            }
        };
        let tasks = self.day_tasks();
        g.text(&head, R::new(c.x + 2.0, hy, tr.x - c.x - 8.0, 26.0), 11.5, 600, T2, Al::L);

        // list
        let ly = c.y + 68.0;
        let rh = 27.0;
        let rows = ((c.bottom() - ly) / rh).floor().max(1.0) as usize;
        if tasks.is_empty() {
            let y = ly + ((c.bottom() - ly - 40.0) / 2.0).max(0.0);
            let (title, hint) = if sel < today {
                ("Nothing was planned", "Pick today or a later day to add tasks.")
            } else if sel == today {
                ("No tasks for today", "Type a task, pick a time, press Enter.")
            } else {
                ("Nothing planned yet", "Tasks added now go on this day.")
            };
            g.text(title, R::new(c.x, y, c.w, 22.0), 13.5, 600, T2, Al::C);
            g.text(hint, R::new(c.x, y + 22.0, c.w, 18.0), 11.0, 400, T3, Al::C);
            return;
        }
        let start = self.task_scroll.min(tasks.len().saturating_sub(rows));
        for (i, t) in tasks.iter().skip(start).take(rows).enumerate() {
            let r = R::new(c.x, ly + i as f32 * rh, c.w, rh - 2.0);
            let hv = ui.hover(r);
            if hv {
                g.rrect(r, 9.0, WHITE.a(0.05));
            }
            let cb = R::new(r.x + 2.0, r.y, 28.0, r.h);
            let ch = ui.hit(cb, Act::TaskToggle(t.id));
            g.icon(if t.done { ic::BOX_ON } else { ic::BOX }, cb, 15.0, if t.done { TASK_C } else if ch { WHITE } else { T2 });
            let overdue = !t.done && t.due.map(|d| d <= n.secs).unwrap_or(false);
            let label = t.due.map(|d| if d.div_euclid(86400) == sel { hm(d) } else { due_label(d, today) });
            let due_w = label.as_ref().map(|l| g.measure(l, 12.0, 600) + 6.0).unwrap_or(0.0);
            let txt = R::new(r.x + 36.0, r.y, r.w - 36.0 - due_w - 30.0, r.h);
            let off = self.marquee_text(g, t.id, hv, &t.text, txt, 13.0, 400, if t.done { T3 } else { WHITE });
            if t.done {
                let tw = g.measure(&t.text, 13.0, 400);
                let (x0, x1) = ((txt.x - off).max(txt.x), (txt.x - off + tw).min(txt.right()));
                g.line(x0, r.cy(), x1, r.cy(), 1.2, T3);
            }
            if let Some(l) = &label {
                g.text(l, R::new(r.right() - due_w - 28.0, r.y, due_w, r.h), 12.0, 600, if t.done { T3 } else if overdue { RED } else { TASK_C }, Al::R);
            }
            if hv {
                let xr = R::new(r.right() - 26.0, r.y + 3.0, 24.0, r.h - 6.0);
                let xh = ui.hit(xr, Act::TaskDel(t.id));
                g.icon(ic::CLOSE, xr, 10.0, if xh { RED } else { T3 });
            }
        }
        if tasks.len() > rows {
            g.text(&format!("{}–{} of {}", start + 1, (start + rows).min(tasks.len()), tasks.len()), R::new(c.x, c.bottom() - 2.0, c.w, 14.0), 10.0, 500, T3, Al::R);
        }
    }

    // -------------------------------------------------------------- actions

    fn cursor_inside(&self) -> bool {
        cursor_in(self.s.isl_px)
    }

    /// The cursor is over the island the panel is on.
    fn home_inside(&self) -> bool {
        self.surfaces().find(|o| o.hwnd == self.home).map(|o| cursor_in(o.isl_px)).unwrap_or(false)
    }

    fn any_inside(&self) -> bool {
        self.surfaces().any(|o| cursor_in(o.isl_px))
    }

    /// Whether the cursor is really over this window (not over its see-through pixels).
    fn cursor_on_window(&self) -> bool {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
            WindowFromPoint(p) == self.s.hwnd
        }
    }

    fn hit_at(&self, p: (f32, f32)) -> Option<Act> {
        self.s.hits.iter().rev().find(|(r, _)| r.contains(p)).map(|(_, a)| *a)
    }

    fn act(&mut self, a: Act, mx: f32) {
        if let Some(e) = &self.edit {
            if a != Act::Edit(e.field) {
                self.commit_edit();
            }
        }
        if !matches!(a, Act::TaskField | Act::TaskTime(_) | Act::Edit(Field::TaskTime) | Act::TaskAdd | Act::CalMonth(_) | Act::CalToday | Act::CalDay(_)) {
            self.input_focus = false;
        }
        match a {
            Act::Tab(t) => {
                self.shelf_sel.clear();
                self.tab = t;
                if t == Tab::Media && self.lyrics_open {
                    self.ensure_lyrics();
                }
            }
            Act::PlayPause => {
                self.send(MediaCmd::PlayPause);
                let p = self.media_pos();
                if let Some(m) = &mut self.media {
                    m.pos = p;
                    m.at = Instant::now();
                    m.playing = !m.playing;
                    self.pending_play = Some((m.playing, Instant::now()));
                }
            }
            Act::Prev => self.send(MediaCmd::Prev),
            Act::Next => self.send(MediaCmd::Next),
            Act::Seek(r) => {
                let f = ((mx - r.x) / r.w).clamp(0.0, 1.0) as f64;
                if let Some(m) = &mut self.media {
                    let t = f * m.dur;
                    m.pos = t;
                    m.at = Instant::now();
                    let _ = self.media_tx.send(MediaCmd::Seek(t));
                }
            }
            Act::Lyrics => {
                self.lyrics_open = !self.lyrics_open;
                if self.lyrics_open {
                    self.ensure_lyrics();
                    if let Some(c) = self.cur_line() {
                        self.lyr_scroll = c as f32;
                    }
                }
            }
            Act::LyricLine(t) => {
                if let Some(m) = &mut self.media {
                    m.pos = t;
                    m.at = Instant::now();
                    let _ = self.media_tx.send(MediaCmd::Seek(t));
                }
            }
            Act::ShelfItem(i) => {
                // Ctrl+click, or any click while something is selected, toggles selection
                let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                if ctrl || !self.shelf_sel.is_empty() {
                    self.toggle_sel(i);
                } else if let Some(p) = self.data.shelf.get(i) {
                    shelf::open(p);
                }
            }
            Act::ShelfRemove(i) => {
                if let Some(p) = self.data.shelf.get(i) {
                    let p = p.clone();
                    self.shelf_sel.retain(|x| *x != p);
                }
                if i < self.data.shelf.len() {
                    self.data.shelf.remove(i);
                    self.dirty = true;
                }
            }
            // click selects/deselects everything; pressing and dragging it starts the drag directly
            Act::ShelfAll => {
                if self.shelf_sel.is_empty() {
                    self.shelf_sel = self.data.shelf.clone();
                } else {
                    self.shelf_sel.clear();
                }
            }
            Act::ShelfCheck(i) => self.toggle_sel(i),
            Act::ClipCopy(i) => {
                // copy it again but leave the list as it is: the entry keeps its place
                if let Some(t) = self.clips.get(i).map(|c| c.text.clone()) {
                    if clip::write(self.hwnd, &t) {
                        self.clip_ignore = Some(t.clone());
                        self.clip_copied = Some((Instant::now(), t));
                    }
                }
            }
            Act::ClipDel(i) => {
                if i < self.clips.len() {
                    self.clips.remove(i);
                }
            }
            // every entry, oldest first (the order they were copied in), one per line
            Act::ClipCopyAll => {
                if !self.clips.is_empty() {
                    let all = self.clips.iter().rev().map(|c| c.text.trim_end()).collect::<Vec<_>>().join("\r\n");
                    if clip::write(self.hwnd, &all) {
                        // the combined text must not come back as a new entry
                        self.clip_ignore = Some(all);
                        self.clip_all_at = Some(Instant::now());
                    }
                }
            }
            Act::ClipClear => {
                self.clips.clear();
                self.clip_scroll = 0;
            }
            Act::ShelfClear => {
                self.shelf_sel.clear();
                self.data.shelf.clear();
                self.shelf_scroll = 0;
                self.dirty = true;
            }
            Act::TaskField => {
                self.input_focus = true;
                unsafe {
                    let _ = SetForegroundWindow(self.home);
                    let _ = SetFocus(self.home);
                }
            }
            Act::TaskTime(d) => self.adjust_due(d),
            Act::Edit(f) => self.start_edit(f),
            Act::TaskAdd => self.add_task(),
            Act::TaskToggle(id) => {
                let mut done = (false, false);
                if let Some(t) = self.data.tasks.iter_mut().find(|t| t.id == id) {
                    done = t.toggle();
                    self.dirty = true;
                }
                if done.0 {
                    self.cheer();
                }
                if done.1 {
                    let ups = self.data.pet.task_done(now().day());
                    self.on_levels(ups);
                }
                let p = &self.data.pet;
                dbg_log(&format!(
                    "task {id}: done={} first={} -> level {} xp {} ({} tasks rewarded today)",
                    done.0, done.1, p.level, p.xp, p.tasks_today
                ));
            }
            Act::Pet => self.pet_cat(),
            Act::Care(need) => self.care_for(need),
            Act::Room(step) => {
                self.data.pet.next_room(step);
                self.dirty = true;
            }
            Act::TaskDel(id) => {
                self.data.tasks.retain(|t| t.id != id);
                self.dirty = true;
            }
            Act::CalMonth(d) => self.shift_month(d),
            Act::CalToday => self.select_day(now().day()),
            Act::CalDay(day) => self.select_day(day),
            Act::Toast => {
                if let Some(t) = self.toast.take() {
                    if !self.toast_tapped(t.tap) {
                        if let Some(tab) = t.tab {
                            self.expand(Some(tab));
                        }
                    }
                }
            }
            Act::VolBar(r) => {
                let f = ((mx - r.x) / r.w).clamp(0.0, 1.0);
                if let Some(a) = &self.audio {
                    a.set(f);
                }
            }
        }
        self.kick();
    }

    fn adjust_due(&mut self, d: i64) {
        let n = now().secs;
        let round_up = |s: i64, step: i64| (s / step + 1) * step;
        let mut v = match self.draft_due() {
            Some(t) => t + d,
            None => round_up(n + 600, 900),
        };
        if v <= n {
            v = round_up(n, 300);
        }
        self.task_due = Some(v);
        self.follow_due();
    }

    /// The time a new task gets: the one picked, or 09:00 on a later day picked in the calendar.
    fn draft_due(&self) -> Option<i64> {
        match self.task_due {
            Some(t) => Some(t),
            None if self.sel_day > now().day() => Some(self.sel_day * 86400 + DEFAULT_TIME),
            None => None,
        }
    }

    /// The picked time moved to another day (+ past midnight, "30m" late in the evening):
    /// the calendar goes along.
    fn follow_due(&mut self) {
        if let Some(t) = self.task_due {
            let day = t.div_euclid(86400);
            if day != self.sel_day {
                self.sel_day = day;
                let (y, m, _) = civil_from_days(day);
                self.cal_month = (y, m);
                self.task_scroll = 0;
            }
        }
    }

    /// Pick a day in the calendar. A time already picked keeps its hour on the new day.
    fn select_day(&mut self, day: i64) {
        let n = now().secs;
        self.sel_day = day;
        let (y, m, _) = civil_from_days(day);
        self.cal_month = (y, m);
        self.task_scroll = 0;
        if let Some(t) = self.task_due {
            let v = day * 86400 + t.rem_euclid(86400);
            self.task_due = (v > n).then_some(v);
        }
    }

    fn shift_month(&mut self, d: i32) {
        let (y, m) = self.cal_month;
        let k = y * 12 + (m - 1) + d as i64;
        self.cal_month = (k.div_euclid(12), k.rem_euclid(12) + 1);
    }

    fn add_task(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            self.input_focus = true;
            return;
        }
        let n = now();
        let due = self.draft_due();
        self.data.next_id += 1;
        self.data.tasks.push(Task {
            id: self.data.next_id,
            text,
            due,
            done: false,
            notified: false,
            alerts: passed_alerts(due, n.secs),
            rewarded: false,
        });
        self.input.clear();
        // on a later day the time stays, for adding more tasks to that day
        if self.sel_day <= n.day() {
            self.task_due = None;
        }
        self.dirty = true;
    }

    // -------------------------------------------------------------- periodic

    pub fn tick(&mut self) {
        let n = Instant::now();
        let mut redraw = false;

        if let Some(at) = self.clip_due {
            if n >= at {
                self.clip_due = None;
                self.read_clipboard();
            }
        }

        // new screenshots -> Shelf (one cheap folder check every 2 s)
        if self.data.settings.screenshots && n >= self.shots_next {
            self.shots_next = n + Duration::from_secs(2);
            let new: Vec<String> = self.shots.poll().into_iter().filter(|p| !self.data.shelf.contains(p)).collect();
            if !new.is_empty() {
                let count = new.len();
                self.data.shelf.extend(new);
                self.shelf_scroll = self.shelf_max();
                self.dirty = true;
                let title = if count == 1 { "Screenshot added to Shelf".to_string() } else { format!("{count} screenshots added to Shelf") };
                self.toast(TIcon::G(ic::FOLDER), &title, "Drag it anywhere from the Shelf", 4.0, SHELF_C, Some(Tab::Shelf));
                redraw = true;
            }
        }

        if let Some(t) = &self.toast {
            if n >= t.until && !self.any_inside() {
                self.toast = None;
                redraw = true;
            }
        }
        // once the island is small again, hand buffers and idle pages back to Windows
        let small = !self.expanded && self.toast.is_none() && self.leaving.is_none();
        if !small {
            self.trim_at = None;
        } else if self.trim_at.is_none() && { let (w, h) = self.biggest_win(); self.gfx.oversized(w, h) } {
            self.trim_at = Some(n + Duration::from_secs(3));
        }
        if let Some(at) = self.trim_at {
            if small && !self.anim_on && n >= at {
                self.trim_at = None;
                let (w, h) = self.biggest_win();
                if w > 0 && h > 0 {
                    self.gfx.trim(w, h);
                }
                // previews come back (decoded again) the next time the Shelf opens
                self.thumbs.clear();
                self.room_bmp = None;
                crate::util::trim_memory();
                dbg_log("memory: trimmed");
                if std::env::var_os("PATPET_DEBUG").is_some() {
                    dbg_log(&crate::util::heap_stats());
                }
            }
        }

        if let Some((at, k)) = self.demo_stress {
            // PATPET_STRESS_N: stop (closed) after that many steps, to watch memory settle
            let max = std::env::var("PATPET_STRESS_N").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(u32::MAX);
            if k >= max && n >= at {
                self.demo_stress = None;
                dbg_log(&format!("stress done: {}", crate::util::heap_stats()));
                self.kick();
            } else if n >= at {
                // PATPET_STRESS=fetch: only fetch lyrics again; =toggle: only close / reopen;
                // otherwise both (odd steps close, even steps reopen and fetch anew)
                let mode = std::env::var("PATPET_STRESS").unwrap_or_default();
                if mode == "fetch" {
                    self.lyr_key = 0;
                    self.ensure_lyrics();
                } else if k % 2 == 0 {
                    self.collapse();
                } else {
                    self.lyrics_open = true;
                    if mode != "toggle" {
                        self.lyr_key = 0;
                    }
                    self.expand(Some(Tab::Media));
                    self.ensure_lyrics();
                }
                let ms = std::env::var("PATPET_STRESS_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
                self.demo_stress = Some((n + Duration::from_millis(ms), k + 1));
                if k % 10 == 0 {
                    dbg_log(&format!("stress {k}: {}", crate::util::heap_stats()));
                }
                self.kick();
            }
        }

        if let Some(at) = self.demo_switch {
            if n >= at {
                self.demo_switch = None;
                self.tab = Tab::Tasks;
                self.kick();
            }
        }

        // keyboard layout
        let l = system::fg_lang();
        if l != 0 && l != self.lang {
            let first = self.lang == 0;
            self.lang = l;
            if !first {
                let (name, short) = system::lang_name(l);
                self.toast(TIcon::Txt(short), &name, "Keyboard language", 2.2, WHITE, None);
            }
        }

        if n.duration_since(self.last_sec) >= Duration::from_secs(1) {
            self.last_sec = n;
            self.per_second();
        }

        if let Some(at) = self.net_check {
            if n >= at {
                self.net_check = None;
                let (ok, name, wifi) = system::net_status();
                if self.net != Some(ok) {
                    self.net = Some(ok);
                    if ok {
                        let ch = if wifi { ic::WIFI } else { ic::ETHERNET };
                        let title = if wifi { "Wi-Fi connected" } else { "Connected" };
                        self.toast(TIcon::G(ch), title, &name, 3.0, GREEN, None);
                    } else {
                        self.toast(TIcon::G(ic::WIFI), "Connection lost", "You're offline", 4.0, RED, None);
                    }
                }
            }
        }

        // each island: hover lost or found behind the mouse's back
        let mut found = false;
        self.each(|a| found |= a.check_hover(n));
        redraw |= found;

        // auto-collapse when the island is open but the user went elsewhere
        if self.expanded && !self.demo && !self.menu_hold {
            self.arm_auto_close(n);
            if let Some(at) = self.collapse_at {
                if n >= at && !self.home_inside() && !self.drag_hover {
                    self.collapse();
                }
            }
        }

        let playing = self.media.as_ref().map(|m| m.playing).unwrap_or(false);
        let compact = !self.expanded && self.toast.is_none();
        // redraw when the closed island should switch activity (e.g. a task became due soon)
        // or, for the idle clock, when the minute rolls over
        let minute = now().secs / 60;
        let mut moved_on = false;
        if compact {
            self.each(|a| moved_on |= a.target().3 != a.s.view);
        }
        if minute != self.last_min || moved_on {
            self.last_min = minute;
            redraw = true;
        }
        let periodic = (compact && (playing || self.task_soon().is_some() || self.breaks.resting()))
            || (self.expanded
                && match self.tab {
                    Tab::Media => playing,
                    Tab::Tasks => self.input_focus || self.edit.is_some(),
                    _ if self.edit.is_some() => true,
                    _ => false,
                });
        let fast = (compact && playing) || (self.expanded && self.tab == Tab::Media && self.lyrics_open && playing);
        // the cat: what it is up to, and a redraw whenever its animation moves on a frame
        self.pet.set_music(playing, n);
        // (demo views show the cat awake, however long the mouse has been still)
        let idle = if self.demo { Duration::ZERO } else { system::user_idle() };
        self.pet.tick(n, idle, now().h as u32);
        self.wander(n);
        let cat_shown = self.cat_visible();
        let others = redraw || periodic;
        let cat_moved = cat_shown && self.pet.frame(n) != self.cat_frame;
        redraw |= cat_moved;
        // quicker while a panel is about to close, so it closes on time
        let mut ms = if fast || (self.expanded && self.collapse_at.is_some()) { 90 } else { 250 };
        if cat_shown {
            ms = ms.min(self.pet.frame_period().as_millis() as u32).max(60);
        }
        self.set_tick(ms);
        if (periodic || redraw) && !self.all_hidden() {
            // only the cat moved on a frame: resting islands just redraw the cat
            let cat_only = cat_moved && !others && !self.anim_on && !self.hearts_on();
            if cat_only {
                // one frame right away: no need to wake the refresh-rate pacing for it
                self.cat_only = true;
                self.last_frame = Instant::now();
                if !self.frame() {
                    self.kick();
                }
            } else {
                self.kick();
            }
        }
    }

    fn per_second(&mut self) {
        stats::report();
        // hide during full-screen apps / presentations; reminders wait until it is back
        self.apply_visibility();
        let t = now();

        // the cat's needs run down; a gentle notice when one is running low
        self.sync_care();
        self.need_notice();
        self.break_tick();

        // task reminders: 1 day, 6 h, 3 h, 1 h and 30 min before, and when due. Not while
        // hidden (full-screen game, video or presentation): they come once it is back.
        if !self.all_hidden() {
            self.task_reminders(t.secs);
        }

        // updates: a quiet check now and then, the offer once nothing is in the way
        if !self.demo && !self.upd_busy && Instant::now() >= self.upd_next && std::env::var_os("PATPET_NO_UPDATE").is_none() {
            self.upd_next = Instant::now() + Duration::from_secs(6 * 3600);
            self.upd_busy = true;
            update::check(false);
        }
        self.offer_update();

        // theme.json edited: apply the new colors right away
        let m = theme::modified();
        if m != self.theme_mtime {
            self.theme_mtime = m;
            self.reload_theme();
        }

        if self.dirty {
            self.save_now();
        }
    }

    fn task_reminders(&mut self, now_s: i64) {
        let today = now_s.div_euclid(86400);
        let mut due: Vec<String> = Vec::new();
        // (task text, deadline)
        let mut soon: Vec<(String, i64)> = Vec::new();
        for task in self.data.tasks.iter_mut() {
            let Some(d) = task.due else { continue };
            if task.done {
                continue;
            }
            let (which, bits) = reminder_due(d, now_s, task.alerts);
            if bits != task.alerts {
                task.alerts = bits;
                self.dirty = true;
            }
            if which.is_some() {
                soon.push((task.text.clone(), d));
            }
            if !task.notified && d <= now_s {
                task.notified = true;
                self.dirty = true;
                due.push(task.text.clone());
            }
        }
        if !due.is_empty() {
            sound::chime();
            let sub = if due.len() == 1 { due[0].clone() } else { format!("{} and {} more", due[0], due.len() - 1) };
            self.toast(TIcon::G(ic::TASKS), "Task due now", &sub, 12.0, TASK_C, Some(Tab::Tasks));
        } else if !soon.is_empty() {
            // the most pressing one leads
            soon.sort_by_key(|(_, d)| *d);
            let (text, d) = &soon[0];
            let when = if d.div_euclid(86400) == today { hm(*d) } else { due_label(*d, today) };
            let (title, sub) = if soon.len() == 1 {
                (format!("Task due {}", left_label(d - now_s)), format!("{text} · {when}"))
            } else {
                (format!("{} tasks due soon", soon.len()), format!("{text} · {when} and {} more", soon.len() - 1))
            };
            sound::chime();
            self.toast(TIcon::G(ic::TASKS), &title, &sub, 12.0, TASK_C, Some(Tab::Tasks));
        }
    }

    // -------------------------------------------------------------- events

    pub fn on_events(&mut self) {
        for e in events::drain() {
            match e {
                Event::Media(mut m) => {
                    dbg_log(&format!("app: media event -> {}", m.as_ref().map(|m| m.title.as_str()).unwrap_or("<none>")));
                    if self.demo_idle {
                        m = None;
                    }
                    let old = self.media.as_ref().map(|m| m.key);
                    let new = m.as_ref().map(|m| m.key);
                    // Keep the optimistic play/pause state until the player confirms it,
                    // otherwise a stale update makes the icon flicker back for a moment.
                    if let (Some(st), Some((want, since))) = (m.as_mut(), self.pending_play) {
                        if st.playing == want || since.elapsed() > Duration::from_millis(2500) || new != old {
                            self.pending_play = None;
                        } else {
                            st.playing = want;
                        }
                    }
                    self.media = m;
                    if self.media.is_none() {
                        self.pending_play = None;
                        if self.expanded && self.tab == Tab::Media {
                            self.lyrics_open = false;
                        }
                    }
                    if new != old && self.lyrics_open {
                        self.ensure_lyrics();
                    }
                }
                Event::Art(a) => match a {
                    // Art arrives before the matching Media event, so accept it and tag it
                    // with its track key; it is only shown while that track is current.
                    Some(a) => {
                        self.art = self.gfx.create_bitmap(a.w, a.h, &a.px);
                        self.art_key = a.key;
                        self.art_color = C(a.color.0, a.color.1, a.color.2, 1.0);
                    }
                    None => {
                        self.art = None;
                        self.art_key = 0;
                    }
                },
                Event::Lyrics(key, r) => {
                    // the download's buffers (JSON with every search hit) are freed by now
                    crate::util::release_free_heap();
                    if key == self.lyr_key {
                        self.lyr = match r {
                            Some(v) if !v.is_empty() => Lyr::Found(v),
                            _ => Lyr::Missing,
                        };
                        if let Some(c) = self.cur_line() {
                            self.lyr_scroll = c as f32;
                        }
                    }
                }
                Event::Volume(v, muted) => {
                    let ch = if muted || v <= 0.001 { ic::MUTE } else { ic::VOL };
                    self.hud(K_VOL, ch, if muted { 0.0 } else { v }, WHITE);
                }
                Event::AudioDevice => {
                    let changed = self.audio.as_mut().map(|a| a.rebind()).unwrap_or(false);
                    if changed {
                        let name = self.audio.as_ref().map(|a| a.name.clone()).unwrap_or_default();
                        let l = name.to_lowercase();
                        let wireless = ["bluetooth", "buds", "airpods", "headphone", "headset", "earbud", "wh-", "wf-"]
                            .iter()
                            .any(|k| l.contains(k));
                        let title = if wireless { "Headphones connected" } else { "Audio output changed" };
                        self.toast(TIcon::G(ic::HEADPHONE), title, &name, 3.5, WHITE, None);
                    }
                }
                Event::NetChanged => self.net_check = Some(Instant::now() + Duration::from_millis(1500)),
                Event::Caps => {
                    self.caps = !self.caps;
                    let title = if self.caps { "Caps Lock on" } else { "Caps Lock off" };
                    self.toast(TIcon::Txt("⇪".into()), title, "", 1.4, if self.caps { GREEN } else { WHITE }, None);
                }
                Event::DragEnter(text) => {
                    if !self.drag_hover {
                        // the drag is over one display's island: the panel opens there
                        self.home = self.island_at_cursor();
                    }
                    self.drag_hover = true;
                    self.drag_text = text;
                    self.collapse_at = None;
                    if !self.expanded {
                        self.expanded = true;
                        self.drag_expanded = true;
                        self.panel_entered = false;
                        system::watch_clicks(true);
                        self.toast = None;
                    }
                    self.tab = if text { Tab::Clipboard } else { Tab::Shelf };
                }
                Event::MouseDown => {
                    // opened by a drag, so not the active window: a click elsewhere closes it
                    if self.expanded && self.drag_expanded && !self.drag_hover && !self.home_inside() {
                        self.collapse();
                    }
                }
                Event::DragLeave => {
                    self.drag_hover = false;
                    if self.drag_expanded {
                        self.collapse_at = Some(Instant::now() + Duration::from_millis(700));
                    }
                }
                Event::Drop(files) => {
                    self.drag_hover = false;
                    dbg_log(&format!("drop: {} file(s) on the island under the cursor ({:?})", files.len(), self.island_at_cursor()));
                    for f in files {
                        if !self.data.shelf.contains(&f) {
                            self.data.shelf.push(f);
                        }
                    }
                    // slide to the end so the newly added files come into view
                    self.shelf_scroll = self.shelf_max();
                    self.dirty = true;
                    if self.drag_expanded {
                        self.collapse_at = Some(Instant::now() + Duration::from_millis(2500));
                    }
                }
                Event::DropText(t) => {
                    self.drag_hover = false;
                    self.drag_text = false;
                    // kept in the history only: what is on the Windows clipboard stays as it is
                    if clip::push(&mut self.clips, t, now().secs) {
                        self.clip_scroll = 0;
                        self.clip_dropped = self.clips.first().map(|i| (Instant::now(), i.text.clone()));
                        dbg_log(&format!("clipboard: {} entries (dropped)", self.clips.len()));
                    }
                    if self.drag_expanded {
                        self.collapse_at = Some(Instant::now() + Duration::from_millis(2500));
                    }
                }
                Event::UpdateInfo(r, manual) => self.on_update_info(r, manual),
                Event::UpdateDone(r) => self.on_update_done(r),
                Event::Thumb(p, t) => {
                    // only while the file is still on the Shelf (it may be gone by now)
                    if let Some(slot) = self.thumbs.get_mut(&p) {
                        *slot = t.and_then(|t| self.gfx.create_bitmap(t.w, t.h, &t.px));
                    }
                }
            }
        }
        self.kick();
    }

    pub fn on_power(&mut self, wp: WPARAM, lp: LPARAM) {
        const PBT_APMPOWERSTATUSCHANGE: usize = 0x000A;
        const PBT_POWERSETTINGCHANGE: usize = 0x8013;
        match wp.0 {
            PBT_APMPOWERSTATUSCHANGE => {
                let p = system::power();
                if let (Some((ac, pct)), Some((old_ac, _))) = (p, self.power) {
                    if ac != old_ac {
                        if ac {
                            self.low_warned = false;
                            self.cheer();
                            self.toast(TIcon::Battery(pct as f32 / 100.0, true), "Charging", &format!("{pct}% · plugged in"), 3.0, GREEN, None);
                        } else {
                            self.toast(TIcon::Battery(pct as f32 / 100.0, false), "On battery", &format!("{pct}% remaining"), 3.0, WHITE, None);
                        }
                    } else if !ac && pct <= 20 && !self.low_warned {
                        self.low_warned = true;
                        self.toast(TIcon::Battery(pct as f32 / 100.0, false), "Battery low", &format!("{pct}% left — plug in soon"), 6.0, RED, None);
                    }
                }
                self.power = p;
                self.kick();
            }
            PBT_POWERSETTINGCHANGE => unsafe {
                let s = &*(lp.0 as *const POWERBROADCAST_SETTING);
                if s.PowerSetting == system::GUID_BRIGHTNESS && s.DataLength >= 4 {
                    let v = std::ptr::read_unaligned(s.Data.as_ptr() as *const u32);
                    if self.bright_seen {
                        self.hud(K_BRIGHT, ic::BRIGHT, (v.min(100)) as f32 / 100.0, C::hex(0xFFD60A));
                    }
                    self.bright_seen = true;
                }
            },
            _ => {}
        }
    }

    pub fn on_display_change(&mut self) {
        crate::vsync::display_changed();
        self.sync_displays();
        self.kick();
    }

    // -------------------------------------------------------------- input

    fn local(&self, lp: LPARAM) -> (f32, f32) {
        let x = (lp.0 & 0xFFFF) as i16 as f32;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
        (x / self.gfx.scale, y / self.gfx.scale)
    }

    /// Ask Windows for a WM_MOUSELEAVE, once per stay of the cursor on the window.
    fn track_leave(&mut self) {
        if !self.s.tracking {
            self.s.tracking = true;
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.s.hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
            }
        }
    }

    pub fn on_mouse_move(&mut self, lp: LPARAM) {
        let p = self.local(lp);
        if self.s.mouse.is_none() {
            self.s.hover_in = Some(Instant::now());
        }
        self.track_leave();
        self.s.mouse = Some(p);
        if self.expanded {
            self.panel_entered = true;
        }
        self.stroke_cat(p);
        if let Some((Some(a), sx, sy)) = self.s.press {
            if (p.0 - sx).abs() + (p.1 - sy).abs() > 6.0 {
                let paths: Vec<String> = match a {
                    // dragging a selected tile (or the button) carries the selection, in shelf order
                    Act::ShelfItem(i) if self.data.shelf.get(i).map(|p| self.shelf_sel.contains(p)).unwrap_or(false) => self.selected_paths(),
                    Act::ShelfItem(i) => self.data.shelf.get(i).cloned().into_iter().collect(),
                    Act::ShelfAll if !self.shelf_sel.is_empty() => self.selected_paths(),
                    Act::ShelfAll => self.data.shelf.clone(),
                    _ => Vec::new(),
                };
                if matches!(a, Act::ShelfItem(_) | Act::ShelfAll) {
                    self.s.press = None;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    if !paths.is_empty() {
                        self.shelf_sel.clear();
                        self.deferred = Some(Deferred::DragOut(paths));
                    }
                }
            }
        }
        self.kick();
    }

    /// Decides whether (and when) the open panel should close by itself.
    fn arm_auto_close(&mut self, n: Instant) {
        if self.home_inside() {
            self.panel_entered = true;
            self.collapse_at = None;
            return;
        }
        // typing, or holding the mouse button (dragging the seek bar): stay until that is over
        let busy = self.input_focus || self.edit.is_some() || self.s.press.is_some();
        let fg = unsafe { GetForegroundWindow() };
        if self.collapse_at.is_none() {
            if self.drag_expanded {
                // after the drop the cursor came back over the island and has now left it: the
                // file is on the Shelf, so close about as soon as the drag leaving would
                let wait = if self.panel_entered { 700 } else { 2500 };
                self.collapse_at = Some(n + Duration::from_millis(wait));
            } else if self.panel_entered && !busy {
                self.collapse_at = Some(n + PANEL_OUT);
            } else if fg != self.home {
                self.collapse_at = Some(n + Duration::from_millis(1200));
            }
        } else if busy && fg == self.home && !self.drag_expanded {
            self.collapse_at = None;
        }
    }

    pub fn on_mouse_leave(&mut self) {
        self.s.tracking = false;
        if self.menu_hold {
            // the island's own menu took the mouse: settled when the menu closes (menu_closed)
            return;
        }
        self.s.mouse = None;
        self.s.hover_out = Some(Instant::now());
        if self.expanded && !self.demo {
            self.arm_auto_close(Instant::now());
            if self.collapse_at.is_some() {
                self.set_tick(90);
            }
        }
        self.kick();
    }

    /// Everything fires on release (like a normal button): pressing only remembers what is
    /// under the cursor, and the action runs when the button is released over the same thing.
    /// Holding the mouse down therefore never opens the island or triggers a button, and
    /// sliding off before releasing cancels. Shelf tiles can be dragged out while held.
    pub fn on_ldown(&mut self, lp: LPARAM) {
        let p = self.local(lp);
        dbg_log(&format!("press at {:.0},{:.0} (foreground: {:?})", p.0, p.1, unsafe { GetForegroundWindow() }));
        self.s.mouse = Some(p);
        self.s.press = Some((self.hit_at(p), p.0, p.1));
        unsafe {
            SetCapture(self.s.hwnd);
        }
    }

    pub fn on_lup(&mut self, lp: LPARAM) {
        let p = self.local(lp);
        unsafe {
            let _ = ReleaseCapture();
        }
        let Some((hit, _, _)) = self.s.press.take() else {
            dbg_log("release without a press");
            return;
        };
        dbg_log(&format!("release at {:.0},{:.0} on {hit:?}", p.0, p.1));
        // released somewhere else than where it was pressed: cancel
        if !self.cursor_inside() || self.hit_at(p) != hit {
            return;
        }
        // the closed island (also one on another display while the panel is open elsewhere)
        if !self.expanded || self.s.hwnd != self.home {
            match hit {
                Some(Act::VolBar(r)) => self.act(Act::VolBar(r), p.0),
                Some(Act::Tab(t)) => self.expand(Some(t)),
                Some(Act::Toast) => {
                    let (tab, tap) = self.toast.as_ref().map(|t| (t.tab, t.tap)).unwrap_or((None, Tap::None));
                    self.toast = None;
                    if !self.toast_tapped(tap) {
                        self.expand(tab);
                    }
                }
                _ => self.expand(None),
            }
            return;
        }
        match hit {
            Some(a) => self.act(a, p.0),
            None => {
                self.input_focus = false;
                self.commit_edit();
                self.kick();
            }
        }
    }

    pub fn on_rup(&mut self) {
        self.menu_hold = true;
        self.menu_on = self.s.hwnd;
        self.deferred = Some(Deferred::Menu);
    }

    /// The menu is gone: back to what the mouse is really doing.
    pub fn menu_closed(&mut self) {
        let h = self.menu_on;
        self.on(h, |a| a.menu_closed_here());
    }

    fn menu_closed_here(&mut self) {
        if !self.menu_hold {
            return;
        }
        self.menu_hold = false;
        self.s.tracking = false;
        if self.cursor_inside() && self.cursor_on_window() {
            let mut p = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut p);
            }
            let s = self.gfx.scale;
            self.s.mouse = Some(((p.x - self.s.win.0) as f32 / s, (p.y - self.s.win.1) as f32 / s));
            self.track_leave();
            self.kick();
        } else {
            self.on_mouse_leave();
        }
    }

    pub fn on_wheel(&mut self, wp: WPARAM, lp: LPARAM) {
        let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
        let steps = delta.signum();
        let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
        unsafe {
            let _ = ScreenToClient(self.s.hwnd, &mut pt);
        }
        let p = (pt.x as f32 / self.gfx.scale, pt.y as f32 / self.gfx.scale);
        let hit = self.hit_at(p);
        let vol_step = |s: &mut App| {
            if let Some(a) = &s.audio {
                if let Some((v, _)) = a.get() {
                    a.set(((v * 50.0).round() + steps as f32) / 50.0);
                }
            }
        };
        if !self.expanded {
            vol_step(self);
            return;
        }
        match (self.tab, hit) {
            (Tab::Tasks, Some(Act::CalDay(_) | Act::CalMonth(_) | Act::CalToday)) => self.shift_month(-steps),
            (Tab::Tasks, Some(Act::TaskTime(_))) | (Tab::Tasks, Some(Act::Edit(Field::TaskTime))) => {
                if self.task_due.is_none() {
                    self.adjust_due(0);
                } else {
                    self.adjust_due(steps as i64 * 300);
                }
            }
            (Tab::Tasks, _) => {
                if steps < 0 {
                    self.task_scroll = (self.task_scroll + 1).min(self.data.tasks.len().saturating_sub(1));
                } else {
                    self.task_scroll = self.task_scroll.saturating_sub(1);
                }
            }
            (Tab::Clipboard, _) => {
                if steps < 0 {
                    self.clip_scroll = (self.clip_scroll + 1).min(self.clips.len().saturating_sub(1));
                } else {
                    self.clip_scroll = self.clip_scroll.saturating_sub(1);
                }
            }
            (Tab::Shelf, _) => {
                if steps < 0 {
                    self.shelf_scroll = (self.shelf_scroll + 1).min(self.shelf_max());
                } else {
                    self.shelf_scroll = self.shelf_scroll.saturating_sub(1);
                }
            }
            (Tab::Media, _) if !self.lyrics_open => vol_step(self),
            _ => {}
        }
        self.kick();
    }

    pub fn on_activate(&mut self, active: bool) {
        // only the window the panel is on counts (another island taking focus moves the panel)
        if !active && self.s.hwnd == self.home {
            self.input_focus = false;
            if self.expanded && !self.drag_expanded && !self.demo && !self.menu_hold && !self.cursor_inside() {
                dbg_log("collapse: deactivate");
                self.collapse();
            }
        }
        self.kick();
    }

    pub fn on_key(&mut self, vk: u32) -> bool {
        if vk == VK_ESCAPE.0 as u32 {
            if self.edit.is_some() {
                self.edit = None;
            } else if self.input_focus {
                self.input_focus = false;
            } else if self.expanded {
                self.collapse();
            }
            self.kick();
            return true;
        }
        false
    }

    pub fn on_char(&mut self, ch: u32) {
        if let Some(e) = &mut self.edit {
            match ch {
                0x08 => {
                    if e.fresh {
                        e.buf.clear();
                    } else {
                        e.buf.pop();
                    }
                    e.fresh = false;
                }
                0x0D => self.commit_edit(),
                _ => {
                    if let Some(c) = char::from_u32(ch) {
                        let c = c.to_ascii_lowercase();
                        if c.is_ascii_digit() || ":.,hml ".contains(c) {
                            if e.fresh {
                                e.buf.clear();
                                e.fresh = false;
                            }
                            if e.buf.len() < 10 {
                                e.buf.push(c);
                            }
                        }
                    }
                }
            }
            self.kick();
            return;
        }
        if !self.input_focus {
            return;
        }
        let ch = ch as u16;
        match ch {
            0x08 => {
                self.input.pop();
            }
            0x0D => self.add_task(),
            0x16 => {
                if let Some(s) = clipboard_text() {
                    let s: String = s.chars().filter(|c| !c.is_control()).collect();
                    self.input.push_str(&s);
                }
            }
            0x01..=0x1F | 0x7F => {}
            0xD800..=0xDBFF => self.surrogate = Some(ch),
            0xDC00..=0xDFFF => {
                if let Some(hi) = self.surrogate.take() {
                    if let Some(Ok(c)) = char::decode_utf16([hi, ch]).next() {
                        self.input.push(c);
                    }
                }
            }
            _ => {
                if let Some(c) = char::from_u32(ch as u32) {
                    self.input.push(c);
                }
            }
        }
        if self.input.chars().count() > 140 {
            self.input = self.input.chars().take(140).collect();
        }
        self.kick();
    }

    fn reload_theme(&mut self) {
        let th = theme::load();
        self.gfx.set_theme(&th);
        dbg_log("theme reloaded");
        self.kick();
    }

    pub fn menu_state(&self) -> tray::MenuState {
        tray::MenuState {
            autostart: self.autostart,
            hidden: self.user_hidden,
            line_mode: self.data.settings.line_mode,
            screenshots: self.data.settings.screenshots,
            breaks: self.data.settings.breaks,
            update: self.upd_avail.as_ref().map(|r| r.version.clone()),
            update_busy: self.upd_busy,
        }
    }

    // -------------------------------------------------------------- updates

    /// A notice that does something of its own when tapped. Returns true if it did.
    fn toast_tapped(&mut self, tap: Tap) -> bool {
        match tap {
            Tap::None => false,
            Tap::Update => {
                self.start_update();
                true
            }
            Tap::Releases => {
                update::open_releases_page();
                true
            }
            Tap::Break => {
                self.breaks.start(self.mono_secs());
                self.pet.set_resting(true, Instant::now());
                dbg_log("break: started");
                self.kick();
                true
            }
        }
    }

    fn notice(&mut self, icon: TIcon, title: &str, sub: &str, secs: f32, accent: C, tap: Tap) {
        self.toast(icon, title, sub, secs, accent, None);
        if let Some(t) = &mut self.toast {
            t.tap = tap;
        }
    }

    /// Tray item: check now, or install the update already found.
    fn update_cmd(&mut self) {
        if self.upd_busy {
            return;
        }
        if self.upd_avail.is_some() {
            self.start_update();
        } else {
            self.upd_busy = true;
            self.upd_manual = true;
            self.upd_next = Instant::now() + Duration::from_secs(6 * 3600);
            update::check(true);
            self.notice(TIcon::G(ic::DOWNLOAD), "Checking for updates…", "", 4.0, WHITE, Tap::None);
        }
    }

    fn start_update(&mut self) {
        let Some(r) = self.upd_avail.clone() else { return };
        if self.upd_busy {
            return;
        }
        self.upd_busy = true;
        if self.expanded {
            self.collapse();
        }
        let sub = format!("Downloading v{} · PatPet restarts by itself", r.version);
        update::install(r);
        self.notice(TIcon::G(ic::DOWNLOAD), "Updating PatPet…", &sub, 120.0, GREEN, Tap::None);
    }

    fn on_update_info(&mut self, r: Result<Option<update::Release>, ()>, manual: bool) {
        self.upd_busy = false;
        let manual = manual || std::mem::take(&mut self.upd_manual);
        match r {
            Ok(Some(rel)) => {
                dbg_log(&format!("update: v{} available", rel.version));
                self.upd_avail = Some(rel);
                if self.demo && std::env::var("PATPET_DEMO").as_deref() == Ok("update") {
                    self.start_update();
                    return;
                }
                if manual {
                    // asked for it: offer it again even if it was offered before
                    self.upd_shown = None;
                    self.toast = None;
                }
                self.offer_update();
            }
            Ok(None) => {
                self.upd_avail = None;
                if manual {
                    self.notice(TIcon::G(ic::CHECK), "PatPet is up to date", &format!("Version {}", update::CURRENT), 3.0, GREEN, Tap::None);
                }
            }
            Err(()) => {
                dbg_log("update: check failed");
                if manual {
                    self.notice(TIcon::G(ic::WIFI), "Couldn't check for updates", "No connection to GitHub", 4.0, RED, Tap::None);
                }
            }
        }
    }

    /// "Update available" once per version, and not while hidden (full screen) or open.
    fn offer_update(&mut self) {
        let Some(r) = &self.upd_avail else { return };
        if self.upd_busy || self.all_hidden() || self.expanded || self.upd_shown.as_deref() == Some(r.version.as_str()) {
            return;
        }
        // don't push away another notice; per_second tries again
        if self.toast.is_some() {
            return;
        }
        let v = r.version.clone();
        self.upd_shown = Some(v.clone());
        self.notice(TIcon::G(ic::DOWNLOAD), &format!("PatPet {v} is available"), "Tap to update · takes a few seconds", 12.0, GREEN, Tap::Update);
    }

    fn on_update_done(&mut self, r: Result<(), String>) {
        match r {
            Ok(()) => {
                dbg_log("update: installed, restarting");
                self.save_now();
                if update::relaunch() {
                    tray::remove(self.hwnd);
                    crate::quit_now();
                }
                // could not start the new copy: it runs from the next launch on
                self.upd_busy = false;
                self.upd_avail = None;
                self.notice(TIcon::G(ic::CHECK), "Update installed", "Restart PatPet to use the new version", 8.0, GREEN, Tap::None);
            }
            Err(e) => {
                dbg_log(&format!("update: failed: {e}"));
                self.upd_busy = false;
                self.notice(TIcon::G(ic::DOWNLOAD), "Update failed", &format!("{e} · tap to open the download page"), 10.0, RED, Tap::Releases);
            }
        }
    }

    /// Started by the updater (`--after-update`).
    pub fn updated(&mut self) {
        self.notice(TIcon::G(ic::CHECK), "PatPet updated", &format!("You're on version {}", update::CURRENT), 5.0, GREEN, Tap::None);
    }

    /// The clipboard changed. Reading it right away would hold it open while the copying app
    /// may still be finishing its copy (OLE apps flush after announcing it), making that app's
    /// copy fail now and then; so read a moment later, from tick(). This also merges the two
    /// notifications many apps send per copy.
    pub fn on_clipboard(&mut self) {
        if self.clip_due.is_none() {
            self.clip_due = Some(Instant::now() + Duration::from_millis(150));
        }
    }

    /// Remember a text copy (see clip.rs for what is skipped).
    fn read_clipboard(&mut self) {
        if let Some(t) = clip::read(self.hwnd) {
            // the echo of an entry copied from the list itself
            if self.clip_ignore.take().as_deref() == Some(t.as_str()) {
                return;
            }
            let blank = t.trim().is_empty();
            let added = clip::push(&mut self.clips, t, now().secs);
            if added {
                self.clip_scroll = 0;
                // count only: copied text must never reach the log file
                dbg_log(&format!("clipboard: {} entries", self.clips.len()));
                if self.expanded && self.tab == Tab::Clipboard && !self.all_hidden() {
                    self.render(false);
                }
            }
            // copying the same text again is a copy too, but some apps announce one copy twice
            let again = self.copied_at.map(|t| t.elapsed() >= Duration::from_secs(1)).unwrap_or(true);
            if !blank && (added || again) {
                self.copied_at = Some(Instant::now());
                self.copied_notice();
            }
        }
    }

    /// A small pill. Not over the open panel, and not over a reminder that can be tapped.
    fn copied_notice(&mut self) {
        let reminder = self.toast.as_ref().map(|t| t.tab.is_some() || t.tap != Tap::None).unwrap_or(false);
        if !self.expanded && !reminder && !self.demo {
            self.toast(TIcon::G(ic::CLIP), "Text copied", "", 1.4, WHITE, None);
        }
    }

    // -------------------------------------------------------------- the cat

    /// Cat size (DIP): CAT_SIZE at every screen scale, in whole screen pixels.
    fn cat_size(&self) -> f32 {
        let s = self.gfx.scale;
        (CAT_SIZE * s).round() / s
    }

    /// Room the cat takes at the left of the closed island.
    fn cat_slot(&self) -> f32 {
        6.0 + self.cat_size()
    }

    /// The cat is on screen (anything but the bare line, or hidden).
    fn cat_visible(&self) -> bool {
        self.surfaces().any(|o| !o.hidden && o.view != ViewKey::Line)
    }

    fn hearts_on(&self) -> bool {
        self.hearts_at.map(|t| t.elapsed().as_secs_f32() < HEARTS_SECS).unwrap_or(false)
    }

    /// Bitmaps for the sprite strips (made once), the room while the Pet tab is open, the
    /// cat's walk, and the frame to draw now.
    fn sync_cat(&mut self) {
        if self.cat_bmps.is_empty() {
            let strips = std::mem::take(&mut self.cat_strips);
            self.cat_bmps = strips.into_iter().map(|s| s.and_then(|s| self.gfx.pixmap(s.w, s.h, s.px))).collect();
            self.props_bmp = crate::sprites::props().and_then(|s| self.gfx.pixmap(s.w, s.h, s.px));
        }
        if self.expanded && self.tab == Tab::Pet {
            let i = self.data.pet.room_index();
            if self.room_bmp.as_ref().map(|(k, _)| *k) != Some(i) {
                self.room_bmp = crate::sprites::room(i).and_then(|s| self.gfx.pixmap(s.w, s.h, s.px)).map(|b| (i, b));
            }
        }
        let n = Instant::now();
        let dt = n.duration_since(self.room_step).as_secs_f32().min(0.25);
        self.room_step = n;
        if let Some(goal) = self.room_goal {
            if self.pet.is_walking() {
                let d = goal - self.room_x;
                let step = WALK_SPEED * dt;
                self.room_x = if d.abs() <= step { goal } else { self.room_x + step * d.signum() };
            }
        }
        self.cat_frame = self.pet.frame(n);
    }

    /// The cat strolls around its room now and then while the Pet tab is open.
    fn wander(&mut self, n: Instant) {
        let in_room = self.expanded && self.tab == Tab::Pet;
        if !in_room {
            if self.room_goal.take().is_some() {
                self.pet.set_walking(false, n);
            }
            return;
        }
        match self.room_goal {
            Some(goal) if (goal - self.room_x).abs() < 0.001 => {
                self.room_goal = None;
                self.pet.set_walking(false, n);
                let r = self.random();
                self.room_rest = n + Duration::from_millis(4000 + (r % 7000) as u64);
            }
            None if n >= self.room_rest => {
                let r = self.random();
                // somewhere else in the room, at least a little way off
                let mut goal = 0.08 + (r % 1000) as f32 / 1000.0 * 0.84;
                if (goal - self.room_x).abs() < 0.2 {
                    goal = if self.room_x > 0.5 { goal - 0.4 } else { goal + 0.4 }.clamp(0.08, 0.92);
                }
                self.room_goal = Some(goal);
                self.room_step = n;
                self.pet.set_walking(true, n);
            }
            _ => {}
        }
    }

    fn random(&mut self) -> u32 {
        let mut x = self.room_rng ^ (Instant::now().duration_since(self.t0).subsec_nanos());
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.room_rng = x;
        x
    }

    /// One frame of a cat animation into `r`, at `opacity`.
    fn draw_cat_frame(&self, g: &Gfx, mood: Mood, i: usize, r: R, opacity: f32) {
        if let Some(Some(b)) = self.cat_bmps.get(crate::sprites::index(mood)) {
            let f = crate::sprites::FRAME as f32;
            g.pixel_art(b, R::new(i as f32 * f, 0.0, f, f), r, opacity);
        }
    }

    /// A prop icon (props.png) into `r`.
    fn draw_prop(&self, g: &Gfx, i: usize, r: R) {
        if let Some(b) = &self.props_bmp {
            let f = crate::sprites::PROP as f32;
            g.pixel_art(b, R::new(i as f32 * f, 0.0, f, f), r, 1.0);
        }
    }

    /// Draws the cat with its left edge at `x`, standing on `bottom`; returns where it went.
    fn draw_cat(&self, g: &Gfx, x: f32, bottom: f32) -> R {
        let size = self.cat_size();
        let px = |v: f32| (v * g.scale).round() / g.scale;
        self.draw_cat_in(g, R::new(px(x), px(bottom - size), size, size))
    }

    /// Draws the cat (current frame, hearts if happy) filling `r`; returns `r`.
    fn draw_cat_in(&self, g: &Gfx, r: R) -> R {
        self.s.cat_rect.set(Some(r));
        let (mood, i) = self.cat_frame;
        self.draw_cat_frame(g, mood, i, r, 1.0);
        if let Some(t) = self.hearts_at {
            // three little hearts float up from the cat and fade
            let e = t.elapsed().as_secs_f32();
            for k in 0..3 {
                let p = (e - k as f32 * 0.18) / (HEARTS_SECS - 0.36);
                if !(0.0..1.0).contains(&p) {
                    continue;
                }
                // from beside the cat's head, drifting up and a little to the right
                let hx = r.x + r.w * (0.86 + 0.12 * k as f32) + (p * 8.0 + k as f32 * 2.0).sin() * 1.5;
                let hy = r.y + r.h * (0.5 - 0.08 * k as f32) - p * r.h * 0.45;
                let a = if p < 0.15 { p / 0.15 } else { 1.0 - (p - 0.15) / 0.85 };
                let sz = 4.0 + 2.0 * p;
                g.icon(HEART, R::new(hx - sz, hy - sz, sz * 2.0, sz * 2.0), sz, HEART_C.a(a));
            }
        }
        r
    }

    /// The cat is happy (petted, a task done, a goal reached...): hearts, and a little jump.
    fn cheer(&mut self) {
        let n = Instant::now();
        self.pet.cheer(n);
        self.hearts_at = Some(n);
        self.kick();
    }

    /// Petting the cat (has a short cooldown so it stays special).
    fn pet_cat(&mut self) {
        let n = Instant::now();
        if self.pet.click(n) {
            self.hearts_at = Some(n);
            self.sync_care();
            let ups = self.data.pet.pet();
            self.dirty = true;
            self.on_levels(ups);
            self.kick();
        }
    }

    // -------------------------------------------------------------- caring for the cat

    /// Bring the needs up to date and tell the cat how it is doing.
    fn sync_care(&mut self) {
        let asleep = !self.demo && system::user_idle() >= CAT_SLEEP_AFTER;
        self.data.pet.advance(unix_now(), asleep);
        let c = &self.data.pet;
        self.pet.set_care(c.nap, c.lowest(), Instant::now());
    }

    /// A card in the Pet tab: feed, play, put to bed / wake up, bath.
    fn care_for(&mut self, need: Need) {
        self.sync_care();
        let n = Instant::now();
        let c = &mut self.data.pet;
        let ups = match need {
            Need::Food => {
                self.pet.act(Action::Feed, n);
                c.feed()
            }
            Need::Fun => {
                self.pet.act(Action::Play, n);
                c.play()
            }
            Need::Energy => {
                c.toggle_nap();
                0
            }
            Need::Clean => {
                self.pet.act(Action::Bath, n);
                c.bath()
            }
        };
        self.dirty = true;
        self.sync_care();
        self.on_levels(ups);
        self.kick();
    }

    /// The cat went up `ups` levels: it celebrates, and a notice says what opened up.
    /// Seconds on a steady clock (since PatPet started), for the break timer.
    fn mono_secs(&self) -> u64 {
        self.t0.elapsed().as_secs()
    }

    /// Break reminders: suggest one after a long stretch of work, reward one taken.
    fn break_tick(&mut self) {
        if !self.data.settings.breaks || self.demo {
            return;
        }
        let now_s = self.mono_secs();
        let idle = system::user_idle().as_secs();
        // not over a full-screen app, and not on top of another notice
        let busy = self.all_hidden() || self.toast.is_some() || self.leaving.is_some();
        let after = u64::from(self.data.settings.break_after_min.max(5)) * 60;
        match self.breaks.tick(now_s, idle, busy, after) {
            Some(crate::breaks::Event::Due) => self.ask_break(),
            Some(crate::breaks::Event::Ignored) => dbg_log("break: not now"),
            Some(crate::breaks::Event::Taken) => {
                dbg_log("break: taken");
                self.pet.set_resting(false, Instant::now());
                let ups = self.data.pet.took_break(now().day());
                self.dirty = true;
                self.cheer();
                self.notice(TIcon::Prop(prop::STAR), "Nice break!", "Your cat enjoyed it too", 6.0, HEART_C, Tap::None);
                self.on_levels(ups);
            }
            Some(crate::breaks::Event::Ended) => {
                dbg_log("break: ended without resting");
                self.pet.set_resting(false, Instant::now());
                self.kick();
            }
            None => {}
        }
    }

    /// The cat stretches and suggests a short break.
    fn ask_break(&mut self) {
        dbg_log("break: suggested");
        self.pet.act(Action::Stretch, Instant::now());
        self.notice(
            TIcon::Prop(prop::ZZZ),
            "Time for a little break?",
            "Tap to rest 5 minutes with your cat",
            crate::breaks::ASK_SECS as f32,
            HEART_C,
            Tap::Break,
        );
        self.kick();
    }

    fn on_levels(&mut self, ups: u32) {
        if ups == 0 {
            return;
        }
        let level = self.data.pet.level;
        self.pet.act(Action::LevelUp, Instant::now());
        let new_room = care::ROOMS.iter().find(|r| r.2 > level - ups && r.2 <= level);
        let sub = match new_room {
            Some(r) => format!("New room unlocked: {}", r.1),
            None => match care::ROOMS.iter().find(|r| r.2 > level) {
                Some(r) => format!("Next room at level {}", r.2),
                None => "Every room is open".to_string(),
            },
        };
        self.toast(TIcon::Prop(prop::STAR), &format!("Level {level}!"), &sub, 6.0, TASK_C, Some(Tab::Pet));
        self.kick();
    }

    /// Now and then, a gentle word when a need is running low (not while the Pet tab is open).
    fn need_notice(&mut self) {
        let Some(need) = self.data.pet.lowest() else { return };
        let t = unix_now();
        if self.all_hidden() || self.demo || (self.expanded && self.tab == Tab::Pet) || t - self.data.pet.noticed < NEED_NOTICE_EVERY {
            return;
        }
        self.data.pet.noticed = t;
        self.dirty = true;
        let (icon, title, sub) = match need {
            Need::Food => (prop::FISH, "Your cat is hungry", "Tap to feed it"),
            Need::Fun => (prop::RAIN, "Your cat is bored", "Tap to play together"),
            Need::Energy => (prop::ZZZ, "Your cat is sleepy", "Tap to put it to bed"),
            Need::Clean => (prop::DIRT, "Your cat needs a bath", "Tap to give it one"),
        };
        self.toast(TIcon::Prop(icon), title, sub, 8.0, HEART_C, Some(Tab::Pet));
    }

    /// The Pet tab: the cat's room, its level, and the four care cards.
    fn draw_pet(&self, g: &Gfx, ui: &mut Ui, c: R) {
        let care = &self.data.pet;
        let room = R::new(c.x, c.y, c.w, ROOM_H);
        g.push_rr(room, 12.0, 1.0);
        match &self.room_bmp {
            Some((_, b)) => {
                let (w, h) = (crate::sprites::ROOM_W as f32, crate::sprites::ROOM_H as f32);
                g.pixel_art(b, R::new(0.0, 0.0, w, h), R::new(room.x, room.y, w * ROOM_SCALE, h * ROOM_SCALE), 1.0);
            }
            None => g.rect(room, SURF),
        }
        if care.nap {
            // lights out
            g.rect(room, C(0.02, 0.03, 0.10, 0.55));
        }
        // the cat, standing on the floor (whole screen pixels so it stays crisp)
        let size = crate::sprites::FRAME as f32 * ROOM_SCALE;
        let px = |v: f32| (v * g.scale).round() / g.scale;
        let floor = room.y + crate::sprites::ROOM_FLOOR as f32 * ROOM_SCALE;
        let cx = room.x + 6.0 + self.room_x * (room.w - size - 12.0);
        let cat = self.draw_cat_in(g, R::new(px(cx), px(floor - size), size, size));
        ui.hit(cat, Act::Pet);

        // level and experience, top left
        let chip = R::new(room.x + 8.0, room.y + 8.0, 150.0, 24.0);
        g.rrect(chip, 12.0, C(0.0, 0.0, 0.0, 0.5));
        self.draw_prop(g, prop::STAR, R::new(chip.x + 6.0, chip.cy() - 7.0, 14.0, 14.0));
        g.text(&format!("Level {}", care.level), R::new(chip.x + 25.0, chip.y, 60.0, chip.h), 12.0, 700, PURE_WHITE, Al::L);
        let bar = R::new(chip.x + 84.0, chip.cy() - 2.5, chip.w - 94.0, 5.0);
        g.rrect(bar, 2.5, C(1.0, 1.0, 1.0, 0.2));
        let f = care.xp as f32 / care::xp_for(care.level) as f32;
        g.rrect(R::new(bar.x, bar.y, (bar.w * f.clamp(0.0, 1.0)).max(5.0), bar.h), 2.5, TASK_C);

        // the room's name, top right; arrows when more than one room is open
        let open = care::rooms_open(care.level);
        let ri = care.room_index();
        let name = care::ROOMS[ri].1;
        let label = match care::ROOMS.get(open) {
            Some(next) if open == 1 => format!("{name} · next at level {}", next.2),
            _ => name.to_string(),
        };
        let tw = g.measure(&label, 11.5, 600);
        let arrows = if open > 1 { 44.0 } else { 0.0 };
        let rc = R::new(room.right() - 8.0 - tw - 20.0 - arrows, room.y + 8.0, tw + 20.0 + arrows, 24.0);
        g.rrect(rc, 12.0, C(0.0, 0.0, 0.0, 0.5));
        g.text(&label, R::new(rc.x + 10.0 + arrows / 2.0, rc.y, tw + 2.0, rc.h), 11.5, 600, C(1.0, 1.0, 1.0, 0.86), Al::L);
        if open > 1 {
            for (k, (ch, step)) in [(ic::LEFT, -1), (ic::RIGHT, 1)].into_iter().enumerate() {
                let b = if k == 0 { R::new(rc.x + 2.0, rc.y + 2.0, 20.0, 20.0) } else { R::new(rc.right() - 22.0, rc.y + 2.0, 20.0, 20.0) };
                let hv = ui.hit(b, Act::Room(step));
                if hv {
                    g.circle(b.cx(), b.cy(), 10.0, C(1.0, 1.0, 1.0, 0.18));
                }
                g.icon(ch, b, 9.0, PURE_WHITE);
            }
        }
        g.pop();

        // the care cards
        let y = room.bottom() + 10.0;
        let gap = 8.0;
        let w = (c.w - 3.0 * gap) / 4.0;
        let cards = [
            (Need::Food, prop::BOWL, "Feed", C::hex(0xFFB86B)),
            (Need::Fun, prop::YARN, "Play", HEART_C),
            (Need::Energy, prop::BOLT, if care.nap { "Wake" } else { "Sleep" }, C::hex(0xFFD60A)),
            (Need::Clean, prop::SOAP, "Bath", CLIP_C),
        ];
        for (k, (need, icon, label, col)) in cards.into_iter().enumerate() {
            let r = R::new(c.x + k as f32 * (w + gap), y, w, 46.0);
            let hv = ui.hit(r, Act::Care(need));
            g.rrect(r, 12.0, surf(g, hv));
            self.draw_prop(g, icon, R::new(r.x + 9.0, r.y + 7.0, 16.0, 16.0));
            g.text(label, R::new(r.x + 31.0, r.y + 5.0, r.w - 66.0, 20.0), 12.5, 600, WHITE, Al::L);
            let v = care.value(need);
            g.text(&format!("{}%", v.round() as i32), R::new(r.right() - 46.0, r.y + 5.0, 37.0, 20.0), 11.0, 500, T3, Al::R);
            let bar = R::new(r.x + 10.0, r.y + 31.0, r.w - 20.0, 6.0);
            g.rrect(bar, 3.0, WHITE.a(0.12));
            let col = if v < care::LOW { RED } else { col };
            g.rrect(R::new(bar.x, bar.y, (bar.w * v / 100.0).max(6.0), bar.h), 3.0, col);
        }
    }

    /// Moving the mouse back and forth over the cat in the closed island strokes it.
    fn stroke_cat(&mut self, p: (f32, f32)) {
        let on_cat = !self.expanded
            && matches!(self.s.view, ViewKey::Compact(_))
            && self.s.cat_rect.get().map(|r| r.contains(p)).unwrap_or(false);
        if !on_cat {
            self.s.stroke = None;
            return;
        }
        let n = Instant::now();
        let (dist, last_x, since) = match self.s.stroke {
            Some((d, lx, t)) if n.duration_since(t) < STROKE_WINDOW => (d + (p.0 - lx).abs(), p.0, t),
            _ => (0.0, p.0, n),
        };
        if dist >= STROKE_WIDTHS * self.cat_size() {
            self.s.stroke = None;
            self.pet_cat();
        } else {
            self.s.stroke = Some((dist, last_x, since));
        }
    }

    pub fn tray_cmd(&mut self, cmd: u32) {
        if cmd == tray::CMD_THEME {
            if let Some(p) = theme::ensure_file() {
                tray::edit_file(&p);
            }
        } else if cmd == tray::CMD_LINE {
            self.data.settings.line_mode = !self.data.settings.line_mode;
            self.dirty = true;
            self.kick();
        } else if cmd == tray::CMD_BREAKS {
            self.data.settings.breaks = !self.data.settings.breaks;
            if !self.data.settings.breaks {
                // switched off in the middle of one: the cat gets up again
                self.breaks = Default::default();
                self.pet.set_resting(false, Instant::now());
                self.kick();
            }
            self.dirty = true;
        } else if cmd == tray::CMD_SHOTS {
            self.data.settings.screenshots = !self.data.settings.screenshots;
            if self.data.settings.screenshots {
                // only screenshots taken from now on
                self.shots = shots::Watcher::new();
            }
            self.dirty = true;
        } else if cmd == tray::CMD_AUTOSTART {
            self.autostart = !self.autostart;
            tray::set_autostart(self.autostart);
        } else if cmd == tray::CMD_UPDATE {
            self.update_cmd();
        } else if cmd == tray::CMD_HIDE {
            if self.user_hidden {
                self.user_hidden = false;
                self.apply_visibility();
            } else {
                self.leave(Leave::Hide);
            }
        }
    }

    /// Hidden while the user asked for it or a full-screen app is in front.
    fn apply_visibility(&mut self) {
        if self.leaving.is_some() {
            return;
        }
        self.each(|a| a.apply_visibility_here());
        // the panel's display went full screen: close it
        if self.expanded && self.surfaces().any(|o| o.hwnd == self.home && o.hidden) {
            self.collapse();
        }
    }

    /// Hide this island while the user hid PatPet or a full-screen app is in front on its
    /// display; bring it back afterwards.
    fn apply_visibility_here(&mut self) {
        let want = self.user_hidden || system::fullscreen_busy(self.s.hidden, self.s.display.handle);
        if want != self.s.hidden {
            dbg_log(&format!("visibility: {:?} hidden={want} (by user: {})", self.s.hwnd.0, self.user_hidden));
            self.s.hidden = want;
            // a hidden window hears nothing more from the mouse, so what it knew is out of date
            self.forget_pointer();
            unsafe {
                if want {
                    let _ = ShowWindow(self.s.hwnd, SW_HIDE);
                } else {
                    // to the very top: a game that made itself topmost went above the island
                    // while it was hidden, and may still be open
                    let _ = SetWindowPos(
                        self.s.hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
            if !want {
                if self.data.settings.line_mode {
                    // back as the line it was, not a pill left over from before or while hidden;
                    // the window appearing under a resting cursor is not a hover yet
                    self.s.no_hover_until = Some(Instant::now() + Duration::from_millis(350));
                    if self.target().3 == ViewKey::Line {
                        self.s.view = ViewKey::Line;
                        self.s.line_from_pill = false;
                    }
                }
                self.pop_in();
            }
        }
    }

    /// Drop everything known about the mouse, as if it had left long ago.
    fn forget_pointer(&mut self) {
        if self.s.press.take().is_some() {
            unsafe {
                let _ = ReleaseCapture();
            }
        }
        self.s.mouse = None;
        self.s.tracking = false;
        self.drag_hover = false;
        self.s.hover_on = false;
        self.s.hover_in = None;
        self.s.hover_out = None;
        self.s.hov = 0.0;
        self.s.hov_v = 0.0;
        self.s.isl_px = (0, 0, 0, 0);
    }

    /// Tray click or a second launch: unhide and open the island.
    pub fn show_and_open(&mut self) {
        match self.leaving {
            Some((Leave::Quit, _, _)) => return,
            // changed their mind halfway through hiding
            Some((Leave::Hide, _, _)) => self.leaving = None,
            None => {}
        }
        self.user_hidden = false;
        self.apply_visibility();
        // on the display the mouse is on
        let h = self.island_at_cursor();
        self.on(h, |a| {
            if !a.s.hidden {
                a.expand(None);
            }
        });
    }

    // -------------------------------------------------------------- displays

    /// The island being worked on, then the others.
    fn surfaces(&self) -> impl Iterator<Item = &Surface> {
        std::iter::once(&self.s).chain(self.others.iter())
    }

    /// Run `f` for every island in turn (each one as `self.s`, with its display's scale).
    fn each(&mut self, mut f: impl FnMut(&mut App)) {
        self.gfx.set_scale(self.s.display.scale);
        f(self);
        for i in 0..self.others.len() {
            std::mem::swap(&mut self.s, &mut self.others[i]);
            self.gfx.set_scale(self.s.display.scale);
            f(self);
            std::mem::swap(&mut self.s, &mut self.others[i]);
        }
        self.gfx.set_scale(self.s.display.scale);
    }

    /// Run `f` for the island in window `hwnd` (as `self.s`); None when it is not one of them.
    pub fn on<T>(&mut self, hwnd: HWND, f: impl FnOnce(&mut App) -> T) -> Option<T> {
        if self.s.hwnd == hwnd {
            return Some(f(self));
        }
        let i = self.others.iter().position(|o| o.hwnd == hwnd)?;
        std::mem::swap(&mut self.s, &mut self.others[i]);
        self.gfx.set_scale(self.s.display.scale);
        let r = f(self);
        std::mem::swap(&mut self.s, &mut self.others[i]);
        self.gfx.set_scale(self.s.display.scale);
        Some(r)
    }

    fn all_hidden(&self) -> bool {
        self.surfaces().all(|o| o.hidden)
    }

    /// Something is being dragged over this island (the one the panel opens on).
    fn drag_here(&self) -> bool {
        self.drag_hover && self.s.hwnd == self.home
    }

    /// The window of the island on the display the mouse is on (the primary one otherwise).
    fn island_at_cursor(&self) -> HWND {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
            let hm = MonitorFromPoint(p, MONITOR_DEFAULTTOPRIMARY);
            self.surfaces().find(|o| o.display.handle == hm && !o.hidden).map(|o| o.hwnd).unwrap_or(self.hwnd)
        }
    }

    /// The largest window any island has (the shared frame buffer only shrinks to this).
    fn biggest_win(&self) -> (i32, i32) {
        self.surfaces().fold((0, 0), |(w, h), o| (w.max(o.win.2), h.max(o.win.3)))
    }

    /// A hover that a resize or a lost WM_MOUSELEAVE left behind on this island, put right.
    /// Returns true when the cursor turned out to be resting on it.
    fn check_hover(&mut self, n: Instant) -> bool {
        // a resize under a still cursor can produce a spurious WM_MOUSELEAVE: recover hover
        // (not while it is a line: it opens only on a real mouse movement over the line, and
        // the island rectangle may still be the size of the panel that just closed)
        let settling = self.s.no_hover_until.map(|t| n < t).unwrap_or(false);
        if self.s.mouse.is_none()
            && !self.s.hidden
            && self.s.view != ViewKey::Line
            && !settling
            && self.cursor_inside()
            && self.cursor_on_window()
        {
            let mut p = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut p);
            }
            let s = self.gfx.scale;
            self.s.mouse = Some(((p.x - self.s.win.0) as f32 / s, (p.y - self.s.win.1) as f32 / s));
            self.s.tracking = false;
            self.track_leave();
            return true;
        }
        // the other way round: a WM_MOUSELEAVE can get lost (a full-screen game taking the
        // mouse, for one), and then nothing would ever end the hover
        if self.s.mouse.is_some() && !self.demo && !self.s.hidden && !self.cursor_on_window() {
            let held = unsafe { GetCapture() } == self.s.hwnd;
            if !held {
                dbg_log("hover: lost WM_MOUSELEAVE, ending it");
                self.on_mouse_leave();
            }
        }
        false
    }

    /// One island per display: windows for new displays, none for unplugged ones, and every
    /// island moved to where its display is now.
    fn sync_displays(&mut self) {
        let list = displays();
        let Some(primary) = list.first().copied() else { return };
        dbg_log(&format!(
            "displays: {}",
            list.iter().map(|d| format!("[{:?} x{} top-centre {},{}]", d.handle.0, d.scale, d.cx, d.top)).collect::<Vec<_>>().join(" ")
        ));
        // the main window stays on the primary display
        self.s.display = primary;
        self.s.mon_cx = primary.cx;
        self.s.mon_top = primary.top;
        let rest = &list[1..];
        // islands of displays that are gone
        let gone: Vec<HWND> = self.others.iter().filter(|o| !rest.iter().any(|d| d.handle == o.display.handle)).map(|o| o.hwnd).collect();
        for h in gone {
            dbg_log("displays: an island goes");
            if h == self.home {
                self.home = self.hwnd;
                if self.expanded {
                    self.collapse();
                }
            }
            self.others.retain(|o| o.hwnd != h);
            shelf::unregister(h);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        for d in rest {
            if let Some(o) = self.others.iter_mut().find(|o| o.display.handle == d.handle) {
                o.display = *d;
                o.mon_cx = d.cx;
                o.mon_top = d.top;
                continue;
            }
            let Some(h) = crate::island_window() else { continue };
            dbg_log("displays: a new island");
            shelf::register(h);
            let mut o = Surface::new(h, *d);
            // shown (or kept hidden) by apply_visibility below, which pops it in
            o.hidden = true;
            self.others.push(o);
        }
        self.gfx.set_scale(self.s.display.scale);
        self.apply_visibility();
        self.kick();
    }
}

fn cursor_in((l, t, w, h): (i32, i32, i32, i32)) -> bool {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p.x >= l && p.x < l + w && p.y >= t && p.y < t + h
}

// ------------------------------------------------------------------ widgets

fn tab_color(t: Tab) -> C {
    match t {
        Tab::Pet => HEART_C,
        Tab::Media => WHITE,
        Tab::Shelf => SHELF_C,
        Tab::Tasks => TASK_C,
        Tab::Clipboard => CLIP_C,
    }
}

fn draw_tab_icon(g: &Gfx, t: Tab, r: R, col: C) {
    match t {
        // drawn by draw_expanded (it is the cat itself)
        Tab::Pet => {}
        Tab::Media => g.icon(ic::MUSIC, r, 14.0, col),
        Tab::Shelf => g.icon(ic::FOLDER, r, 14.0, col),
        Tab::Tasks => g.icon(ic::TASKS, r, 14.0, col),
        Tab::Clipboard => g.icon(ic::CLIP, r, 14.0, col),
    }
}

fn draw_battery(g: &Gfx, x: f32, cy: f32, w: f32, h: f32, f: f32, charging: bool) {
    let body = R::new(x, cy - h / 2.0, w, h);
    g.rrect_stroke(body, 3.0, WHITE.a(0.55), 1.2, false);
    g.rrect(R::new(body.right() + 1.0, cy - h * 0.2, 2.0, h * 0.4), 1.0, WHITE.a(0.55));
    let col = if charging {
        GREEN
    } else if f <= 0.2 {
        RED
    } else {
        WHITE
    };
    let inner = body.inset(2.2, 2.2);
    g.rrect(R::new(inner.x, inner.y, (inner.w * f.clamp(0.04, 1.0)).max(1.5), inner.h), 1.5, col);
}

/// Background of a regular button (theme.conf `button` / `button_hover`).
fn surf(g: &Gfx, hover: bool) -> C {
    let t = &g.theme;
    match (hover, t.button, t.button_hover) {
        (true, _, Some(h)) => h,
        (true, Some(b), None) => b.mix(t.fg, 0.18),
        (true, None, None) => SURF_H,
        (false, Some(b), _) => b,
        (false, None, _) => SURF,
    }
}

/// Background of the selected button (theme.conf `button_active`); hovering never changes it.
fn surf_active(g: &Gfx) -> C {
    let t = &g.theme;
    match (t.button_active, t.button) {
        (Some(a), _) => a,
        (None, Some(b)) => b.mix(t.fg, 0.18),
        (None, None) => SURF_H,
    }
}

/// Text color on a regular button (theme.conf `button_text`), keeping `c`'s transparency.
fn btn_fg(g: &Gfx, c: C) -> C {
    match g.theme.button_text {
        Some(t) => g.text_alpha(C(t.0, t.1, t.2, t.3 * c.3)),
        None => c,
    }
}

/// Icon color on a regular button: theme.conf `icon` wins (applied when drawn), else `button_text`.
fn btn_icon(g: &Gfx, c: C) -> C {
    if g.theme.icon.is_some() {
        c
    } else {
        btn_fg(g, c)
    }
}

/// Icon of the open tab / a switched-on toggle: the feature's color unless theme.conf sets `icon_active`.
fn active_icon(g: &Gfx, feature: C) -> C {
    match g.theme.icon_active {
        Some(a) => C(a.0, a.1, a.2, a.3 * feature.3),
        None => feature,
    }
}

/// Fill of a main button: the feature's color unless theme.conf sets `button_accent`.
fn accent_btn(g: &Gfx, feature: C) -> C {
    g.theme.button_accent.unwrap_or(feature)
}

/// Text on a main button: theme.conf `button_accent_text`, else black, or white when a custom
/// `button_accent` is dark.
fn accent_fg(g: &Gfx) -> C {
    if let Some(t) = g.theme.button_accent_text {
        return t;
    }
    match g.theme.button_accent {
        Some(a) if crate::theme::luma(std::iter::once(a)) < 0.5 => C(0.9999, 0.9999, 0.9999, 1.0),
        _ => BLACK,
    }
}

fn icon_btn(g: &Gfx, ui: &mut Ui, r: R, ch: char, size: f32, col: C, act: Act, bg: bool) {
    let hv = ui.hit(r, act);
    if hv || bg {
        g.rrect(r, (r.h / 2.0).min(11.0), surf(g, hv));
    }
    g.icon(ch, r, size, btn_icon(g, col));
}

/// A number shown in place; while being edited it becomes a small text box with a caret.
fn value_box(g: &Gfx, r: R, val: &str, edit: Option<(&str, bool)>, hover: bool, size: f32) {
    match edit {
        Some((buf, caret)) => {
            g.rrect(r.inset(0.0, 2.0), 7.0, WHITE.a(0.14));
            g.rrect_stroke(r.inset(0.5, 2.5), 6.5, WHITE.a(0.55), 1.0, false);
            let tw = g.measure(buf, size, 600);
            let tx = r.cx() - tw / 2.0;
            g.text(buf, R::new(tx, r.y, tw + 4.0, r.h), size, 600, WHITE, Al::L);
            if caret {
                g.line(tx + tw + 1.5, r.cy() - 6.0, tx + tw + 1.5, r.cy() + 6.0, 1.3, WHITE);
            }
        }
        None => {
            if hover {
                g.rrect(r.inset(0.0, 2.0), 7.0, WHITE.a(0.08));
            }
            g.text(val, r, size, 600, btn_fg(g, WHITE), Al::C);
        }
    }
}

/// Task time: "" clears it; "14:30", "14.30", "1430", "9" set a clock time (tomorrow if
/// already past); "30m", "2h", "1h30" are relative to now. None means "couldn't parse".
#[cfg(test)]
fn parse_due(s: &str, now: i64) -> Option<Option<i64>> {
    parse_due_on(s, now, now.div_euclid(86400))
}

/// Like parse_due, but a clock time lands on `day` when that is a later day than today.
fn parse_due_on(s: &str, now: i64, day: i64) -> Option<Option<i64>> {
    let s: String = s.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return Some(None);
    }
    if s.contains('h') || s.contains('m') {
        let (mut total, mut num, mut seen_h) = (0i64, String::new(), false);
        for c in s.chars() {
            match c {
                '0'..='9' => num.push(c),
                'h' => {
                    total += num.parse::<i64>().ok()? * 3600;
                    num.clear();
                    seen_h = true;
                }
                'm' => {
                    total += num.parse::<i64>().ok()? * 60;
                    num.clear();
                }
                _ => return None,
            }
        }
        if !num.is_empty() {
            // "1h30" -> trailing minutes
            if !seen_h {
                return None;
            }
            total += num.parse::<i64>().ok()? * 60;
        }
        return (total > 0).then(|| Some((now + total) / 60 * 60));
    }
    let (h, m) = if let Some((a, b)) = s.split_once([':', '.']) {
        (a.parse::<i64>().ok()?, b.parse::<i64>().ok()?)
    } else {
        let n = s.parse::<i64>().ok()?;
        match s.len() {
            1 | 2 => (n, 0),
            3 | 4 => (n / 100, n % 100),
            _ => return None,
        }
    };
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    if day > now.div_euclid(86400) {
        return Some(Some(day * 86400 + h * 3600 + m * 60));
    }
    let day = now.div_euclid(86400) * 86400;
    let mut t = day + h * 3600 + m * 60;
    if t <= now {
        t += 86400;
    }
    Some(Some(t))
}

fn ext_color(ext: &str) -> C {
    match ext {
        "PDF" => C::hex(0xE5484D),
        "DOC" | "DOCX" | "RTF" | "TXT" | "MD" => C::hex(0x3E7BFA),
        "XLS" | "XLSX" | "CSV" => C::hex(0x30A46C),
        "PPT" | "PPTX" | "KEY" => C::hex(0xF76B15),
        "PNG" | "JPG" | "JPEG" | "GIF" | "WEBP" | "BMP" | "SVG" | "HEIC" => C::hex(0x8E4EC6),
        "ZIP" | "RAR" | "7Z" | "TAR" | "GZ" => C::hex(0xC9A227),
        "MP3" | "WAV" | "FLAC" | "M4A" | "OGG" => C::hex(0xD6409F),
        "MP4" | "MOV" | "MKV" | "AVI" | "WEBM" => C::hex(0x12A594),
        "EXE" | "MSI" => C::hex(0x6E6E73),
        _ => C::hex(0x5B5B66),
    }
}

/// "now", "5m", "2h", "3d": how long ago `secs` seconds were.
fn ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        "now".into()
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}

fn clipboard_text() -> Option<String> {
    unsafe {
        OpenClipboard(HWND::default()).ok()?;
        let r = (|| {
            let h = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let hg = HGLOBAL(h.0);
            let p = GlobalLock(hg) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut n = 0usize;
            while *p.add(n) != 0 && n < 4096 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(hg);
            Some(s)
        })();
        let _ = CloseClipboard();
        r
    }
}

/// Testing aid: with PATPET_DEBUG, how many islands were drawn each minute and how long that
/// took, to keep an eye on the CPU an idle cat costs.
mod stats {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    use std::time::Instant;

    pub static RENDERS: AtomicU64 = AtomicU64::new(0);
    pub static RENDER_US: AtomicU64 = AtomicU64::new(0);
    /// of that, handing the picture to Windows (EndDraw + UpdateLayeredWindow)
    pub static END_US: AtomicU64 = AtomicU64::new(0);
    /// of those, only the cat
    pub static PARTS: AtomicU64 = AtomicU64::new(0);
    static SINCE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

    pub fn add(count: &AtomicU64, us: &AtomicU64, t: Instant) {
        count.fetch_add(1, Relaxed);
        us.fetch_add(t.elapsed().as_micros() as u64, Relaxed);
    }

    pub fn report() {
        if std::env::var_os("PATPET_DEBUG").is_none() {
            return;
        }
        let mut since = SINCE.lock().unwrap_or_else(|e| e.into_inner());
        let start = *since.get_or_insert_with(Instant::now);
        let secs = start.elapsed().as_secs_f32();
        if secs < 60.0 {
            return;
        }
        *since = Some(Instant::now());
        let (n, us, end) = (RENDERS.swap(0, Relaxed), RENDER_US.swap(0, Relaxed), END_US.swap(0, Relaxed));
        let parts = PARTS.swap(0, Relaxed);
        let each = |us: u64| us as f32 / 1000.0 / n.max(1) as f32;
        super::dbg_log(&format!(
            "draws: {n} in {secs:.0} s ({:.1}/s, {parts} only the cat), {:.2} ms each ({:.2} ms handing over), {:.2} % of a core",
            n as f32 / secs,
            each(us),
            each(end),
            us as f32 / 10_000.0 / secs
        ));
    }
}

pub fn dbg_log(s: &str) {
    if std::env::var_os("PATPET_DEBUG").is_some() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("patpet.log")) {
            let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100_000_000).unwrap_or(0);
            let _ = writeln!(f, "{ms:>9} {s}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{left_label, parse_due, parse_due_on, passed_alerts, reminder_due};

    // 2026-11-03 20:00:00 in the naive-local epoch used by the app
    const NOW: i64 = 20760 * 86400 + 20 * 3600;
    const DAY: i64 = 20760 * 86400;

    #[test]
    fn clock_times() {
        assert_eq!(parse_due("21:30", NOW), Some(Some(DAY + 21 * 3600 + 30 * 60)));
        assert_eq!(parse_due("21.30", NOW), Some(Some(DAY + 21 * 3600 + 30 * 60)));
        assert_eq!(parse_due("2130", NOW), Some(Some(DAY + 21 * 3600 + 30 * 60)));
        // already past today -> tomorrow
        assert_eq!(parse_due("930", NOW), Some(Some(DAY + 86400 + 9 * 3600 + 30 * 60)));
        assert_eq!(parse_due("9", NOW), Some(Some(DAY + 86400 + 9 * 3600)));
        assert_eq!(parse_due("25:00", NOW), None);
        assert_eq!(parse_due("12:61", NOW), None);
    }

    #[test]
    fn relative_and_clear() {
        assert_eq!(parse_due("30m", NOW), Some(Some(NOW + 1800)));
        assert_eq!(parse_due("2h", NOW), Some(Some(NOW + 7200)));
        assert_eq!(parse_due("1h30", NOW), Some(Some(NOW + 5400)));
        assert_eq!(parse_due("1h 30m", NOW), Some(Some(NOW + 5400)));
        assert_eq!(parse_due("", NOW), Some(None));
        assert_eq!(parse_due("abc", NOW), None);
    }

    #[test]
    fn clock_time_on_a_later_day() {
        assert_eq!(parse_due_on("14:30", NOW, 20765), Some(Some(20765 * 86400 + 14 * 3600 + 30 * 60)));
        // relative times stay relative to now
        assert_eq!(parse_due_on("30m", NOW, 20765), Some(Some(NOW + 1800)));
    }

    #[test]
    fn reminders() {
        let due = NOW + 2 * 86400;
        // nothing yet two days ahead
        assert_eq!(reminder_due(due, NOW, 0), (None, 0));
        // one day before: the 1-day notice
        let (w, bits) = reminder_due(due, due - 86400, 0);
        assert_eq!(w, Some(0));
        // the same again does nothing
        assert_eq!(reminder_due(due, due - 86000, bits), (None, bits));
        // 6 h, 3 h, 1 h, 30 min
        let (w, bits) = reminder_due(due, due - 6 * 3600, bits);
        assert_eq!(w, Some(1));
        let (w, bits) = reminder_due(due, due - 3 * 3600, bits);
        assert_eq!(w, Some(2));
        let (w, bits) = reminder_due(due, due - 3600, bits);
        assert_eq!(w, Some(3));
        let (w, bits) = reminder_due(due, due - 1800, bits);
        assert_eq!(w, Some(4));
        assert_eq!(bits, 0b11111);
        // asleep (or in a game) from 5 h to 40 min before: only the closest one, once
        let (w, bits) = reminder_due(due, due - 40 * 60, 0b1);
        assert_eq!(w, Some(3));
        assert_eq!(bits, 0b1111);
        // overdue: none of them (the due-now notice takes over)
        assert_eq!(reminder_due(due, due + 5, 0), (None, 0b11111));
        // a task added 2 h before its deadline skips the 1-day, 6 h and 3 h ones
        assert_eq!(passed_alerts(Some(NOW + 7200), NOW), 0b111);
        assert_eq!(passed_alerts(None, NOW), 0);
        assert_eq!(left_label(86400), "in 1 day");
        assert_eq!(left_label(6 * 3600), "in 6 hours");
        assert_eq!(left_label(3 * 3600 - 1), "in 3 hours");
        assert_eq!(left_label(3600), "in 1 hour");
        assert_eq!(left_label(1800), "in 30 minutes");
        assert_eq!(left_label(40 * 60), "in 40 minutes");
    }
}
