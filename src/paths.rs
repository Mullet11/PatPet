//! Lokasi data aplikasi: %APPDATA%\PatPet (sama dengan versi C#).

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let dir = base.join("PatPet");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
