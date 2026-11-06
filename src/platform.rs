//! Platform Slint di atas Win32.
//!
//! Satu jendela layered (alpha per piksel). Renderer software Slint menggambar
//! langsung ke memori DIB section, lalu `UpdateLayeredWindow` menampilkannya.
//! Tanpa OpenGL/GPU context, jadi RAM tetap kecil. Piksel yang transparan
//! penuh otomatis meneruskan klik ke jendela di belakangnya.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::platform::software_renderer::{
    PremultipliedRgbaColor, RepaintBufferType, SoftwareRenderer, TargetPixel,
};
use slint::platform::{
    EventLoopProxy, Platform, PointerEventButton, Renderer, WindowAdapter, WindowEvent,
    WindowProperties,
};
use slint::{EventLoopError, LogicalPosition, LogicalSize, PhysicalSize, PlatformError};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW,
    MonitorFromPoint, MonitorFromWindow, ScreenToClient, SelectObject, MONITOR_DEFAULTTONEAREST, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFO,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetLastInputInfo, ReleaseCapture, SetCapture, TrackMouseEvent, LASTINPUTINFO, TME_LEAVE,
    TRACKMOUSEEVENT, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_CAPITAL, VK_HOME, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::placement::Rect;
use crate::{clipboard, dragdrop, tray};

/// Pesan untuk membangunkan loop saat ada kerja dari thread lain.
const WM_WAKE: u32 = WM_APP + 1;
/// Tidak diekspor modul WindowsAndMessaging di crate `windows`.
const WM_MOUSELEAVE: u32 = 0x02A3;
/// Jarak gulir per satu klik roda mouse, dalam piksel logis.
const WHEEL_STEP: f32 = 40.0;

/// Kejadian sistem yang diteruskan ke aplikasi.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    TrayClick,
    TrayMenu,
    /// Pengguna mengklik di luar jendela.
    Deactivated,
    Escape,
    /// Charger dicolok/dicabut atau persentase baterai berubah.
    PowerChanged,
    ScaleChanged(f32),
    /// Resolusi atau susunan monitor berubah.
    DisplayChanged,
    /// Kecerahan layar berubah, dalam persen.
    Brightness(u32),
    /// Mode hemat baterai nyala/mati.
    BatterySaver(bool),
    CapsLock(bool),
    /// Isi clipboard berubah (oleh aplikasi mana pun).
    ClipboardChanged,
    /// File sedang diseret di atas island (`true`) atau sudah pergi (`false`).
    DragHover(bool),
    FilesDropped(Vec<PathBuf>),
}

thread_local! {
    static HANDLER: RefCell<Option<Rc<dyn Fn(Event)>>> = RefCell::default();
}

/// Mendaftarkan penerima [`Event`]. Hanya satu; pendaftaran baru menggantikan yang lama.
pub fn on_event(handler: impl Fn(Event) + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some(Rc::new(handler)));
}

fn emit(event: Event) {
    // Klon dulu supaya handler bebas memanggil fungsi platform lain.
    if let Some(handler) = HANDLER.with(|h| h.borrow().clone()) {
        handler(event);
    }
}

/// Seret file `path` keluar dari jendela. Memblokir sampai dilepas; panggil
/// saat tombol mouse masih ditekan. `true` bila Explorer menerimanya.
pub fn drag_out(path: &Path) -> bool {
    let Some(w) = current_window() else { return false };
    let Some(hwnd) = w.hwnd.get() else { return false };
    let moved = dragdrop::drag_out(hwnd, path);
    // Loop seret OLE menelan pelepasan tombol; beri tahu Slint supaya
    // kartu yang diseret tidak tetap dalam keadaan ditekan.
    let mut at = POINT::default();
    // SAFETY: hwnd valid, titik milik kita.
    unsafe {
        let _ = GetCursorPos(&mut at);
        let _ = ScreenToClient(hwnd, &mut at);
        let _ = ReleaseCapture();
    }
    let scale = w.scale.get();
    let position = LogicalPosition::new(at.x as f32 / scale, at.y as f32 / scale);
    w.window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
    w.window.dispatch_event(WindowEvent::PointerExited);
    w.tracking_mouse.set(false);
    moved
}

