//! Ikon di system tray Windows dan menu klik kanannya.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, PostMessageW, WM_NULL, DestroyMenu, DestroyIcon, GetCursorPos, LoadImageW,
    SetForegroundWindow, TrackPopupMenu, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, MF_SEPARATOR,
    MF_STRING, SM_CXSMICON, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
};

/// Pesan yang dikirim shell ke jendela saat ikon tray diklik.
pub const WM_TRAY: u32 = WM_APP + 2;
/// Klik kiri / Enter pada ikon (NOTIFYICON_VERSION_4). Tidak diekspor crate `windows`.
pub const NIN_SELECT: u32 = 0x0400;
pub const NIN_KEYSELECT: u32 = 0x0401;

/// Satu baris menu. `None` = garis pemisah.
pub type MenuItem<'a> = Option<(u32, &'a str)>;

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

/// Mengembalikan handle ikon yang harus dilepas lewat [`remove`].
pub fn add(hwnd: HWND) -> Option<HICON> {
    // SAFETY: memuat resource ikon id 1 dari exe ini dengan ukuran sesuai DPI.
    let icon = unsafe {
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(hwnd));
        let module = GetModuleHandleW(None).ok()?;
        LoadImageW(Some(module.into()), PCWSTR(1 as _), IMAGE_ICON, size, size, LR_DEFAULTCOLOR).ok()?
    };
    let icon = HICON(icon.0);

    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    copy_into(&mut nid.szTip, "PatPet");
    nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    // SAFETY: struct lengkap milik kita; hwnd valid.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
    }
    Some(icon)
}

fn copy_into(dst: &mut [u16], text: &str) {
    // Sisakan satu tempat untuk penutup nol.
    let room = dst.len() - 1;
    for (d, s) in dst.iter_mut().take(room).zip(text.encode_utf16()) {
        *d = s;
    }
}

pub fn remove(hwnd: HWND, icon: HICON) {
    // SAFETY: menghapus ikon yang kita tambahkan dan melepas handle-nya sekali.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd));
        let _ = DestroyIcon(icon);
    }
}

/// Menampilkan menu di posisi kursor. Mengembalikan id item yang dipilih.
pub fn menu(hwnd: HWND, items: &[MenuItem]) -> Option<u32> {
    // SAFETY: menu dibuat dan dihancurkan di sini; string hidup selama panggilan.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for item in items {
            match item {
                Some((id, text)) => {
                    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
                    let _ = AppendMenuW(menu, MF_STRING, *id as usize, PCWSTR(wide.as_ptr()));
                }
                None => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
            }
        }
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        // Wajib supaya menu tertutup saat pengguna mengklik di luar menu.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        // Anjuran Microsoft: memastikan menu berikutnya tertutup dengan benar.
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        (chosen.0 != 0).then_some(chosen.0 as u32)
    }
}
