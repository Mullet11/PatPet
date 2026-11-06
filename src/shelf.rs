//! Shelf: tempat menaruh file sementara. Yang disimpan hanya lokasi file,
//! di `shelf.json`, jadi isinya tetap ada setelah restart sampai file
//! dipindahkan keluar atau dikeluarkan dari Shelf.

use std::path::{Path, PathBuf};

pub struct Shelf {
    file: PathBuf,
    items: Vec<PathBuf>,
}

impl Shelf {
    /// Muat dari `file`; item yang file aslinya sudah hilang dibuang.
    pub fn load(file: PathBuf) -> Self {
        let items: Vec<PathBuf> = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();
        Self { file, items }
    }

    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }

    /// Tambahkan di depan. Mengembalikan `false` bila sudah ada.
    pub fn add(&mut self, path: PathBuf) -> bool {
        if self.items.iter().any(|p| same(p, &path)) {
            return false;
        }
        self.items.insert(0, path);
        self.save();
        true
    }

    /// Keluarkan dari Shelf (filenya tidak dihapus).
    pub fn remove(&mut self, path: &Path) {
        let before = self.items.len();
        self.items.retain(|p| !same(p, path));
        if self.items.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        let list: Vec<String> = self.items.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        if let Ok(text) = serde_json::to_string_pretty(&list) {
            let tmp = self.file.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &self.file);
            }
        }
    }
}

/// Lokasi Windows tidak membedakan huruf besar/kecil.
fn same(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// Nama tampilan: nama file tanpa folder.
pub fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("patpet-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn adds_removes_and_survives_restart() {
        let dir = temp_dir("shelf");
        let a = dir.join("laporan.pdf");
        let b = dir.join("foto.png");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();

        let mut shelf = Shelf::load(dir.join("shelf.json"));
        assert!(shelf.add(a.clone()));
        assert!(shelf.add(b.clone()));
        assert!(!shelf.add(PathBuf::from(a.to_string_lossy().to_uppercase())), "tidak dobel");
        assert_eq!(shelf.items(), &[b.clone(), a.clone()]);

        let again = Shelf::load(dir.join("shelf.json"));
        assert_eq!(again.items(), &[b.clone(), a.clone()]);

        // File yang sudah dipindah keluar tidak dimuat lagi.
        std::fs::remove_file(&b).unwrap();
        let mut after = Shelf::load(dir.join("shelf.json"));
        assert_eq!(after.items(), &[a.clone()]);
        after.remove(&a);
        assert!(after.items().is_empty());
        assert!(a.exists(), "keluarkan dari Shelf tidak menghapus file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_show_only_the_file() {
        assert_eq!(display_name(Path::new(r"C:\Users\x\Downloads\tugas akhir.docx")), "tugas akhir.docx");
    }
}
