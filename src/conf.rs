//! Format file `patpet.conf`: bagian `[nama]`, baris `kunci = nilai`, dan
//! komentar setelah `#`. Saat PatPet mengubah nilai, hanya bagian nilai pada
//! baris itu yang diganti; komentar dan susunan file pengguna tetap utuh.

pub struct Conf {
    lines: Vec<String>,
}

/// Satu nilai beserta nomor barisnya (mulai dari 1), untuk pesan kesalahan.
pub struct Entry<'a> {
    pub line: usize,
    pub value: &'a str,
}

impl Conf {
    pub fn parse(text: &str) -> Self {
        Self { lines: text.lines().map(str::to_string).collect() }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<Entry<'_>> {
        let i = self.find(section, key)?;
        let (_, value, _) = split(&self.lines[i])?;
        Some(Entry { line: i + 1, value: value.trim() })
    }

    /// Ganti nilai `kunci` di `[bagian]`. Bila belum ada, tambahkan di akhir bagian
    /// (atau buat bagiannya di akhir file).
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some(i) = self.find(section, key) {
            if let Some((head, old, comment)) = split(&self.lines[i]) {
                let mut line = format!("{head} {value}");
                if !comment.is_empty() {
                    // Jaga komentar tetap di kolom yang sama bila muat.
                    let column = head.len() + old.len();
                    let pad = column.saturating_sub(line.len()).max(1);
                    line = format!("{line}{}{comment}", " ".repeat(pad));
                }
                self.lines[i] = line;
            }
            return;
        }
        let entry = format!("{key} = {value}");
        match self.section_end(section) {
            Some(end) => self.lines.insert(end, entry),
            None => {
                self.lines.push(String::new());
                self.lines.push(format!("[{section}]"));
                self.lines.push(entry);
            }
        }
    }

    pub fn text(&self) -> String {
        let mut s = self.lines.join("\r\n");
        s.push_str("\r\n");
        s
    }

    fn find(&self, section: &str, key: &str) -> Option<usize> {
        let mut current = "";
        for (i, line) in self.lines.iter().enumerate() {
            let t = line.trim();
            if let Some(name) = t.strip_prefix('[').and_then(|r| r.split(']').next()) {
                current = name.trim();
            } else if current.eq_ignore_ascii_case(section) {
                if let Some((head, _, _)) = split(line) {
                    if head.trim_end_matches('=').trim().eq_ignore_ascii_case(key) {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    /// Indeks tepat setelah baris berisi terakhir di `[bagian]`.
    fn section_end(&self, section: &str) -> Option<usize> {
        let start = self.lines.iter().position(|l| {
            l.trim().strip_prefix('[').and_then(|r| r.split(']').next()).is_some_and(|n| n.trim().eq_ignore_ascii_case(section))
        })?;
        let mut end = start + 1;
        for (i, line) in self.lines.iter().enumerate().skip(start + 1) {
            let t = line.trim();
            if t.starts_with('[') {
                break;
            }
            if !t.is_empty() && !t.starts_with('#') {
                end = i + 1;
            }
        }
        Some(end)
    }
}

/// Pecah `kunci = nilai   # komentar` menjadi ("kunci =", " nilai   ", "# komentar").
/// Komentar dimulai dari `#` yang didahului spasi dan tidak langsung diikuti
/// angka heksadesimal, jadi warna seperti `#FF8800` tidak dianggap komentar.
fn split(line: &str) -> Option<(&str, &str, &str)> {
    let t = line.trim_start();
    if t.is_empty() || t.starts_with('#') || t.starts_with('[') {
        return None;
    }
    let eq = line.find('=')?;
    let (head, rest) = line.split_at(eq + 1);
    let bytes = rest.as_bytes();
    let comment = (1..bytes.len()).find(|&i| {
        bytes[i] == b'#'
            && bytes[i - 1].is_ascii_whitespace()
            && !bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
    });
    Some(match comment {
        Some(c) => (head, &rest[..c], &rest[c..]),
        None => (head, rest, ""),
    })
}

// ------------------------------------------------------------ nilai umum

pub fn parse_bool(v: &str) -> Option<bool> {
    match v.to_lowercase().as_str() {
        "ya" | "y" | "true" | "on" | "nyala" | "1" => Some(true),
        "tidak" | "t" | "false" | "off" | "mati" | "0" => Some(false),
        _ => None,
    }
}

pub fn bool_text(b: bool) -> &'static str {
    if b { "ya" } else { "tidak" }
}

/// `#RRGGBB` atau `#RRGGBBAA` → (r, g, b, a).
pub fn parse_hex(v: &str) -> Option<(u8, u8, u8, u8)> {
    let hex = v.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let a = if hex.len() == 8 { byte(6)? } else { 255 };
    Some((byte(0)?, byte(2)?, byte(4)?, a))
}

/// "00:25:00" → detik.
pub fn parse_hms(v: &str) -> Option<i32> {
    let parts: Vec<i32> = v.split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let [h, m, s] = parts[..] else { return None };
    ((0..24).contains(&h) && (0..60).contains(&m) && (0..60).contains(&s)).then_some(h * 3600 + m * 60 + s)
}

pub fn hms_text(secs: i32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# judul\n\n[tampilan]\nmode = bar          # bar atau ikon\nukuran = 2\n\n[warna]\nlatar = #000000     # warna dasar\nkosong =            # ikuti aksen\n";

    #[test]
    fn reads_values_and_ignores_comments() {
        let c = Conf::parse(SAMPLE);
        assert_eq!(c.get("tampilan", "mode").unwrap().value, "bar");
        assert_eq!(c.get("TAMPILAN", "Mode").unwrap().line, 4);
        assert_eq!(c.get("warna", "latar").unwrap().value, "#000000");
        assert_eq!(c.get("warna", "kosong").unwrap().value, "");
        assert!(c.get("tampilan", "latar").is_none());
    }

    #[test]
    fn set_keeps_comments_and_layout() {
        let mut c = Conf::parse(SAMPLE);
        c.set("tampilan", "mode", "ikon");
        assert!(c.text().contains("mode = ikon         # bar atau ikon"));
        c.set("warna", "latar", "#123456");
        assert!(c.text().contains("latar = #123456     # warna dasar"));
        assert!(c.text().starts_with("# judul"));
    }

    #[test]
    fn set_adds_missing_keys_and_sections() {
        let mut c = Conf::parse(SAMPLE);
        c.set("tampilan", "posisi", "10, 20");
        c.set("timer", "durasi", "00:05:00");
        assert_eq!(c.get("tampilan", "posisi").unwrap().value, "10, 20");
        assert_eq!(c.get("timer", "durasi").unwrap().value, "00:05:00");
        let text = c.text();
        assert!(text.find("posisi").unwrap() < text.find("[warna]").unwrap());
    }

    #[test]
    fn value_parsers() {
        assert_eq!(parse_bool("Ya"), Some(true));
        assert_eq!(parse_bool("tidak"), Some(false));
        assert_eq!(parse_bool("mungkin"), None);
        assert_eq!(parse_hex("#FF8800"), Some((255, 136, 0, 255)));
        assert_eq!(parse_hex("#00000080"), Some((0, 0, 0, 128)));
        assert_eq!(parse_hex("FF8800"), None);
        assert_eq!(parse_hex("#GG0000"), None);
        assert_eq!(parse_hms("00:25:00"), Some(1500));
        assert_eq!(parse_hms("1:02:03"), Some(3723));
        assert_eq!(parse_hms("25:00"), None);
        assert_eq!(hms_text(3723), "01:02:03");
    }
}
