//! Lirik dari LRCLIB (lrclib.net, gratis tanpa API key). Lirik tersinkron
//! (format LRC) diutamakan supaya baris yang sedang dinyanyikan bisa disorot.

use std::time::Duration;

use serde_json::Value;

use crate::http::{self, encode};

const MAX_LINES: usize = 400;
const MAX_LINE_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub enum Lyrics {
    /// Baris berurutan waktu.
    Synced(Vec<(Duration, String)>),
    /// Teks biasa tanpa waktu.
    Plain(String),
    Instrumental,
}

#[derive(Debug, PartialEq)]
pub enum Fetch {
    Found(Lyrics),
    NotFound,
    Failed,
}

pub fn fetch(title: &str, artist: &str, album: &str, duration: Duration) -> Fetch {
    let title = clean(title);
    let artist = clean(artist);
    if title.is_empty() {
        return Fetch::NotFound;
    }

    // 1) Pencocokan tepat: judul, artis, album, durasi.
    let mut path = format!("/api/get?track_name={}&artist_name={}", encode(&title), encode(&artist));
    if !album.trim().is_empty() {
        path += &format!("&album_name={}", encode(&clean(album)));
    }
    if !duration.is_zero() {
        path += &format!("&duration={}", duration.as_secs_f64().round() as u64);
    }
    match http::get("lrclib.net", &path) {
        Ok(body) => {
            if let Some(found) = serde_json::from_str(&body).ok().as_ref().and_then(parse) {
                return Fetch::Found(found);
            }
        }
        Err(http::Error::Status(404)) => {}
        Err(_) => return Fetch::Failed,
    }

    // 2) Pencarian longgar: hasil pertama yang punya lirik.
    let path = format!("/api/search?track_name={}&artist_name={}", encode(&title), encode(&artist));
    let Ok(body) = http::get("lrclib.net", &path) else { return Fetch::Failed };
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&body) else { return Fetch::NotFound };
    items.iter().take(10).find_map(parse).map_or(Fetch::NotFound, Fetch::Found)
}

/// Indeks baris terakhir yang waktunya ≤ `position`; `None` sebelum baris pertama.
pub fn index_at(lines: &[(Duration, String)], position: Duration) -> Option<usize> {
    lines.partition_point(|(t, _)| *t <= position).checked_sub(1)
}

fn parse(item: &Value) -> Option<Lyrics> {
    if item.get("instrumental").and_then(Value::as_bool) == Some(true) {
        return Some(Lyrics::Instrumental);
    }
    if let Some(lrc) = item.get("syncedLyrics").and_then(Value::as_str) {
        let lines = parse_lrc(lrc);
        if !lines.is_empty() {
            return Some(Lyrics::Synced(lines));
        }
    }
    let plain = item.get("plainLyrics").and_then(Value::as_str)?.trim();
    if plain.is_empty() {
        return None;
    }
    let text: Vec<String> = plain.lines().take(MAX_LINES).map(|l| limit(l.trim())).collect();
    Some(Lyrics::Plain(text.join("\n")))
}

/// Format LRC: "[mm:ss.xx] teks". Satu baris bisa punya beberapa tag waktu;
/// tag lain seperti "[ar:...]" diabaikan.
fn parse_lrc(lrc: &str) -> Vec<(Duration, String)> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim_start();
        let mut times = Vec::new();
        while let Some(tag) = rest.strip_prefix('[') {
            let Some(end) = tag.find(']') else { break };
            let Some(time) = parse_time(&tag[..end]) else { break };
            times.push(time);
            rest = &tag[end + 1..];
        }
        let text = limit(rest.trim());
        lines.extend(times.into_iter().map(|t| (t, text.clone())));
        if lines.len() >= MAX_LINES {
            break;
        }
    }
    lines.sort_by_key(|(t, _)| *t);
    lines
}

/// "01:02.345" atau "1:02:34" → durasi.
fn parse_time(tag: &str) -> Option<Duration> {
    let (min, sec) = tag.split_once(':')?;
    let min: u64 = min.parse().ok().filter(|_| (1..=3).contains(&min.len()))?;
    let sec: f64 = sec.replacen(':', ".", 1).parse().ok().filter(|s: &f64| (0.0..60.0).contains(s))?;
    Some(Duration::from_secs(min * 60) + Duration::from_secs_f64(sec))
}

fn clean(s: &str) -> String {
    s.trim().chars().take(200).collect()
}

fn limit(s: &str) -> String {
    s.chars().take(MAX_LINE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn parses_and_sorts_timed_lines() {
        let lines = parse_lrc("[00:12.50] baris kedua\n[00:03.00] baris pertama\n[01:02.345]baris ketiga\ntanpa waktu");
        assert_eq!(
            lines,
            vec![
                (ms(3000), "baris pertama".into()),
                (ms(12500), "baris kedua".into()),
                (ms(62345), "baris ketiga".into()),
            ]
        );
    }

    #[test]
    fn line_with_several_tags_repeats() {
        let lines = parse_lrc("[00:10.00][00:40.00] reff contoh");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|(_, t)| t == "reff contoh"));
    }

    #[test]
    fn metadata_tags_are_ignored() {
        assert!(parse_lrc("[ar: Penyanyi]\n[length: 03:20]").is_empty());
    }

    #[test]
    fn index_follows_position() {
        let lines = parse_lrc("[00:05.00] a\n[00:10.00] b\n[00:20.00] c");
        assert_eq!(index_at(&lines, ms(1000)), None);
        assert_eq!(index_at(&lines, ms(5000)), Some(0));
        assert_eq!(index_at(&lines, ms(19_900)), Some(1));
        assert_eq!(index_at(&lines, ms(180_000)), Some(2));
    }

    #[test]
    fn synced_preferred_over_plain() {
        let v: Value = serde_json::from_str(r#"{"syncedLyrics":"[00:01.00] satu","plainLyrics":"satu"}"#).unwrap();
        assert_eq!(parse(&v), Some(Lyrics::Synced(vec![(ms(1000), "satu".into())])));
    }

    #[test]
    fn plain_and_instrumental_are_handled() {
        let plain: Value = serde_json::from_str(r#"{"syncedLyrics":null,"plainLyrics":"satu\ndua"}"#).unwrap();
        assert_eq!(parse(&plain), Some(Lyrics::Plain("satu\ndua".into())));

        let ins: Value = serde_json::from_str(r#"{"instrumental":true}"#).unwrap();
        assert_eq!(parse(&ins), Some(Lyrics::Instrumental));

        let empty: Value = serde_json::from_str(r#"{"plainLyrics":""}"#).unwrap();
        assert_eq!(parse(&empty), None);
    }
}