/// Isi clipboard untuk riwayat (lihat [`clipboard::read_for_history`]).
pub fn clipboard_content() -> Option<clipboard::Content> {
    clipboard::read_for_history(current_hwnd())
}

pub fn set_clipboard_image(image: &crate::image::Rgba, png: &[u8]) {
    clipboard::set_image(current_hwnd(), image, png);
}

pub fn set_clipboard_text(text: &str) {
    clipboard::set(current_hwnd(), text);
}

fn current_hwnd() -> Option<HWND> {
    CURRENT.with(|c| c.borrow().upgrade()).and_then(|w| w.hwnd.get())
}

/// Menjadikan jendela aktif supaya klik di luar bisa terdeteksi.
pub fn activate() {
    if let Some(hwnd) = current_hwnd() {
        // SAFETY: hwnd valid.
        let _ = unsafe { SetForegroundWindow(hwnd) };
    }
}

/// Menampilkan menu tray di posisi kursor; mengembalikan id item yang dipilih.
/// Letak jendela di layar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Tengah atas monitor (mode bar).
    TopCenter,
    /// Kiri-atas jendela di titik ini, piksel fisik (mode ikon).
    At(i32, i32),
}

fn current_window() -> Option<Rc<LayeredWindow>> {
    CURRENT.with(|c| c.borrow().upgrade())
}

pub fn place(placement: Placement) {
    if let Some(w) = current_window() {
        if w.placement.replace(placement) != placement {
            w.relayout();
        }
    }
}

/// Lebar monitor tempat jendela berada, dalam piksel logis.
pub fn monitor_width() -> f32 {
    let Some(w) = current_window() else { return 1536.0 };
    let Some(hwnd) = w.hwnd.get() else { return 1536.0 };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: hwnd valid, info milik kita.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let _ = GetMonitorInfoW(monitor, &mut info);
    }
    (info.rcMonitor.right - info.rcMonitor.left) as f32 / w.scale.get()
}

/// Skala layar jendela saat ini (1.25 = 125%).
pub fn scale() -> f32 {
    current_window().map_or(1.0, |w| w.scale.get())
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    // SAFETY: titik milik kita.
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// Area kerja (tanpa taskbar) monitor yang memuat titik ini.
pub fn work_area_at(x: i32, y: i32) -> Rect {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: info milik kita; MonitorFromPoint selalu mengembalikan monitor.
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let _ = GetMonitorInfoW(monitor, &mut info);
    }
    let r = info.rcWork;
    Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

pub fn set_topmost(on: bool) {
    if let Some(w) = current_window() {
        w.topmost.set(on);
        w.relayout();
    }
}

/// Sembunyikan tanpa mengubah status jendela Slint (mis. saat ada aplikasi layar penuh).
pub fn set_hidden(hidden: bool) {
    if let Some(hwnd) = current_hwnd() {
        // SAFETY: hwnd valid.
        unsafe {
            let _ = ShowWindow(hwnd, if hidden { SW_HIDE } else { SW_SHOWNOACTIVATE });
        }
    }
}

/// Ada aplikasi layar penuh (video, game, presentasi) di depan.
pub fn fullscreen_app_active() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    // SAFETY: tanpa prasyarat.
    unsafe { SHQueryUserNotificationState() }
        .is_ok_and(|s| s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE)
}

pub fn show_menu(items: &[tray::MenuItem]) -> Option<u32> {
    tray::menu(current_hwnd()?, items)
}

pub fn install() -> Result<(), PlatformError> {
    // SAFETY: dipanggil sekali di thread UI sebelum jendela apa pun dibuat.
    // DPI: gagal berarti sudah diatur lewat manifest; aman diabaikan.
    // OLE (COM STA): dibutuhkan Core Audio dan drag & drop; berlaku sampai proses selesai.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = windows::Win32::System::Ole::OleInitialize(None);
    }
    slint::platform::set_platform(Box::new(WinPlatform::default()))
        .map_err(|e| PlatformError::Other(e.to_string()))
}

