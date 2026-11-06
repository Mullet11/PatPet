//! Seret & lepas file lewat OLE: menerima file dari Explorer ke island, dan
//! menyeret file dari Shelf ke folder mana pun. Pemindahan filenya sendiri
//! dikerjakan Explorer, sama seperti menyeret antar-folder.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use windows::core::{implement, Ref, HSTRING};
use windows::Win32::Foundation::{GlobalFree, HWND, POINTL};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{
    DragQueryFileW, IShellItem, SHCreateItemFromParsingName, SHDoDragDrop, BHID_DataObject,
    CFSTR_PREFERREDDROPEFFECT, HDROP,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

thread_local! {
    /// Sedang menyeret kartu Shelf keluar; island sendiri bukan tujuan.
    static DRAGGING_OUT: Cell<bool> = const { Cell::new(false) };
}

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

#[implement(IDropTarget)]
struct Target {
    /// `true` saat file masuk ke atas jendela, `false` saat keluar atau dilepas.
    on_hover: Box<dyn Fn(bool)>,
    on_drop: Box<dyn Fn(Vec<PathBuf>)>,
}

impl Target {
    fn has_files(data: Ref<'_, IDataObject>) -> bool {
        // SAFETY: format milik kita; hanya bertanya, tidak mengambil data.
        data.as_ref().is_some_and(|d| unsafe { d.QueryGetData(&hdrop_format()) }.is_ok())
    }
}

/// Efek "salin" hanya untuk kursor; Shelf menyimpan lokasi, file tidak disentuh.
fn set_effect(effect: *mut DROPEFFECT, value: DROPEFFECT) {
    if !effect.is_null() {
        // SAFETY: penunjuk dari OLE, valid selama panggilan.
        unsafe { *effect = value };
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        if !DRAGGING_OUT.get() && Target::has_files(data) {
            set_effect(effect, DROPEFFECT_COPY);
            (self.on_hover)(true);
        } else {
            set_effect(effect, DROPEFFECT_NONE);
        }
        Ok(())
    }

    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        // SAFETY: penunjuk dari OLE; nilai masuk adalah efek yang diizinkan sumber.
        let allowed = if effect.is_null() || DRAGGING_OUT.get() { DROPEFFECT_NONE } else { unsafe { *effect } };
        set_effect(effect, if allowed.0 & DROPEFFECT_COPY.0 != 0 { DROPEFFECT_COPY } else { DROPEFFECT_NONE });
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        (self.on_hover)(false);
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let paths = if DRAGGING_OUT.get() { Vec::new() } else { data.as_ref().map(dropped_paths).unwrap_or_default() };
        set_effect(effect, if paths.is_empty() { DROPEFFECT_NONE } else { DROPEFFECT_COPY });
        (self.on_hover)(false);
        if !paths.is_empty() {
            (self.on_drop)(paths);
        }
        Ok(())
    }
}

fn dropped_paths(data: &IDataObject) -> Vec<PathBuf> {
    // SAFETY: medium dilepas dengan ReleaseStgMedium; HDROP hanya dibaca selama medium hidup.
    unsafe {
        let Ok(mut medium) = data.GetData(&hdrop_format()) else { return Vec::new() };
        let hdrop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(hdrop, u32::MAX, None);
        let paths = (0..count)
            .filter_map(|i| {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buffer = vec![0u16; len + 1];
                (DragQueryFileW(hdrop, i, Some(&mut buffer)) > 0)
                    .then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
            })
            .collect();
        ReleaseStgMedium(&mut medium);
        paths
    }
}

/// Jendela terdaftar sebagai tujuan seret selama nilai ini hidup.
pub struct Registration(HWND);

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: pasangan RegisterDragDrop yang berhasil.
        let _ = unsafe { RevokeDragDrop(self.0) };
    }
}

/// Terima file yang diseret ke `hwnd`. Butuh OleInitialize di thread ini.
pub fn register(
    hwnd: HWND,
    on_hover: impl Fn(bool) + 'static,
    on_drop: impl Fn(Vec<PathBuf>) + 'static,
) -> Option<Registration> {
    let target: IDropTarget = Target { on_hover: Box::new(on_hover), on_drop: Box::new(on_drop) }.into();
    // SAFETY: hwnd milik thread ini; OLE menyimpan referensinya sendiri sampai RevokeDragDrop.
    unsafe { RegisterDragDrop(hwnd, &target) }.ok().map(|_| Registration(hwnd))
}

/// Mulai menyeret `path` keluar. Memblokir sampai dilepas; harus dipanggil
/// saat tombol mouse masih ditekan. `true` bila file dipindah/disalin.
pub fn drag_out(hwnd: HWND, path: &Path) -> bool {
    // SAFETY: objek COM dilepas otomatis; medium diserahkan ke data object (fRelease).
    let effect = unsafe {
        let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(path.as_os_str()), None) else {
            return false;
        };
        let Ok(data) = item.BindToHandler::<_, IDataObject>(None, &BHID_DataObject) else { return false };
        prefer_move(&data);
        DRAGGING_OUT.set(true);
        let effect = SHDoDragDrop(Some(hwnd), &data, None, DROPEFFECT_MOVE | DROPEFFECT_COPY);
        DRAGGING_OUT.set(false);
        effect.unwrap_or(DROPEFFECT_NONE)
    };
    effect != DROPEFFECT_NONE
}

/// Minta Explorer memindahkan (bukan menyalin) saat file dilepas di folder.
unsafe fn prefer_move(data: &IDataObject) {
    // SAFETY (pemanggil): data object valid. Memori global berpindah ke data object bila SetData berhasil.
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, 4) else { return };
        let target = GlobalLock(memory).cast::<u32>();
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return;
        }
        *target = DROPEFFECT_MOVE.0;
        let _ = GlobalUnlock(memory);
        let format = FORMATETC {
            cfFormat: RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) as u16,
            ..hdrop_format()
        };
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: memory },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        };
        if data.SetData(&format, &medium, true).is_err() {
            let _ = GlobalFree(Some(memory));
        }
    }
}
