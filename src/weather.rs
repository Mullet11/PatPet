//! Lokasi perangkat (Windows), nama daerah (OpenStreetMap Nominatim),
//! dan cuaca saat ini (Open-Meteo, tanpa API key).

use serde_json::Value;
use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};

use crate::http::{self, encode};

#[derive(Clone, Debug, PartialEq)]
pub struct City {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

pub enum Position {
    Found { lat: f64, lon: f64 },
    /// Akses lokasi Windows dimatikan untuk aplikasi ini.
    Denied,
    /// Tidak ada sensor, timeout, atau layanan lokasi tidak menjawab.
    Unavailable,
}

/// Memblokir sampai Windows menjawab; panggil dari thread latar.
pub fn device_position() -> Position {
    let access = Geolocator::RequestAccessAsync().and_then(|op| op.get());
    match access {
        Ok(GeolocationAccessStatus::Allowed) => {}
        Ok(_) => return Position::Denied,
        Err(_) => return Position::Unavailable,
    }
    let point = Geolocator::new()
        .and_then(|g| g.GetGeopositionAsync())
        .and_then(|op| op.get())
        .and_then(|p| p.Coordinate())
        .and_then(|c| c.Point())
        .and_then(|p| p.Position());
    match point {
        Ok(p) if is_valid(p.Latitude, p.Longitude) => Position::Found { lat: p.Latitude, lon: p.Longitude },
        _ => Position::Unavailable,
    }
}

/// "Banjarbaru, Kalimantan Selatan"
pub fn place_name(lat: f64, lon: f64) -> Option<String> {
    let path = format!("/reverse?format=jsonv2&zoom=10&accept-language=id&lat={lat:.4}&lon={lon:.4}");
    parse_place_name(&http::get("nominatim.openstreetmap.org", &path).ok()?)
}

/// Suhu (°C) dan kode cuaca WMO.
pub fn current(lat: f64, lon: f64) -> Option<(f64, i64)> {
    let path = format!(
        "/v1/forecast?latitude={lat:.4}&longitude={lon:.4}&current=temperature_2m,weather_code&timezone=auto"
    );
    parse_current(&http::get("api.open-meteo.com", &path).ok()?)
}

/// Cari kota lewat geocoding Open-Meteo; paling banyak lima hasil.
pub fn search_city(query: &str) -> Option<Vec<City>> {
    let query = query.trim();
    if !(2..=80).contains(&query.chars().count()) {
        return Some(Vec::new());
    }
    let path = format!("/v1/search?count=5&language=id&format=json&name={}", encode(query));
    Some(parse_cities(&http::get("geocoding-api.open-meteo.com", &path).ok()?))
}

pub fn describe(code: i64) -> &'static str {
    match code {
        0 => "Cerah",
        1 => "Cerah berawan",
        2 => "Berawan",
        3 => "Mendung",
        45 | 48 => "Berkabut",
        51 | 53 | 55 => "Gerimis",
        56 | 57 => "Gerimis beku",
        61 => "Hujan ringan",
        63 => "Hujan sedang",
        65 => "Hujan lebat",
        66 | 67 => "Hujan beku",
        71 | 73 | 75 | 77 => "Salju",
        80 | 81 => "Hujan lokal",
        82 => "Hujan deras",
        85 | 86 => "Hujan salju",
        95 => "Badai petir",
        96 | 99 => "Badai petir dan hujan es",
        _ => "Tidak diketahui",
    }
}

fn is_valid(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

fn parse_place_name(body: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    let address = json.get("address")?;
    let first = |keys: &[&str]| {
        keys.iter()
            .filter_map(|k| address.get(*k)?.as_str())
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(|s| s.chars().take(60).collect::<String>())
    };
    let local = first(&["city", "town", "municipality", "county", "village", "state_district"]);
    let region = first(&["state"]);
    match (local, region) {
        (Some(l), Some(r)) if l != r => Some(format!("{l}, {r}")),
        (Some(l), _) => Some(l),
        (None, r) => r,
    }
}

fn parse_cities(body: &str) -> Vec<City> {
    let Ok(json) = serde_json::from_str::<Value>(body) else { return Vec::new() };
    let Some(results) = json.get("results").and_then(Value::as_array) else { return Vec::new() };
    results
        .iter()
        .filter_map(|r| {
            let (lat, lon) = (r.get("latitude")?.as_f64()?, r.get("longitude")?.as_f64()?);
            if !is_valid(lat, lon) {
                return None;
            }
            let mut parts: Vec<&str> = Vec::new();
            for key in ["name", "admin1", "country"] {
                if let Some(v) = r.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
                    if !parts.contains(&v) {
                        parts.push(v);
                    }
                }
            }
            Some(City { name: parts.join(", ").chars().take(80).collect(), lat, lon })
        })
        .collect()
}

fn parse_current(body: &str) -> Option<(f64, i64)> {
    let json: Value = serde_json::from_str(body).ok()?;
    let now = json.get("current")?;
    let celsius = now.get("temperature_2m")?.as_f64()?;
    let code = now.get("weather_code")?.as_i64()?;
    Some((celsius, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_name_joins_city_and_province() {
        let body = r#"{"address":{"city":"Banjarbaru","state":"Kalimantan Selatan"}}"#;
        assert_eq!(parse_place_name(body).as_deref(), Some("Banjarbaru, Kalimantan Selatan"));
    }

    #[test]
    fn place_name_falls_back_and_skips_duplicates() {
        let body = r#"{"address":{"county":"Jakarta","state":"Jakarta"}}"#;
        assert_eq!(parse_place_name(body).as_deref(), Some("Jakarta"));
        assert_eq!(parse_place_name(r#"{"address":{"state":"Bali"}}"#).as_deref(), Some("Bali"));
        assert_eq!(parse_place_name(r#"{"error":"x"}"#), None);
    }

    #[test]
    fn current_weather_is_parsed() {
        let body = r#"{"current":{"temperature_2m":29.4,"weather_code":2}}"#;
        assert_eq!(parse_current(body), Some((29.4, 2)));
        assert_eq!(parse_current("bukan json"), None);
    }

    #[test]
    fn weather_codes_have_text() {
        assert_eq!(describe(0), "Cerah");
        assert_eq!(describe(99), "Badai petir dan hujan es");
        assert_eq!(describe(42), "Tidak diketahui");
    }

    #[test]
    fn cities_are_named_without_repeats() {
        let body = r#"{"results":[
            {"name":"Banjarbaru","admin1":"Kalimantan Selatan","country":"Indonesia","latitude":-3.44,"longitude":114.83},
            {"name":"Jakarta","admin1":"Jakarta","country":"Indonesia","latitude":-6.2,"longitude":106.8},
            {"name":"Rusak","latitude":200,"longitude":0}
        ]}"#;
        let cities = parse_cities(body);
        assert_eq!(cities.len(), 2);
        assert_eq!(cities[0].name, "Banjarbaru, Kalimantan Selatan, Indonesia");
        assert_eq!(cities[1].name, "Jakarta, Indonesia");
        assert!(parse_cities("{}").is_empty());
    }

    #[test]
    fn rejects_impossible_coordinates() {
        assert!(is_valid(-3.44, 114.83));
        assert!(!is_valid(91.0, 0.0));
        assert!(!is_valid(f64::NAN, 0.0));
    }
}