/// Lama pengguna tidak menyentuh mouse atau keyboard.
pub fn user_idle() -> Duration {
    let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: struct milik kita dengan cbSize yang benar.
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return Duration::ZERO;
    }
    let now = unsafe { GetTickCount() };
    Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
}

// ---------------------------------------------------------------------------
// Event loop

type Job = Box<dyn FnOnce() + Send>;

struct WinPlatform {
    thread_id: u32,
    jobs: Arc<Mutex<Vec<Job>>>,
    window: RefCell<Weak<LayeredWindow>>,
}

impl Default for WinPlatform {
    fn default() -> Self {
        Self {
            // SAFETY: tanpa prasyarat.
            thread_id: unsafe { GetCurrentThreadId() },
            jobs: Arc::default(),
            window: RefCell::default(),
        }
    }
}

impl Platform for WinPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = LayeredWindow::new()?;
        *self.window.borrow_mut() = Rc::downgrade(&window);
        Ok(window)
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        let mut msg = MSG::default();
        loop {
            slint::platform::update_timers_and_animations();

            let jobs = std::mem::take(&mut *self.jobs.lock().unwrap());
            for job in jobs {
                job();
            }

            let window = self.window.borrow().upgrade();
            let wait = match &window {
                Some(w) => {
                    w.draw();
                    if w.window.has_active_animations() {
                        Some(w.frame_time.get())
                    } else {
                        slint::platform::duration_until_next_timer_update()
                    }
                }
                None => slint::platform::duration_until_next_timer_update(),
            };
            drop(window);

            // Bulatkan ke atas supaya tidak bangun sedikit lebih awal lalu berputar kosong.
            let ms = wait.map_or(INFINITE, |d| d.as_micros().div_ceil(1000).min(u32::MAX as u128 - 1) as u32);
            // SAFETY: menunggu pesan pada thread ini saja.
            unsafe { MsgWaitForMultipleObjects(None, false, ms, QS_ALLINPUT) };

            // SAFETY: pompa pesan standar pada thread pemilik jendela.
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }

    fn set_clipboard_text(&self, text: &str, clipboard: slint::platform::Clipboard) {
        if clipboard == slint::platform::Clipboard::DefaultClipboard {
            clipboard::set(current_hwnd(), text);
        }
    }

    fn clipboard_text(&self, clipboard: slint::platform::Clipboard) -> Option<String> {
        (clipboard == slint::platform::Clipboard::DefaultClipboard).then(|| clipboard::get(current_hwnd())).flatten()
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(Proxy { thread_id: self.thread_id, jobs: self.jobs.clone() }))
    }
}

struct Proxy {
    thread_id: u32,
    jobs: Arc<Mutex<Vec<Job>>>,
}

impl Proxy {
    fn post(&self, msg: u32) -> Result<(), EventLoopError> {
        // SAFETY: mengirim pesan ke antrean thread UI; tidak menyentuh memori kita.
        unsafe { PostThreadMessageW(self.thread_id, msg, WPARAM(0), LPARAM(0)) }
            .map_err(|_| EventLoopError::EventLoopTerminated)
    }
}

impl EventLoopProxy for Proxy {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        self.post(WM_QUIT)
    }

    fn invoke_from_event_loop(&self, job: Job) -> Result<(), EventLoopError> {
        self.jobs.lock().unwrap().push(job);
        self.post(WM_WAKE)
    }
}

// ---------------------------------------------------------------------------
// Jendela

/// Piksel BGRA premultiplied, urutan yang dipakai DIB 32-bit dan UpdateLayeredWindow.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Bgra {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
}

impl TargetPixel for Bgra {
    fn blend(&mut self, c: PremultipliedRgbaColor) {
        let keep = 255 - c.alpha as u16;
        let mix = |dst: u8, src: u8| (src as u16 + (dst as u16 * keep + 127) / 255) as u8;
        self.r = mix(self.r, c.red);
        self.g = mix(self.g, c.green);
        self.b = mix(self.b, c.blue);
        self.a = mix(self.a, c.alpha);
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { b, g, r, a: 255 }
    }

