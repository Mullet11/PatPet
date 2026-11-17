#![windows_subsystem = "windows"]

mod app;
mod care;
mod clip;
mod events;
mod gfx;
mod install;
mod lyrics;
mod media;
mod pet;
mod sprites;
mod player;
mod shelf;
mod shots;
mod sound;
mod store;
mod system;
mod theme;
mod thumbs;
mod tray;
mod update;
mod util;
mod vsync;

use std::cell::RefCell;
use std::sync::atomic::Ordering;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use app::{App, Deferred};

const WM_MOUSELEAVE: u32 = 0x02A3;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static TASKBAR_CREATED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static ICON: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);

        let args: Vec<String> = std::env::args().collect();

        // `patpet.exe --quit`: ask the running copy to close (used by rebuild.bat and for testing)
        if args.len() >= 2 && args[1] == "--quit" {
            if let Ok(h) = FindWindowW(CLASS, PCWSTR::null()) {
                let _ = PostMessageW(h, app::WM_APP_QUIT, WPARAM(0), LPARAM(0));
            }
            return;
        }

        // Started by the updater: the old copy is still closing, so wait for it to let go.
        let after_update = args.len() >= 2 && args[1] == "--after-update";
        let mut _mutex = CreateMutexW(None, BOOL::from(true), w!("Local\\PatPet.SingleInstance"));
        if after_update {
            for _ in 0..50 {
                if GetLastError() != ERROR_ALREADY_EXISTS {
                    break;
                }
                if let Ok(h) = &_mutex {
                    let _ = CloseHandle(*h);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                _mutex = CreateMutexW(None, BOOL::from(true), w!("Local\\PatPet.SingleInstance"));
            }
        }
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        // Already running (e.g. launched again from Windows Search): bring it back and open it.
        if already {
            if let Ok(h) = FindWindowW(CLASS, PCWSTR::null()) {
                // we were launched by the user, so we may hand the foreground to the running copy
                let mut pid = 0u32;
                GetWindowThreadProcessId(h, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                let _ = PostMessageW(h, app::WM_APP_SHOW, WPARAM(0), LPARAM(0));
            }
            return;
        }
        update::cleanup();

        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: CLASS,
            hCursor: LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        // the main window: the primary display's island (the other displays get theirs in App)
        let Some(hwnd) = island_window() else {
            return;
        };
        events::HWND_MAIN.store(hwnd.0 as isize, Ordering::Relaxed);
        TASKBAR_CREATED.with(|c| c.set(RegisterWindowMessageW(w!("TaskbarCreated"))));

        let app = match App::new(hwnd) {
            Ok(a) => a,
            Err(_) => return,
        };
        APP.with(|a| *a.borrow_mut() = Some(app));
        vsync::start(hwnd);

        shelf::register(hwnd);
        install::ensure();
        // ICON 1 from assets/patpet.rc, at the small-icon size the tray uses
        let sz = GetSystemMetrics(SM_CXSMICON).max(16);
        let icon = LoadImageW(hinst, PCWSTR(1 as *const u16), IMAGE_ICON, sz, sz, LR_DEFAULTCOLOR)
            .map(|h| HICON(h.0))
            .unwrap_or_default();
        ICON.with(|c| c.set(icon.0 as isize));
        tray::add(hwnd, icon);

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().as_mut() {
                app.start();
                if after_update {
                    app.updated();
                }
            }
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // Data was saved in WM_DESTROY. A normal exit can deadlock while COM/WinRT DLLs
        // detach with background threads still alive, leaving a windowless process that
        // blocks the next launch, so end the process directly.
        app::dbg_log("quit: loop ended");
        quit_now();
    }
}

const CLASS: PCWSTR = w!("PatPetIsland");

/// A new (hidden) island window; one per display.
pub fn island_window() -> Option<HWND> {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("PatPet"),
            WS_POPUP,
            0,
            0,
            10,
            10,
            HWND::default(),
            HMENU::default(),
            hinst,
            None,
        )
        .ok()
    }
}

pub fn quit_now() -> ! {
    unsafe {
        let _ = windows::Win32::System::Threading::TerminateProcess(windows::Win32::System::Threading::GetCurrentProcess(), 0);
    }
    std::process::exit(0)
}

