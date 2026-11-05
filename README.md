# PatPet 🐾

Widget desktop Windows dengan kucing pixel art. Dua mode tampilan (pilih di tab **Atur**):

| Mode | Saat tertutup | Saat diklik |
|---|---|---|
| **Bar atas** (default) | Bar hitam kecil, tetap di tengah atas layar: kucing, jam, sisa timer, ikon musik, baterai (+ animasi saat charger dicolok/dicabut), suhu laptop. Hover → bar melebar menampilkan cuaca + lagu sedang/terakhir diputar | Bar melebar dan tray hitam panjang menyambung di bawahnya |
| **Ikon bebas** | Hanya kucing, bisa diseret ke mana saja | Tray hitam muncul di dekat kucing (di bawah atau di atas, selalu di dalam layar) |

Di kedua mode kucing ikut berubah: tidur setelah idle 5 menit, pakai headphone saat musik diputar, membaca saat timer berjalan.

Tab di dalam tray (perpindahan tab dianimasikan geser + fade, mengikuti refresh rate monitor):

| Tab | Isi |
|---|---|
| **Waktu** | Jam tanpa detik + WIB/WITA/WIT, tanggal, nama daerah, cuaca, kucing ekspresif |
| **Musik** | Pemutar di kiri, **lirik tersinkron** (LRCLIB) di kanan, garis waktu yang bisa diseret di bawah |
| **Timer** | Satu timer dengan durasi bebas (jam:menit:detik , ketik, panah, atau gulir mouse), ubah sisa waktu saat berjalan, **alarm** saat habis (bisa dimatikan/dicoba) |
| **Catatan** | Catatan dengan deadline, selesai, pin, cari, undo, notifikasi jatuh tempo |
| **Atur** | Mode tampilan, lokasi, ukuran ikon, idle, always-on-top, sembunyi saat layar penuh |


## Kebutuhan

- Windows 10 1903+ / Windows 11
- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0) untuk build, atau .NET 8 Desktop Runtime untuk menjalankan hasil publish

## Menjalankan, test, publish

```bash
dotnet run --project PatPet.App
```

```bash
dotnet test
```

```bash
dotnet publish PatPet.App -c Release -r win-x64 --self-contained false -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=none -o publish/small
```

## Cara pakai

| Aksi | Hasil |
|---|---|
| Klik bar / kucing | Buka / tutup tray |
| Seret kucing (mode ikon) | Pindah ke mana saja (tersimpan) |
| Alt+Tab, klik aplikasi lain, Esc, atau klik bar/kucing lagi | Tutup tray (bisa dimatikan di **Atur**) |
| Arahkan mouse ke kucing (mode ikon) | Tooltip jam, tanggal, cuaca |
| Arahkan mouse ke bar (mode bar) | Bar melebar: suhu + keadaan cuaca, lagu sedang/terakhir diputar |
| Alarm berbunyi | Klik bar/kucing atau "Matikan alarm" (berhenti sendiri setelah 60 dtk) |
| Aplikasi lain layar penuh | PatPet otomatis bersembunyi (bisa dimatikan) |
| Ikon tray | Klik kiri: buka/tutup tray · klik kanan: sembunyikan, ganti mode, always on top, mulai/jeda timer, keluar |

## Cuaca & lokasi

- Lokasi diambil dari layanan lokasi Windows (Pengaturan › Privasi › Lokasi › izinkan aplikasi desktop).
  Jika tidak diizinkan, pilih kota manual di tab **Atur**.
- Cuaca: [Open-Meteo](https://open-meteo.com) (tanpa API key). Nama daerah: OpenStreetMap Nominatim.
  Yang dikirim hanya koordinat dibulatkan 4 desimal / nama kota yang dicari. Cuaca diperbarui tiap 30 menit.

## Performa & keamanan

- **Render software** (tanpa perangkat Direct3D GPU): penghematan terbesar, ±60 MB → ±28 MB. Cocok untuk widget
  kecil dengan animasi 4–8 fps; CPU tetap ±0,1%.
- Saat tertutup hanya bar/ikon kecil dengan satu timer animasi; animasi baterai hanya ±3 detik saat charger berubah.
  Tray dibuat saat dibuka dan dibuang saat ditutup (hanya satu halaman yang hidup), lalu GC + trim working set sekali.
- Lirik hanya diambil saat tab Musik terbuka, di-cache di memori (6 lagu), tidak disimpan ke disk.
- Bunyi alarm dibuat di memori (±60 KB) hanya saat berbunyi, lalu dibuang.
- Baterai via `GetSystemPowerStatus`; suhu via performance counter "Thermal Zone Information" (tanpa admin,
  tanpa driver). Ini sensor ACPI laptop, bukan suhu per-core CPU; di PC tanpa sensor, suhu disembunyikan.
- Terukur (Release, mode bar), kolom Memori Task Manager: ±5–11 MB saat tertutup, ±32 MB saat baru jalan,
  ±38 MB saat tray terbuka di halaman Musik; stabil setelah 20 siklus buka/tutup. Memori yang dialokasikan
  (*private bytes*) ±40–50 MB saat tertutup, ±60 MB saat tray terbuka.
- Deteksi layar penuh mengabaikan jendela yang sekadar di-maximize (penting saat taskbar auto-hide).
- PatPet hanya tampil di monitor utama. Ikon tidak bisa diseret ke monitor kedua, dan aplikasi layar penuh di monitor kedua tidak menyembunyikannya.
- Tray berukuran tetap 400 px, tiap halaman setinggi 240 px.
- Semua query SQLite berparameter (termasuk escape wildcard pencarian), panjang catatan dibatasi 4000 karakter.
- HTTP hanya HTTPS, timeout (cuaca 20 dtk + coba ulang 30 dtk–5 mnt, lirik 30 dtk), respons maks 256/512 KB, kedalaman JSON dibatasi;
  tanpa API key/rahasia. Panjang dan jumlah baris lirik dibatasi.
- `settings.json` divalidasi saat dibaca (nilai di luar rentang dikoreksi), ditulis atomik; `error.log` dibatasi 512 KB.
- Cover album maks 2 MB, di-decode ke 128 px, cache 4 gambar.

## Data

Semua di `%AppData%\PatPet`: `settings.json`, `notes.db`, `backups\` (backup harian, 7 terakhir), `error.log`.
