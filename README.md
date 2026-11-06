# PatPet 🐾

Widget desktop Windows dengan kucing pixel art. Ditulis dengan Rust dan [Slint](https://slint.dev): satu file `.exe` sekitar 8 MB, tanpa runtime tambahan, memakai RAM sekitar 13 MB.

Versi C#/WPF terakhir (1.4.0) disimpan di tag `v1.4-csharp`. Catatan dan pengaturannya terbaca langsung oleh versi ini.

## Dua mode tampilan

Pilih lewat `mode` di `patpet.conf`.

| Mode | Saat tertutup | Saat diklik |
|---|---|---|
| **Bar atas** (bawaan) | Bar hitam di tengah atas layar seperti notch: kucing, jam, dan sisa timer. Saat kursor di atasnya, bar melebar dan menampilkan ikon musik, suhu laptop, dan baterai | Panel hitam tumbuh dari bar |
| **Ikon kucing** | Hanya kucing, bisa diseret ke mana saja | Panel muncul di bawah kucing, atau di atasnya bila tidak muat |

Kucing ikut keadaanmu: tidur setelah diam beberapa menit, memakai headphone saat musik diputar, membaca saat timer berjalan, menguap larut malam, dan melompat senang saat dielus.

## Tab di panel

| Tab | Isi |
|---|---|
| **Waktu** | Jam dengan WIB/WITA/WIT, tanggal, sapaan, cuaca, dan kucing yang bisa dielus |
| **Musik** | Lagu yang diputar di Spotify desktop, cover, tombol putar/jeda, dan garis waktu yang bisa diklik. Lirik tersinkron (LRCLIB) bisa digulir dan mengikuti lagu; klik satu baris untuk lompat ke sana. Warna garis waktu dan kotak lirik mengikuti cover lagu |
| **Timer** | Durasi bebas (ketik, panah, atau gulir mouse), ubah sisa waktu saat berjalan, alarm, dan notifikasi saat habis |
| **Catatan** | Catatan dengan deadline, tanda selesai, sematan, pencarian, batal hapus, dan notifikasi saat deadline lewat. Deadline dipilih lewat ikon kalender: kalender bulanan dan roda jam/menit yang bisa digulir, diseret, atau diklik |
| **Shelf** | Tempat menaruh file sementara, tombol screenshot, dan riwayat clipboard. Lihat di bawah |

Arahkan kursor ke tombol atau kontrol mana pun untuk melihat keterangannya. Klik island, kucing, atau ikon tray untuk membuka panel. Klik di luar panel atau tekan Esc untuk menutupnya. Untuk keluar, klik kanan ikon PatPet di system tray lalu pilih **Keluar**.

## Shelf

Shelf bekerja seperti potong dan tempel, tapi dengan seret:

1. Seret file atau folder dari Explorer ke island. Panel terbuka sendiri di tab Shelf; lepaskan di sana.
2. Pindah ke folder tujuan kapan saja, lalu seret kartu dari Shelf ke folder itu. Explorer yang memindahkan filenya, dan kartunya hilang dari Shelf.

Selama belum dipindahkan, file tetap di tempat asalnya dan kartunya tetap ada, juga setelah PatPet atau komputer dinyalakan ulang. Shelf hanya menyimpan lokasi file di `shelf.json`. Tombol × pada kartu mengeluarkan file dari Shelf tanpa menghapus filenya. Klik kartu untuk membuka filenya. Bila banyak, gulir mouse untuk menggeser deretan ke samping.

**Screenshot:** tombol kamera membuka alat potong layar Windows. Hasilnya disimpan sebagai PNG di `%APPDATA%\PatPet\Shelf` dan langsung muncul di Shelf.

**Riwayat clipboard:** 20 teks dan gambar terakhir yang Anda salin. Klik kartu untuk menyalinnya lagi; tombol + pada kartu gambar menyimpannya sebagai file di Shelf. Riwayat hanya ada di memori dan hilang saat PatPet ditutup. Isi yang ditandai rahasia oleh aplikasinya (mis. kata sandi dari pengelola kata sandi) tidak dicatat. Gambar disimpan terkompresi, paling banyak 8 gambar.

## Notifikasi di island

Semua notifikasi PatPet muncul di island, bukan di pojok kanan bawah layar. Island membesar sebentar lalu kembali; mengklik notifikasi membuka tab terkait (mis. deadline membuka Catatan).

| Notifikasi | Kapan |
|---|---|
| Waktu habis | Timer selesai; tetap tampil sampai diklik bila alarm berbunyi |
| Deadline | Catatan melewati deadline |
| Mengisi daya / Charger dicabut | Charger dicolok atau dicabut, dengan persen baterai |
| Baterai lemah | Baterai turun melewati 20% dan 10% |
| Volume / Suara dimatikan | Volume speaker utama berubah |
| Kecerahan | Kecerahan layar berubah |
| Hemat baterai nyala/mati | Mode hemat baterai Windows berubah |
| Caps Lock nyala/mati | Tombol Caps Lock ditekan |
| Pengaturan | Ada baris `patpet.conf` yang tidak valid |
| Screenshot disimpan | Hasil tombol screenshot sudah masuk Shelf |

### Notifikasi dari semua aplikasi

PatPet juga bisa menampilkan notifikasi dari aplikasi lain (WhatsApp, Chrome, Discord, Windows Update, dan lainnya) lengkap dengan logo aplikasinya; mengkliknya membuka aplikasi itu. Windows hanya mengizinkan ini untuk aplikasi yang punya identitas, jadi PatPet perlu dipasang lewat skrip:

```bash
cargo build --release
```

```bash
powershell -ExecutionPolicy Bypass -File packaging/install.ps1
```

Skrip ini menyalin `patpet.exe` ke `%LOCALAPPDATA%\Programs\PatPet`, membuat paket identitas kecil yang menunjuk ke exe itu, menandatanganinya dengan sertifikat "CN=PatPet Local" yang dibuat di komputer Anda, lalu mendaftarkannya. Izin admin hanya diminta sekali, untuk mempercayai sertifikat itu. Kunci sertifikat tetap di sertifikat pribadi Windows Anda dan tidak masuk ke repo.

Setelah itu:

1. Pastikan **Settings → Privacy & security → Notifications → Let apps access your notifications** menyala.
2. Matikan **Settings → System → Notifications → Show notification banners**, supaya notifikasi tidak muncul dua kali (di island dan di pojok kanan bawah).

PatPet membaca daftar notifikasi Windows setiap 2 detik dan hanya menampilkan yang baru. Tanpa pemasangan ini (mis. saat dijalankan dengan `cargo run`), fitur ini tidak aktif dan PatPet tetap berjalan normal. Untuk menghapus paket identitas: `packaging/uninstall.ps1` (tambah `-RemoveCertificate` untuk menghapus sertifikatnya juga).

Untuk Caps Lock, PatPet memakai kait keyboard Windows yang hanya memperhatikan tombol Caps Lock; tombol lain tidak dibaca atau dicatat. Pop-up volume dan kecerahan bawaan Windows tetap muncul karena Windows tidak menyediakan cara resmi untuk mematikannya.

## Pengaturan: `patpet.conf`

Klik kanan island (atau kucing pada mode ikon) untuk membuka `%APPDATA%\PatPet\patpet.conf` di Notepad. Ubah nilainya, simpan, dan PatPet langsung memakainya tanpa perlu ditutup. Isinya:

- **Tampilan:** mode bar atau ikon, ukuran kucing, sembunyi otomatis, lebar island dan panel dalam persen layar
- **Perilaku:** selalu di atas, tutup panel saat pindah aplikasi, sembunyi saat layar penuh, kapan kucing tidur
- **Timer:** durasi awal, alarm, lama alarm
- **Cuaca:** `otomatis` (lokasi Windows) atau nama kota
- **Warna:** semua warna UI dalam kode hex (`#RRGGBB` atau `#RRGGBBAA`), lengkap dengan penjelasan tiap warna

Setiap baris diberi penjelasan. Kalau ada nilai yang salah ketik, PatPet tetap jalan dengan nilai bawaan untuk baris itu dan menyebut nomor barisnya di island. Hapus file ini untuk kembali ke semua nilai bawaan. Template-nya ada di [`assets/default.conf`](assets/default.conf).

Data lain di folder yang sama: `notes.db` dan cadangan harian catatan di `backups` (7 terakhir), `shelf.json` (daftar file di Shelf), dan folder `Shelf` untuk screenshot. `settings.json` dari versi lama dipindahkan otomatis ke `patpet.conf` saat pertama kali dijalankan.

Privasi: untuk cuaca, yang dikirim ke Open-Meteo dan OpenStreetMap hanya koordinat yang dibulatkan. Lirik dicari di LRCLIB berdasarkan judul dan artis lagu.

## Target dan hasil

| | Target | Hasil |
|---|---|---|
| Ukuran `.exe` | < 10 MB | 8,2 MB |
| RAM (private working set) | ≤ 50 MB | 13 MB (dengan 10 file di Shelf dan gambar di riwayat clipboard) |
| CPU saat diam | serendah mungkin | 0,1% |

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
| `ui/parts.slint` | Komponen bersama: tombol, chip, sakelar, kolom teks, kolom angka, ikon |
| `ui/*_panel.slint`, `ui/main.slint` | Isi tiap tab dan jendela utama |
| `src/main.rs` | Status aplikasi dan penghubung UI |
| `src/platform.rs` | Jendela Win32 transparan per piksel yang digambar renderer software Slint, keyboard, pemantau clipboard |
| `src/placement.rs` | Letak kucing dan panel pada mode ikon |
| `src/conf.rs` | Format `patpet.conf`: baca dan ubah nilai tanpa menghapus komentar |
| `src/settings.rs`, `src/preferences.rs` | Pengaturan dari `patpet.conf`, perpindahan dari `settings.json`, muat ulang otomatis |
| `src/colors.rs` | Warna UI dari `patpet.conf` |
| `src/tray.rs` | Ikon dan menu di system tray |
| `src/notices.rs` | Antrean notifikasi island |
| `src/audio.rs` | Volume speaker lewat Core Audio |
| `src/listener.rs` | Notifikasi aplikasi lain lewat UserNotificationListener |
| `src/image.rs` | Dekode/enkode gambar (cover, logo aplikasi, PNG) lewat WIC dan gambar kecil file dari Explorer |
| `src/pet.rs` | Logika suasana hati kucing |
| `src/clock.rs` | Waktu lokal, sapaan, dan zona WIB/WITA/WIT |
| `src/system.rs` | Baterai dan suhu laptop |
| `src/weather.rs`, `src/http.rs` | Lokasi, pencarian kota, nama daerah, dan cuaca (Open-Meteo, OpenStreetMap) lewat WinHTTP |
| `src/background.rs` | Thread latar untuk suhu, cuaca, kota, dan lirik |
| `src/media.rs` | Sesi Spotify lewat Windows Media Session, cover didekode WIC |
| `src/lyrics.rs`, `src/player.rs` | Lirik LRCLIB, parser LRC, dan status tab Musik |
| `src/tint.rs` | Warna dari cover lagu untuk tab Musik |
| `src/timer.rs`, `src/alarm.rs` | Hitung mundur dan bunyi alarm yang dibuat di memori |
| `src/notes.rs`, `src/notes_view.rs` | Catatan di SQLite dan status tab Catatan |
| `src/shelf.rs`, `src/shelf_view.rs` | Daftar file Shelf dan status tab Shelf |
| `src/dragdrop.rs` | Seret file masuk ke island dan keluar ke Explorer (OLE) |
| `src/clipboard.rs`, `src/clip_history.rs` | Clipboard teks dan gambar, riwayat clipboard di memori |
| `src/paths.rs` | Lokasi folder data |
| `assets/pet` | Sprite kucing 32×32 |
| `assets/default.conf` | Template `patpet.conf` beserta penjelasannya |
| `assets/patpet.manifest` | Manifest exe: DPI dan tautan ke paket identitas |
| `packaging/` | Paket identitas (`AppxManifest.xml`) serta skrip pasang dan hapus |

UI dibuat dengan [Slint](https://slint.dev) (lisensi royalty-free, dengan atribusi).
