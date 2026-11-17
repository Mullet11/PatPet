//! Installing without an installer. The first time `patpet.exe` is opened from somewhere else
//! (usually Downloads), it copies itself to `%LOCALAPPDATA%\Programs\PatPet`, starts that copy
//! and quits; the copy starts with Windows, has a Start-menu shortcut and an entry in
//! Settings > Apps > Installed apps, whose Uninstall runs `patpet.exe --uninstall`.
//! No admin rights are needed: everything is per user. Developer builds (cargo's `target`
//! folder) and the Store version (see package.rs) run where they are.

use std::path::{Path, PathBuf};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::util::wide;

const UNINSTALL_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PatPet");

/// Where PatPet installs itself.
pub fn install_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Programs").join("PatPet"))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    norm(a) == norm(b)
}

/// A build in cargo's `target\release` or `target\debug` folder.
fn dev_build(exe: &Path) -> bool {
    let parts: Vec<String> = exe.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
    parts.windows(2).any(|w| w[0] == "target" && (w[1] == "release" || w[1] == "debug"))
}

/// Whether the exe at `exe` should install itself into `dir`.
pub fn should_install(exe: &Path, dir: &Path) -> bool {
    match exe.parent() {
        Some(parent) => !same_dir(parent, dir) && !dev_build(exe),
        None => false,
    }
}

/// Running from the install folder.
pub fn is_installed() -> bool {
    match (std::env::current_exe(), install_dir()) {
        (Ok(exe), Some(dir)) => exe.parent().map(|p| same_dir(p, &dir)).unwrap_or(false),
        _ => false,
    }
}

/// The version recorded by the installed copy (empty when there is none).
fn installed_version() -> String {
    unsafe {
        let mut buf = [0u16; 64];
        let mut len = (buf.len() * 2) as u32;
        let ok = RegGetValueW(HKEY_CURRENT_USER, UNINSTALL_KEY, w!("DisplayVersion"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len));
        if ok.is_err() {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)])
    }
}

/// Opened from outside the install folder: put this exe there (unless a newer one is already
/// installed), start the installed copy and return true, so this one quits.
pub fn install_and_relaunch() -> bool {
    if crate::package::is_packaged() || std::env::var_os("PATPET_DEMO").is_some() {
        return false;
    }
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), install_dir()) else { return false };
    if !should_install(&exe, &dir) {
        return false;
    }
    let target = dir.join("patpet.exe");
    let keep = target.exists() && crate::update::newer(&installed_version(), crate::update::CURRENT);
    if !keep && (std::fs::create_dir_all(&dir).is_err() || std::fs::copy(&exe, &target).is_err()) {
        // could not install (disk full, blocked folder): just run from here
        return false;
    }
    std::process::Command::new(&target).arg("--installed").spawn().is_ok()
}

fn set_str(key: HKEY, name: PCWSTR, v: &str) {
    let w = wide(v);
    unsafe {
        let _ = RegSetValueExW(key, name, 0, REG_SZ, Some(std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2)));
    }
}

fn set_dword(key: HKEY, name: PCWSTR, v: u32) {
    unsafe {
        let _ = RegSetValueExW(key, name, 0, REG_DWORD, Some(&v.to_le_bytes()));
    }
}

/// The entry in Settings > Apps > Installed apps (kept up to date after updates).
pub fn register_uninstall() {
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), install_dir()) else { return };
    let size_kb = std::fs::metadata(&exe).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
    let exe_s = exe.to_string_lossy().to_string();
    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, UNINSTALL_KEY, 0, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None).is_err() {
            return;
        }
        set_str(key, w!("DisplayName"), "PatPet");
        set_str(key, w!("DisplayVersion"), crate::update::CURRENT);
        set_str(key, w!("Publisher"), "PatPet");
        set_str(key, w!("DisplayIcon"), &format!("{exe_s},0"));
        set_str(key, w!("InstallLocation"), &dir.to_string_lossy());
        set_str(key, w!("UninstallString"), &format!("\"{exe_s}\" --uninstall"));
        set_str(key, w!("URLInfoAbout"), "https://github.com/Mullet11/PatPet");
        set_dword(key, w!("EstimatedSize"), size_kb);
        set_dword(key, w!("NoModify"), 1);
        set_dword(key, w!("NoRepair"), 1);
        let _ = RegCloseKey(key);
    }
}

fn shortcut_path() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("APPDATA")?)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("PatPet.lnk"),
    )
}