fn main_hwnd() -> HWND {
    HWND(events::HWND_MAIN.load(Ordering::Relaxed) as *mut _)
}

fn with_app<T>(f: impl FnOnce(&mut App) -> T) -> Option<T> {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f),
        Err(_) => None,
    })
}

fn run_deferred(hwnd: HWND, d: Deferred) {
    match d {
        Deferred::DragOut(paths) => shelf::drag_out(hwnd, &paths),
        Deferred::Menu => {
            let Some(st) = with_app(|a| a.menu_state()) else { return };
            let cmd = tray::menu(main_hwnd(), &st);
            with_app(|a| a.menu_closed());
            if cmd == tray::CMD_QUIT {
                // plays the closing animation, then closes the window
                with_app(|a| a.begin_quit());
            } else if cmd != 0 {
                with_app(|a| a.tray_cmd(cmd));
            }
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Every island window gets the broadcasts (power, display, settings, the taskbar coming
    // back); the main one answers them for all. Pointer and keyboard go to the island they hit.
    let main = hwnd == main_hwnd();
    if msg == TASKBAR_CREATED.with(|c| c.get()) && msg != 0 {
        if main {
            tray::add(hwnd, HICON(ICON.with(|c| c.get()) as *mut _));
        }
        return LRESULT(0);
    }
    if msg == vsync::WM_APP_FRAME {
        // let the pacing thread queue the next refresh while this one renders
        vsync::frame_done();
    }
    let handled: Option<(Option<LRESULT>, Option<Deferred>)> = with_app(|app| {
        let done = Some(LRESULT(0));
        let r = match msg {
            WM_TIMER if main => {
                match wp.0 {
                    1 => {
                        app.frame();
                    }
                    _ => app.tick(),
                }
                done
            }
            vsync::WM_APP_FRAME if main => {
                let _ = app.frame();
                done
            }
            // a second launch or `--quit` may find any of the windows
            app::WM_APP_SHOW => {
                app.show_and_open();
                done
            }
            app::WM_APP_QUIT => {
                app.begin_quit();
                done
            }
            events::WM_APP_EVENT if main => {
                app.on_events();
                done
            }
            tray::WM_APP_TRAY if main => {
                let ev = (lp.0 & 0xFFFF) as u32;
                if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                    app.deferred = Some(Deferred::Menu);
                } else if ev == WM_LBUTTONUP {
                    app.show_and_open();
                }
                done
            }
            WM_CLIPBOARDUPDATE if main => {
                app.on_clipboard();
                done
            }
            WM_MOUSEMOVE => app.on(hwnd, |a| a.on_mouse_move(lp)).and(done),
            WM_MOUSELEAVE => app.on(hwnd, |a| a.on_mouse_leave()).and(done),
            WM_LBUTTONDOWN => app.on(hwnd, |a| a.on_ldown(lp)).and(done),
            WM_LBUTTONUP => app.on(hwnd, |a| a.on_lup(lp)).and(done),
            WM_RBUTTONUP => app.on(hwnd, |a| a.on_rup()).and(done),
            WM_MOUSEWHEEL => app.on(hwnd, |a| a.on_wheel(wp, lp)).and(done),
            WM_ACTIVATE => {
                app.on(hwnd, |a| a.on_activate((wp.0 & 0xFFFF) as u32 != WA_INACTIVE));
                None
            }
            WM_KEYDOWN => app.on(hwnd, |a| a.on_key(wp.0 as u32)).filter(|k| *k).and(done),
            WM_CHAR => app.on(hwnd, |a| a.on_char(wp.0 as u32)).and(done),
            WM_POWERBROADCAST if main => {
                app.on_power(wp, lp);
                Some(LRESULT(1))
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE if main => {
                app.on_display_change();
                None
            }
            WM_DESTROY if main => {
                app::dbg_log("quit: WM_DESTROY");
                clip::stop(hwnd);
                app.save_now();
                app::dbg_log("quit: saved");
                done
            }
            _ => None,
        };
        (r, app.deferred.take())
    });

    if msg == WM_DESTROY {
        // an island of an unplugged display just goes; the main one ends PatPet
        if main {
            tray::remove(hwnd);
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }
    if let Some((r, d)) = handled {
        if let Some(d) = d {
            run_deferred(hwnd, d);
        }
        if let Some(r) = r {
            return r;
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
