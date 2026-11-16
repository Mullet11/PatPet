//! Makes PatPet findable in Windows Search: keeps a Start-menu shortcut pointing at the
//! current executable (its icon comes from the exe itself).

use std::path::PathBuf;

use windows::core::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;

use crate::util::wide;

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

/// (Re)create the Start-menu shortcut.
pub fn ensure() {
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
}
