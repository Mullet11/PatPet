//! Letak jendela pada mode ikon. Jendela selebar panel; kucing berada di
//! dalamnya, dan panel muncul di bawah kucing atau di atasnya bila tidak muat.
//! Semua nilai dalam piksel fisik.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Lebar island dan panel dalam piksel logis, proporsional terhadap layar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Widths {
    /// Island tertutup: kucing, jam, baterai.
    pub closed: f32,
    /// Island saat hover atau ada notifikasi.
    pub wide: f32,
    /// Panel Musik, Timer, Catatan, Shelf. Panel Waktu tetap 420.
    pub panel: f32,
}

impl Widths {
    /// `closed`, `wide`, `panel`: bagian dari lebar layar (0,15 = 15%).
    /// Hasilnya dibatasi supaya tetap terbaca di layar kecil maupun sangat lebar.
    pub fn for_screen(screen: f32, closed: f32, wide: f32, panel: f32) -> Self {
        Self {
            closed: (screen * closed).clamp(200.0, 320.0),
            wide: (screen * wide).clamp(320.0, 480.0),
            panel: (screen * panel).clamp(520.0, 720.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconLayout {
    /// Kiri-atas jendela di layar.
    pub window: (i32, i32),
    /// Kiri-atas kucing, relatif terhadap jendela.
    pub pet: (i32, i32),
    /// Kiri-atas kucing di layar setelah dijaga tetap di dalam layar.
    pub pet_on_screen: (i32, i32),
    pub panel_below: bool,
}

/// `pet`: posisi kucing yang diinginkan; `pet_size`: sisi kucing;
/// `gap`: jarak kucing–panel; `width`/`panel`: lebar jendela dan tinggi panel.
pub fn icon_layout(pet: (i32, i32), pet_size: i32, gap: i32, width: i32, panel: i32, work: Rect) -> IconLayout {
    let x = pet.0.clamp(work.left, (work.right - pet_size).max(work.left));
    let y = pet.1.clamp(work.top, (work.bottom - pet_size).max(work.top));

    let fits_below = y + pet_size + gap + panel <= work.bottom;
    let fits_above = y - gap - panel >= work.top;
    let panel_below = fits_below || !fits_above;

    let height = pet_size + gap + panel;
    let window_x = (x + pet_size / 2 - width / 2).clamp(work.left, (work.right - width).max(work.left));
    let window_y = if panel_below { y } else { y + pet_size - height };

    IconLayout {
        window: (window_x, window_y),
        pet: (x - window_x, y - window_y),
        pet_on_screen: (x, y),
        panel_below,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };

    fn layout(x: i32, y: i32) -> IconLayout {
        icon_layout((x, y), 64, 8, 420, 300, SCREEN)
    }

    #[test]
    fn widths_follow_screen_within_limits() {
        let ratios = |w| Widths::for_screen(w, 0.15, 0.24, 0.38);
        let laptop = ratios(1536.0);
        assert!((laptop.closed - 230.4).abs() < 0.01);
        assert!((laptop.wide - 368.64).abs() < 0.01);
        assert!((laptop.panel - 583.68).abs() < 0.01);

        let small = ratios(1024.0);
        assert_eq!((small.closed, small.wide, small.panel), (200.0, 320.0, 520.0));

        let wide = ratios(3440.0);
        assert_eq!((wide.closed, wide.wide, wide.panel), (320.0, 480.0, 720.0));
    }

    #[test]
    fn panel_opens_below_with_cat_centered() {
        let l = layout(900, 100);
        assert!(l.panel_below);
        assert_eq!(l.window, (900 + 32 - 210, 100));
        assert_eq!(l.pet, (210 - 32, 0));
    }

    #[test]
    fn panel_flips_above_near_bottom() {
        let l = layout(900, 1000);
        assert!(!l.panel_below);
        assert_eq!(l.pet_on_screen, (900, 1000));
        assert_eq!(l.window.1 + l.pet.1, 1000);
        assert_eq!(l.window.1, 1000 + 64 - 372);
    }

    #[test]
    fn window_stays_on_screen_at_the_edges() {
        let l = layout(1900, 50);
        assert_eq!(l.pet_on_screen, (1920 - 64, 50));
        assert_eq!(l.window.0, 1920 - 420);
        assert_eq!(l.window.0 + l.pet.0, 1920 - 64);

        let l = layout(-500, -500);
        assert_eq!(l.pet_on_screen, (0, 0));
        assert_eq!(l.window, (0, 0));
    }
}