    fn background() -> Self {
        Self::default()
    }
}

/// DIB section tempat Slint menggambar. Dilepas otomatis lewat Drop.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut Bgra,
    width: u32,
    height: u32,
}

impl Surface {
    fn new(width: u32, height: u32) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // negatif = baris atas lebih dulu
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: semua handle yang dibuat di sini dilepas di Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let previous = SelectObject(dc, bitmap.into());
            Some(Self { dc, bitmap, previous, bits: bits.cast(), width, height })
        }
    }

    fn pixels(&mut self) -> &mut [Bgra] {
        // SAFETY: DIB berisi width*height piksel 32-bit dan hidup selama self.
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: handle dibuat di Surface::new dan hanya dilepas di sini.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

thread_local! {
    /// Jendela yang sedang menerima pesan Win32 di thread UI.
    static CURRENT: RefCell<Weak<LayeredWindow>> = RefCell::default();
}

struct LayeredWindow {
    me: Weak<Self>,
    window: slint::Window,
    renderer: SoftwareRenderer,
    hwnd: Cell<Option<HWND>>,
    scale: Cell<f32>,
    /// Ukuran logis yang diminta layout Slint.
    logical: Cell<LogicalSize>,
    surface: RefCell<Option<Surface>>,
    dirty: Cell<bool>,
    tracking_mouse: Cell<bool>,
    high_surrogate: Cell<Option<u16>>,
    frame_time: Cell<Duration>,
    tray_icon: Cell<Option<HICON>>,
    placement: Cell<Placement>,
    topmost: Cell<bool>,
    watchers: RefCell<Option<Watchers>>,
    /// Laporan pertama tiap pengaturan daya hanya nilai awal; jangan diberitahukan.
    power_seen: Cell<u8>,
    /// Dikirim Explorer setelah restart; ikon tray harus ditambahkan ulang.
    taskbar_created: u32,
}

impl LayeredWindow {
    fn new() -> Result<Rc<Self>, PlatformError> {
        Ok(Rc::new_cyclic(|me: &Weak<Self>| Self {
            me: me.clone(),
            window: slint::Window::new(me.clone() as Weak<dyn WindowAdapter>),
            renderer: SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::ReusedBuffer),
            hwnd: Cell::new(None),
            scale: Cell::new(1.0),
            logical: Cell::new(LogicalSize::new(0.0, 0.0)),
            surface: RefCell::new(None),
            dirty: Cell::new(true),
            tracking_mouse: Cell::new(false),
            high_surrogate: Cell::new(None),
            frame_time: Cell::new(Duration::from_millis(16)),
            tray_icon: Cell::new(None),
            placement: Cell::new(Placement::TopCenter),
            topmost: Cell::new(true),
            watchers: RefCell::new(None),
            power_seen: Cell::new(0),
            // SAFETY: tanpa prasyarat.
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        }))
    }

    fn create(&self) -> Result<HWND, PlatformError> {
        let fail = |e: windows::core::Error| PlatformError::Other(e.to_string());
        // SAFETY: registrasi kelas dan pembuatan jendela standar Win32.
        unsafe {
            let instance = GetModuleHandleW(None).map_err(fail)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
                lpszClassName: w!("PatPetWindow"),
                ..Default::default()
            };
            RegisterClassW(&class); // gagal bila sudah terdaftar; tidak masalah

            let hwnd = CreateWindowExW(
                // Tanpa WS_EX_NOACTIVATE: jendela harus bisa menjadi foreground
                // supaya menu tray dan deteksi klik di luar panel bekerja.
                // Klik biasa tetap tidak mencuri fokus (lihat WM_MOUSEACTIVATE).
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("PatPetWindow"),
                w!("PatPet"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(fail)?;

            self.scale.set(GetDpiForWindow(hwnd) as f32 / 96.0);
            self.frame_time.set(refresh_interval());
            self.tray_icon.set(tray::add(hwnd));
            *self.watchers.borrow_mut() = Some(Watchers::start(hwnd));
            Ok(hwnd)
        }
    }

    fn physical_size(&self) -> PhysicalSize {
        self.logical.get().to_physical(self.scale.get())
    }

    /// Menyesuaikan buffer dan posisi jendela dengan ukuran logis dan skala saat ini.
    fn relayout(&self) {
        let Some(hwnd) = self.hwnd.get() else { return };
        let size = self.physical_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let resized = self
            .surface
            .borrow()
            .as_ref()
            .is_none_or(|s| (s.width, s.height) != (size.width, size.height));
        if resized {
            *self.surface.borrow_mut() = Surface::new(size.width, size.height);
            self.window.dispatch_event(WindowEvent::Resized { size: self.logical.get() });
        }

        let (x, y) = match self.placement.get() {
            Placement::At(x, y) => (x, y),
            Placement::TopCenter => {
                // Tengah atas monitor tempat jendela berada.
                let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
                // SAFETY: hwnd valid, info milik kita.
                unsafe {
                    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
                    let _ = GetMonitorInfoW(monitor, &mut info);
                }
                let RECT { left, top, right, .. } = info.rcMonitor;
                (left + (right - left - size.width as i32) / 2, top)
            }
        };
        let z = if self.topmost.get() { HWND_TOPMOST } else { HWND_NOTOPMOST };
        // SAFETY: hwnd valid.
        unsafe {
            let _ = SetWindowPos(hwnd, Some(z), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
        self.dirty.set(true);
    }

    fn set_scale(&self, scale: f32) {
        if scale == self.scale.get() {
            return;
        }
        self.scale.set(scale);
        self.window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
        self.relayout();
        emit(Event::ScaleChanged(scale));
    }

    fn draw(&self) {
        if !self.dirty.take() {
            return;
        }
        let (Some(hwnd), Some(surface)) = (self.hwnd.get(), &mut *self.surface.borrow_mut()) else {
            return;
        };
        let stride = surface.width as usize;
        self.renderer.render(surface.pixels(), stride);

        let size = SIZE { cx: surface.width as i32, cy: surface.height as i32 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
            ..Default::default()
        };
        // SAFETY: dc berisi bitmap seukuran `size`; hwnd adalah jendela layered.
        unsafe {
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                None,
                Some(&size),
                Some(surface.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    fn position(&self, lparam: LPARAM) -> LogicalPosition {
        let x = (lparam.0 & 0xFFFF) as i16 as f32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
        let scale = self.scale.get();
        LogicalPosition::new(x / scale, y / scale)
    }

    fn handle(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        let button = match msg {
            WM_LBUTTONDOWN | WM_LBUTTONUP => PointerEventButton::Left,
            WM_RBUTTONDOWN | WM_RBUTTONUP => PointerEventButton::Right,
            _ => PointerEventButton::Other,
        };
        let event = match msg {
            WM_MOUSEMOVE => {
                if !self.tracking_mouse.replace(true) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: struct milik kita, hwnd valid.
                    let _ = unsafe { TrackMouseEvent(&mut track) };
                }
                WindowEvent::PointerMoved { position: self.position(lparam) }
            }
            WM_MOUSELEAVE => {
                self.tracking_mouse.set(false);
                WindowEvent::PointerExited
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                // SAFETY: hwnd valid; capture memastikan tombol dilepas tetap sampai ke sini.
                unsafe { SetCapture(hwnd) };
                WindowEvent::PointerPressed { position: self.position(lparam), button }
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                // SAFETY: melepas capture milik thread ini.
                let _ = unsafe { ReleaseCapture() };
                WindowEvent::PointerReleased { position: self.position(lparam), button }
            }
            WM_MOUSEWHEEL => {
                // Posisi roda dalam koordinat layar; ubah ke koordinat jendela.
                let mut at = POINT {
                    x: (lparam.0 & 0xFFFF) as i16 as i32,
                    y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
                };
                // SAFETY: hwnd valid, titik milik kita.
                let _ = unsafe { ScreenToClient(hwnd, &mut at) };
                let scale = self.scale.get();
                let notches = ((wparam.0 >> 16) & 0xFFFF) as i16 as f32 / WHEEL_DELTA as f32;
                WindowEvent::PointerScrolled {
                    position: LogicalPosition::new(at.x as f32 / scale, at.y as f32 / scale),
                    delta_x: 0.0,
                    delta_y: notches * WHEEL_STEP,
                }
            }
            WM_MOUSEACTIVATE => return Some(LRESULT(MA_NOACTIVATE as isize)),
            tray::WM_TRAY => {
                match (lparam.0 & 0xFFFF) as u32 {
                    tray::NIN_SELECT | tray::NIN_KEYSELECT => emit(Event::TrayClick),
                    WM_CONTEXTMENU => emit(Event::TrayMenu),
                    _ => {}
                }
                return Some(LRESULT(0));
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    emit(Event::Deactivated);
                }
                return Some(LRESULT(0));
            }
            WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
                emit(Event::Escape);
                return Some(LRESULT(0));
            }
            // Tombol non-teks (panah, Backspace, Enter, ...) dan pintasan Ctrl+huruf.
            // Huruf biasa datang lewat WM_CHAR.
            WM_KEYDOWN | WM_KEYUP => {
                let vk = wparam.0 as u16;
                // SAFETY: membaca status tombol thread ini.
                let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                let text: slint::SharedString = match special_key(vk) {
                    Some(key) => key.into(),
                    // Ctrl+C/V/X/A/Z: WM_CHAR untuk ini berupa karakter kontrol, jadi kirim hurufnya.
                    None if ctrl && (b'A' as u16..=b'Z' as u16).contains(&vk) => {
                        char::from(vk as u8).to_ascii_lowercase().to_string().into()
                    }
                    None => return None,
                };
                if msg == WM_KEYDOWN {
                    WindowEvent::KeyPressed { text }
                } else {
                    WindowEvent::KeyReleased { text }
                }
            }
            // Tombol pengubah yang masih ditekan saat fokus pindah tidak akan
            // pernah dilepas di sini; lepaskan supaya tidak "macet".
            WM_KILLFOCUS => {
                use slint::platform::Key;
                for key in [Key::Control, Key::Shift] {
                    self.window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
                }
                return Some(LRESULT(0));
            }
            WM_CHAR => {
                let unit = wparam.0 as u16;
                // Karakter di luar BMP datang sebagai dua surrogate berurutan.
                let text = if (0xD800..0xDC00).contains(&unit) {
                    self.high_surrogate.set(Some(unit));
                    return Some(LRESULT(0));
                } else if let Some(high) = self.high_surrogate.take() {
                    String::from_utf16(&[high, unit]).ok()?
                } else {
                    char::from_u32(unit as u32).filter(|c| !c.is_control())?.to_string()
                };
                self.window.dispatch_event(WindowEvent::KeyPressed { text: text.as_str().into() });
                WindowEvent::KeyReleased { text: text.into() }
            }
            WM_POWERBROADCAST => {
                if wparam.0 == PBT_APMPOWERSTATUSCHANGE as usize {
                    emit(Event::PowerChanged);
                } else if wparam.0 == PBT_POWERSETTINGCHANGE as usize {
                    self.power_setting(lparam);
                }
                return Some(LRESULT(1));
            }
            WM_CLIPBOARDUPDATE => {
                emit(Event::ClipboardChanged);
                return Some(LRESULT(0));
            }
            WM_CAPS => {
                // SAFETY: membaca status tombol; dipanggil setelah Caps Lock dilepas.
                let on = unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 == 1;
                emit(Event::CapsLock(on));
                return Some(LRESULT(0));
            }
            m if m == self.taskbar_created => {
                if let Some(old) = self.tray_icon.take() {
                    tray::remove(hwnd, old);
                }
                self.tray_icon.set(tray::add(hwnd));
                return Some(LRESULT(0));
            }
            WM_DPICHANGED => {
                self.set_scale((wparam.0 & 0xFFFF) as f32 / 96.0);
                return Some(LRESULT(0));
            }
            WM_DISPLAYCHANGE => {
                self.frame_time.set(refresh_interval());
                self.relayout();
                emit(Event::DisplayChanged);
                return Some(LRESULT(0));
            }
            _ => return None,
        };
        self.window.dispatch_event(event);
        Some(LRESULT(0))
    }
}

impl WindowAdapter for LayeredWindow {
    fn window(&self) -> &slint::Window {
        &self.window
    }

    fn size(&self) -> PhysicalSize {
        self.physical_size()
    }

    fn set_size(&self, size: slint::WindowSize) {
        self.logical.set(size.to_logical(self.scale.get()));
        self.relayout();
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn request_redraw(&self) {
        self.dirty.set(true);
    }

    fn set_visible(&self, visible: bool) -> Result<(), PlatformError> {
        let hwnd = match self.hwnd.get() {
            Some(h) => h,
            None if visible => {
                let h = self.create()?;
                self.hwnd.set(Some(h));
                CURRENT.with(|c| *c.borrow_mut() = self.me.clone());
                self.window.dispatch_event(WindowEvent::ScaleFactorChanged {
                    scale_factor: self.scale.get(),
                });
                self.relayout();
                self.draw();
                h
            }
            None => return Ok(()),
        };
        let cmd = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
        // SAFETY: hwnd valid.
        unsafe {
            let _ = ShowWindow(hwnd, cmd);
        }
        Ok(())
    }

    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        let preferred = properties.layout_constraints().preferred;
        if preferred != self.logical.get() {
            self.logical.set(preferred);
            self.relayout();
        }
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        self.watchers.borrow_mut().take();
        if let Some(hwnd) = self.hwnd.take() {
            if let Some(icon) = self.tray_icon.take() {
                tray::remove(hwnd, icon);
            }
            // SAFETY: jendela milik kita, dihancurkan sekali.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let window = CURRENT.with(|c| c.borrow().upgrade());
    if let Some(result) = window.and_then(|w| w.handle(hwnd, msg, wparam, lparam)) {
        return result;
    }
    // SAFETY: penerusan standar ke prosedur bawaan.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

// ---------------------------------------------------------------------------
// Pemantau sistem: kecerahan, hemat baterai, Caps Lock

/// Dikirim kait keyboard saat Caps Lock dilepas.
const WM_CAPS: u32 = WM_APP + 3;
const GUID_BRIGHTNESS: windows::core::GUID = windows::core::GUID::from_u128(0x8ffee2c6_2d01_46be_adb9_398addc5b4ff);
const GUID_SAVER: windows::core::GUID = windows::core::GUID::from_u128(0xe00958c0_c213_4ace_ac77_fecced2eeea5);

/// Langganan notifikasi daya, clipboard, seret & lepas, dan kait keyboard.
/// Dilepas saat di-drop.
struct Watchers {
    hwnd: HWND,
    power: Vec<windows::Win32::System::Power::HPOWERNOTIFY>,
    hook: Option<HHOOK>,
    clipboard: bool,
    _drop_target: Option<dragdrop::Registration>,
}

impl Watchers {
    fn start(hwnd: HWND) -> Self {
        use windows::Win32::System::Power::RegisterPowerSettingNotification;
        // SAFETY: hwnd valid; handle disimpan dan dilepas di Drop.
        let power = [GUID_BRIGHTNESS, GUID_SAVER]
            .iter()
            .filter_map(|g| unsafe { RegisterPowerSettingNotification(HANDLE(hwnd.0), g, DEVICE_NOTIFY_WINDOW_HANDLE) }.ok())
            .collect();
        // Kait hanya melihat tombol Caps Lock; tombol lain langsung diteruskan.
        // SAFETY: prosedur kait statis; dilepas di Drop.
        let hook = unsafe {
            GetModuleHandleW(None)
                .ok()
                .and_then(|m| SetWindowsHookExW(WH_KEYBOARD_LL, Some(caps_hook), Some(m.into()), 0).ok())
        };
        use windows::Win32::System::DataExchange::AddClipboardFormatListener;
        // SAFETY: hwnd valid; dilepas di Drop.
        let clipboard = unsafe { AddClipboardFormatListener(hwnd) }.is_ok();
        let drop_target = dragdrop::register(hwnd, |on| emit(Event::DragHover(on)), |paths| emit(Event::FilesDropped(paths)));
        Self { hwnd, power, hook, clipboard, _drop_target: drop_target }
    }
}

impl Drop for Watchers {
    fn drop(&mut self) {
        use windows::Win32::System::Power::UnregisterPowerSettingNotification;
        // SAFETY: handle dari `start`, dilepas sekali.
        unsafe {
            for h in self.power.drain(..) {
                let _ = UnregisterPowerSettingNotification(h);
            }
            if let Some(hook) = self.hook.take() {
                let _ = UnhookWindowsHookEx(hook);
            }
            if self.clipboard {
                let _ = windows::Win32::System::DataExchange::RemoveClipboardFormatListener(self.hwnd);
            }
        }
    }
}

extern "system" fn caps_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
    if code >= 0 && up {
        // SAFETY: untuk WH_KEYBOARD_LL, lparam menunjuk KBDLLHOOKSTRUCT yang valid.
        let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if key.vkCode == VK_CAPITAL.0 as u32 {
            if let Some(hwnd) = current_hwnd() {
                // SAFETY: hanya mengirim pesan ke jendela kita sendiri.
                let _ = unsafe { PostMessageW(Some(hwnd), WM_CAPS, WPARAM(0), LPARAM(0)) };
            }
        }
    }
    // SAFETY: meneruskan ke kait berikutnya sesuai aturan Windows.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

impl LayeredWindow {
    fn power_setting(&self, lparam: LPARAM) {
        use windows::Win32::System::Power::POWERBROADCAST_SETTING;
        // SAFETY: untuk PBT_POWERSETTINGCHANGE, lparam menunjuk POWERBROADCAST_SETTING
        // yang datanya paling sedikit DataLength byte.
        let (guid, value) = unsafe {
            let s = &*(lparam.0 as *const POWERBROADCAST_SETTING);
            if s.DataLength < 4 {
                return;
            }
            (s.PowerSetting, std::ptr::read_unaligned(s.Data.as_ptr().cast::<u32>()))
        };
        let bit = if guid == GUID_BRIGHTNESS { 1 } else if guid == GUID_SAVER { 2 } else { return };
        let seen = self.power_seen.get();
        self.power_seen.set(seen | bit);
        if seen & bit == 0 {
            return; // nilai awal saat mendaftar, bukan perubahan
        }
        emit(if bit == 1 { Event::Brightness(value.min(100)) } else { Event::BatterySaver(value != 0) });
    }
}

/// Tombol khusus Windows → tombol Slint. `None` untuk tombol yang tidak dipakai.
fn special_key(vk: u16) -> Option<slint::platform::Key> {
    use slint::platform::Key;
    Some(match VIRTUAL_KEY(vk) {
        VK_BACK => Key::Backspace,
        VK_DELETE => Key::Delete,
        VK_RETURN => Key::Return,
        VK_TAB => Key::Tab,
        VK_LEFT => Key::LeftArrow,
        VK_RIGHT => Key::RightArrow,
        VK_UP => Key::UpArrow,
        VK_DOWN => Key::DownArrow,
        VK_HOME => Key::Home,
        VK_END => Key::End,
        VK_SHIFT => Key::Shift,
        VK_CONTROL => Key::Control,
        _ => return None,
    })
}

/// Durasi satu frame monitor utama, supaya animasi mengikuti refresh rate.
fn refresh_interval() -> Duration {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY: struct milik kita dengan dmSize yang benar.
    let ok = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool();
    let hz = if ok && mode.dmDisplayFrequency > 1 { mode.dmDisplayFrequency } else { 60 };
    Duration::from_micros(1_000_000 / hz as u64)
}
