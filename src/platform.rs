//! Platform Slint di atas Win32.
//!
//! Satu jendela layered (alpha per piksel). Renderer software Slint menggambar
//! langsung ke memori DIB section, lalu `UpdateLayeredWindow` menampilkannya.
//! Tanpa OpenGL/GPU context, jadi RAM tetap kecil. Piksel yang transparan
//! penuh otomatis meneruskan klik ke jendela di belakangnya.

use std::cell::{Cell, RefCell};
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
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW,
    MonitorFromWindow, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
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
    GetLastInputInfo, ReleaseCapture, SetCapture, TrackMouseEvent, LASTINPUTINFO, TME_LEAVE,
    TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Pesan untuk membangunkan loop saat ada kerja dari thread lain.
const WM_WAKE: u32 = WM_APP + 1;
/// Tidak diekspor modul WindowsAndMessaging di crate `windows`.
const WM_MOUSELEAVE: u32 = 0x02A3;

pub fn install() -> Result<(), PlatformError> {
    // SAFETY: dipanggil sekali sebelum jendela apa pun dibuat. Gagal berarti
    // sudah diatur lewat manifest; aman diabaikan.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
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
    frame_time: Cell<Duration>,
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
            frame_time: Cell::new(Duration::from_millis(16)),
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
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
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

        // Tengah atas monitor tempat jendela berada.
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        // SAFETY: hwnd valid, info milik kita.
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
            let _ = GetMonitorInfoW(monitor, &mut info);
        }
        let RECT { left, top, right, .. } = info.rcMonitor;
        let x = left + (right - left - size.width as i32) / 2;
        // SAFETY: hwnd valid.
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
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
            WM_MOUSEACTIVATE => return Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_DPICHANGED => {
                self.set_scale((wparam.0 & 0xFFFF) as f32 / 96.0);
                return Some(LRESULT(0));
            }
            WM_DISPLAYCHANGE => {
                self.frame_time.set(refresh_interval());
                self.relayout();
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
        if let Some(hwnd) = self.hwnd.take() {
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

/// Durasi satu frame monitor utama, supaya animasi mengikuti refresh rate.
fn refresh_interval() -> Duration {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY: struct milik kita dengan dmSize yang benar.
    let ok = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool();
    let hz = if ok && mode.dmDisplayFrequency > 1 { mode.dmDisplayFrequency } else { 60 };
    Duration::from_micros(1_000_000 / hz as u64)
}
