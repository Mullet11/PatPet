namespace PatPet.Models;

public enum DisplayMode
{
    /// <summary>Bar hitam tetap di tengah atas layar: kucing, jam, baterai, suhu.</summary>
    Bar,
    /// <summary>Ikon kucing yang bisa dipindah ke mana saja.</summary>
    Icon,
}

public sealed class AppSettings
{
    public DisplayMode Mode { get; set; } = DisplayMode.Bar;
    public double? IconLeft { get; set; }
    public double? IconTop { get; set; }
    public bool AlwaysOnTop { get; set; } = true;
    /// <summary>Sembunyikan saat aplikasi lain layar penuh (video, game, presentasi).</summary>
    public bool HideInFullscreen { get; set; } = true;
    /// <summary>
    /// Tutup tray saat pindah ke aplikasi lain (Alt+Tab, klik jendela lain, desktop, taskbar). Bawaan: aktif.
    /// Kunci JSON baru agar nilai lama dari versi sebelumnya tidak terbawa.
    /// </summary>
    [System.Text.Json.Serialization.JsonPropertyName("closeOnFocusLoss")]
    public bool AutoCollapse { get; set; } = true;
    /// <summary>Skala ikon kucing pada mode ikon (1–3). 2 = 64 px.</summary>
    [System.Text.Json.Serialization.JsonPropertyName("dockScale")]
    public int PetScale { get; set; } = 2;
    public TimerSettings Timer { get; set; } = new();
    public int IdleSleepMinutes { get; set; } = 5;
    public LocationSettings Location { get; set; } = new();
    /// <summary>Lagu terakhir yang diputar (judul & artis saja) untuk ditampilkan saat hover.</summary>
    public LastTrack? LastTrack { get; set; }
}

public sealed class LastTrack
{
    public string Title { get; set; } = "";
    public string Artist { get; set; } = "";
}

public sealed class TimerSettings
{
    /// <summary>Durasi timer terakhir yang diatur pengguna (detik).</summary>
    public int DurationSeconds { get; set; } = 25 * 60;
    /// <summary>Bunyikan alarm saat timer habis.</summary>
    public bool AlarmEnabled { get; set; } = true;
    /// <summary>Alarm berhenti sendiri setelah sekian detik bila tidak dimatikan.</summary>
    public int AlarmSeconds { get; set; } = 60;
}

public sealed class LocationSettings
{
    /// <summary>true = pakai lokasi Windows; false = kota yang dipilih manual.</summary>
    public bool UseDeviceLocation { get; set; } = true;
    public string? Name { get; set; }
    public double? Latitude { get; set; }
    public double? Longitude { get; set; }
    public DateTime? ResolvedAt { get; set; }
}
