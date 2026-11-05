# PatPet 🐾

Widget desktop Windows dengan kucing pixel art yang menempel di tengah atas layar seperti notch.

> **Sedang ditulis ulang ke Rust.** Versi C#/WPF terakhir (1.4.0) disimpan di tag `v1.4-csharp`.

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Bar atas, kucing, jam, tanggal saat hover | Selesai |
| 1 | Ikon tray, tab Waktu, cuaca, baterai, suhu | Belum |
| 2 | Musik (Spotify) dan lirik tersinkron | Belum |
| 3 | Timer dan alarm | Belum |
| 4 | Catatan dan notifikasi jatuh tempo | Belum |
| 5 | Pengaturan, mode ikon bebas, sembunyi saat layar penuh | Belum |

Untuk sementara, klik kanan bar untuk menutup aplikasi.

## Target

- File `.exe` di bawah 10 MB, tanpa runtime tambahan
- RAM (private working set) di bawah 50 MB
- Tidak memakai CPU saat diam

Hasil fase 0: exe 6,4 MB, RAM 1,2 MB, CPU rata-rata 0,007% saat diam.

## Kebutuhan build

- Windows 10 1903+ / Windows 11
- [Rust](https://rustup.rs) dengan toolchain MSVC
- Visual Studio Build Tools (C++)

## Menjalankan, test, build

```bash
cargo run
```

```bash
cargo test
```

```bash
cargo build --release
```

Hasilnya ada di `target/release/patpet.exe`.

## Struktur

| Lokasi | Isi |
|---|---|
| `ui/theme.slint` | Semua warna, ukuran, jarak, dan durasi animasi. Komponen lain wajib mengambil nilai dari sini |
| `ui/*.slint` | Tampilan |
| `src/platform.rs` | Jendela Win32 transparan per piksel yang digambar renderer software Slint |
| `src/pet.rs` | Logika suasana hati kucing |
| `src/clock.rs` | Waktu lokal dan format tanggal |
| `assets/pet` | Sprite kucing 32×32 |

UI dibuat dengan [Slint](https://slint.dev) (lisensi royalty-free, dengan atribusi).
