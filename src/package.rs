//! The Microsoft Store version: PatPet packaged as MSIX (see packaging/). Windows installs,
//! signs and updates it, so the Store copy skips PatPet's own installing and updating, and it
//! starts with Windows through a startup task declared in the package instead of the Run key.

use windows::core::*;
use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// The startup task's id in packaging/AppxManifest.xml.
const STARTUP_TASK: &str = "PatPetStartup";

/// Running as the Store package (cached: it cannot change while running).
pub fn is_packaged() -> bool {
    static PACKAGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut len = 0u32;
        unsafe { GetCurrentPackageFullName(&mut len, PWSTR::null()) != APPMODEL_ERROR_NO_PACKAGE }
    })
}

fn task() -> Option<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK)).ok()?.get().ok()
}

/// The package starts with Windows.
pub fn startup_enabled() -> bool {
    task().and_then(|t| t.State().ok()).map(|s| s == StartupTaskState::Enabled || s == StartupTaskState::EnabledByPolicy).unwrap_or(false)
}

/// Turn starting with Windows on or off (Windows may refuse "on" when the user switched it
/// off in Settings > Apps > Startup; that choice is theirs).
pub fn set_startup(on: bool) {
    let Some(t) = task() else { return };
    if on {
        let _ = t.RequestEnableAsync().and_then(|op| op.get());
    } else {
        let _ = t.Disable();
    }
}