/// (Re)create the Start-menu shortcut (the Store version has its own tile).
pub fn ensure() {
    if crate::package::is_packaged() {
        return;
    }
    let (Ok(exe), Some(lnk)) = (std::env::current_exe(), shortcut_path()) else { return };
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return };
        let exe_w = wide(&exe.to_string_lossy());
        let dir_w = wide(&exe.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
        let _ = link.SetPath(PCWSTR(exe_w.as_ptr()));
        let _ = link.SetWorkingDirectory(PCWSTR(dir_w.as_ptr()));
        let _ = link.SetDescription(w!("PatPet - a Dynamic Island for Windows"));
        let _ = link.SetIconLocation(PCWSTR(exe_w.as_ptr()), 0);
        if let Ok(pf) = link.cast::<IPersistFile>() {
            let lnk_w = wide(&lnk.to_string_lossy());
            let _ = pf.Save(PCWSTR(lnk_w.as_ptr()), true);
        }
    }
    if is_installed() {
        register_uninstall();
    }
}

fn ask(text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let t = wide(text);
    unsafe { MessageBoxW(HWND::default(), PCWSTR(t.as_ptr()), w!("PatPet"), flags | MB_SETFOREGROUND) }
}

/// `patpet.exe --uninstall` (Settings > Apps > Installed apps > Uninstall).
pub fn uninstall() {
    if ask("Uninstall PatPet?\n\nIt will stop starting with Windows and be removed from this PC.", MB_YESNO | MB_ICONQUESTION) != IDYES {
        return;
    }
    // close the running copy and wait until its process is gone: it saves data.json on the
    // way out, and its exe stays locked until then
    unsafe {
        if let Ok(h) = FindWindowW(crate::CLASS, PCWSTR::null()) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let proc = OpenProcess(PROCESS_SYNCHRONIZE, false, pid);
            let _ = PostMessageW(h, crate::app::WM_APP_QUIT, WPARAM(0), LPARAM(0));
            if let Ok(p) = proc {
                WaitForSingleObject(p, 15_000);
                let _ = CloseHandle(p);
            }
        }
    }
    let keep = ask("Keep your tasks, your cat and your settings?\n\nChoose Yes to keep them for a later install, No to delete them too.", MB_YESNO | MB_ICONQUESTION) == IDYES;
    if !keep {
        if let Some(data) = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("PatPet")) {
            let _ = std::fs::remove_dir_all(data);
        }
    }
    crate::tray::set_autostart(false);
    if let Some(lnk) = shortcut_path() {
        let _ = std::fs::remove_file(lnk);
    }
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, UNINSTALL_KEY);
    }
    ask("PatPet has been removed. Thanks for looking after the cat!", MB_OK | MB_ICONINFORMATION);
    // this exe is still running from the folder: keep trying to remove it (for a minute)
    // until this process has quit, which it does right away
    if let Some(dir) = install_dir().filter(|d| d.exists()) {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let d = dir.display();
        let cmd = format!("for /l %i in (1,1,60) do @(ping 127.0.0.1 -n 2 >nul & rmdir /s /q \"{d}\" 2>nul & if not exist \"{d}\" exit /b)");
        let _ = std::process::Command::new("cmd").arg("/c").raw_arg(&cmd).current_dir(std::env::temp_dir()).creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        PathBuf::from(r"C:\Users\Ana\AppData\Local\Programs\PatPet")
    }

    #[test]
    fn a_download_installs_itself() {
        assert!(should_install(Path::new(r"C:\Users\Ana\Downloads\patpet.exe"), &dir()));
        assert!(should_install(Path::new(r"D:\Apps\patpet (1).exe"), &dir()));
    }

    #[test]
    fn the_installed_copy_stays_put() {
        assert!(!should_install(Path::new(r"C:\Users\Ana\AppData\Local\Programs\PatPet\patpet.exe"), &dir()));
        // whatever the letter case
        assert!(!should_install(Path::new(r"c:\users\ana\appdata\local\programs\patpet\PatPet.exe"), &dir()));
    }

    #[test]
    fn developer_builds_run_where_they_are() {
        assert!(!should_install(Path::new(r"E:\code\PatPet\target\release\patpet.exe"), &dir()));
        assert!(!should_install(Path::new(r"E:\code\PatPet\target\debug\patpet.exe"), &dir()));
        // a folder that is merely called "target" is not a build folder
        assert!(should_install(Path::new(r"E:\target\patpet.exe"), &dir()));
    }
}
