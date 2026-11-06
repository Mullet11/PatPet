//! Warna dari cover lagu untuk garis waktu dan kotak lirik.
//!
//! Diambil rona yang paling "hidup" di cover (bukan sekadar rata-rata, yang
//! sering jadi cokelat keruh), lalu terangnya diatur supaya:
//! - aksen tetap terbaca di atas latar gelap,
//! - latar kotak lirik selalu jelas berbeda dari warna island.

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    /// Garis waktu dan baris lirik yang sedang dinyanyikan.
    pub accent: Rgb,
    /// Latar kotak lirik.
    pub card: Rgb,
}

const HUE_BINS: usize = 24;
/// Cover yang hampir tanpa warna (hitam-putih) tidak diberi warna sendiri.
const MIN_VIVIDNESS: f32 = 0.02;

/// `pixels`: RGBA berurutan. `island`: warna latar island dari patpet.conf.
pub fn from_cover(pixels: &[[u8; 4]], island: Rgb) -> Option<Tint> {
    let mut weight = [0f32; HUE_BINS];
    let mut sum = [[0f32; 3]; HUE_BINS];
    let mut total = 0f32;

    for p in pixels.iter().step_by(2) {
        if p[3] < 128 {
            continue;
        }
        let (h, s, l) = to_hsl((p[0], p[1], p[2]));
        // Warna jenuh di tengah terang paling berpengaruh; hitam, putih, abu hampir tidak.
        let w = s * (1.0 - (2.0 * l - 1.0).abs());
        if w <= 0.0 {
            continue;
        }
        let bin = ((h / 360.0) * HUE_BINS as f32) as usize % HUE_BINS;
        weight[bin] += w;
        for c in 0..3 {
            sum[bin][c] += p[c] as f32 * w;
        }
        total += w;
    }
    if total / (pixels.len() as f32 / 2.0).max(1.0) < MIN_VIVIDNESS {
        return None;
    }

    let best = (0..HUE_BINS).max_by(|&a, &b| weight[a].total_cmp(&weight[b]))?;
    let avg = sum[best].map(|v| (v / weight[best]).round() as u8);
    let (h, s, _) = to_hsl((avg[0], avg[1], avg[2]));

    let (_, _, island_l) = to_hsl(island);
    // Latar kotak lirik: rona yang sama, redup, dan beda terang jelas dari island.
    let card_l = if island_l < 0.5 { (island_l + 0.12).min(0.3) } else { (island_l - 0.12).max(0.7) };
    Some(Tint {
        accent: from_hsl(h, s.clamp(0.35, 0.85), 0.68),
        card: from_hsl(h, s.min(0.45), card_l),
    })
}

fn to_hsl((r, g, b): Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to(r), to(g), to(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Rgb = (0, 0, 0);

    fn fill(color: [u8; 4], n: usize) -> Vec<[u8; 4]> {
        vec![color; n]
    }

    #[test]
    fn hsl_round_trips() {
        for c in [(255, 0, 0), (120, 130, 200), (253, 213, 181), (10, 200, 90)] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            assert!((back.0 as i32 - c.0 as i32).abs() <= 1 && (back.2 as i32 - c.2 as i32).abs() <= 1, "{c:?} → {back:?}");
        }
    }

    #[test]
    fn picks_the_vivid_color_over_a_dull_background() {
        // Sebagian besar abu gelap, sedikit biru terang.
        let mut px = fill([40, 40, 40, 255], 900);
        px.extend(fill([30, 90, 230, 255], 300));
        let t = from_cover(&px, BLACK).unwrap();
        let (h, _, l) = to_hsl(t.accent);
        assert!((200.0..240.0).contains(&h), "rona biru, dapat {h}");
        assert!((l - 0.68).abs() < 0.02);
    }

    #[test]
    fn grayscale_cover_has_no_tint() {
        let mut px = fill([20, 20, 20, 255], 500);
        px.extend(fill([230, 230, 230, 255], 500));
        assert_eq!(from_cover(&px, BLACK), None);
    }

    #[test]
    fn card_stays_apart_from_the_island() {
        let px = fill([200, 30, 60, 255], 400);
        let dark = from_cover(&px, BLACK).unwrap();
        assert!((to_hsl(dark.card).2 - 0.12).abs() < 0.02);

        let light = from_cover(&px, (240, 240, 240)).unwrap();
        assert!(to_hsl(light.card).2 <= to_hsl((240, 240, 240)).2 - 0.1);
    }
}
