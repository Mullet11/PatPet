use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Task {
    pub id: u64,
    pub text: String,
    pub due: Option<i64>,
    pub done: bool,
    /// "due now" was shown
    pub notified: bool,
    /// early reminders already shown (or skipped), one bit per entry of app::REMINDERS
    pub alerts: u8,
    /// ticking it off already earned the cat experience (only the first time counts)
    pub rewarded: bool,
}

impl Task {
    /// Tick it off or back on. Returns (now done, done for the first time): only the first
    /// time earns the cat experience, so unticking and ticking again earns nothing more.
    pub fn toggle(&mut self) -> (bool, bool) {
        self.done = !self.done;
        let first = self.done && !self.rewarded;
        self.rewarded |= self.done;
        (self.done, first)
    }
}

/// Switches from the tray menu.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// shrink to a thin line at the top of the screen unless the mouse is on it
    pub line_mode: bool,
    /// put new screenshots on the Shelf
    pub screenshots: bool,
    /// the cat suggests a short break after `break_after_min` minutes of work
    pub breaks: bool,
    pub break_after_min: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { line_mode: false, screenshots: true, breaks: true, break_after_min: 50 }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Data {
    /// the cat: its needs, level and room
    pub pet: crate::care::Care,
    pub tasks: Vec<Task>,
    pub shelf: Vec<String>,
    pub next_id: u64,
    pub settings: Settings,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    let mut p = PathBuf::from(base);
    p.push("PatPet");
    Some(p)
}

/// No data file yet: PatPet has never run for this user.
pub fn first_run() -> bool {
    path().map(|p| !p.join("data.json").exists()).unwrap_or(false)
}

pub fn load() -> Data {
    path()
        .and_then(|p| std::fs::read_to_string(p.join("data.json")).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(d: &Data) {
    let Some(dir) = path() else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(s) = serde_json::to_string_pretty(d) {
        let tmp = dir.join("data.json.tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("data.json"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_earns_experience_once() {
        let mut t = Task::default();
        assert_eq!(t.toggle(), (true, true));
        assert_eq!(t.toggle(), (false, false));
        assert_eq!(t.toggle(), (true, false));
    }

    #[test]
    fn screenshots_on_by_default() {
        assert!(Data::default().settings.screenshots);
        // a data file from before the setting existed
        let d: Data = serde_json::from_str("{}").unwrap();
        assert!(d.settings.screenshots);
        // the user's choice stays
        let d: Data = serde_json::from_str(r#"{"settings":{"screenshots":false}}"#).unwrap();
        assert!(!d.settings.screenshots);
    }
}
