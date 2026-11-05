using System.Text.Json;
using System.Text.Json.Serialization;
using PatPet.Models;

namespace PatPet.Services;

public sealed class SettingsService
{
    private const long MaxFileBytes = 64 * 1024;

    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
        Converters = { new JsonStringEnumConverter() },
    };

    private readonly string _path;

    public SettingsService() : this(AppPaths.SettingsFile) { }

    public SettingsService(string path) => _path = path;

    public AppSettings Current { get; private set; } = new();

    public void Load()
    {
        try
        {
            var fi = new FileInfo(_path);
            if (fi.Exists && fi.Length <= MaxFileBytes)
                Current = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(_path), Options) ?? new AppSettings();
        }
        catch
        {
            Current = new AppSettings(); // file rusak → pakai default
        }
        Sanitize(Current);
    }

    /// <summary>Nilai dari file bisa diubah tangan: batasi ke rentang yang aman.</summary>
    internal static void Sanitize(AppSettings s)
    {
        s.Timer ??= new TimerSettings();
        s.Location ??= new LocationSettings();
        if (!Enum.IsDefined(s.Mode)) s.Mode = DisplayMode.Bar;
        s.Timer.DurationSeconds = Math.Clamp(s.Timer.DurationSeconds, 1, (int)FocusTimerService.MaxDuration.TotalSeconds);
        s.Timer.AlarmSeconds = Math.Clamp(s.Timer.AlarmSeconds, 5, 600);
        s.IdleSleepMinutes = Math.Clamp(s.IdleSleepMinutes, 1, 120);
        s.PetScale = Math.Clamp(s.PetScale, 1, 3);
        if (s.IconLeft is { } l && !double.IsFinite(l)) s.IconLeft = null;
        if (s.IconTop is { } t && !double.IsFinite(t)) s.IconTop = null;

        var loc = s.Location;
        if (loc.Latitude is not { } lat || loc.Longitude is not { } lon || !WeatherService.IsValid(lat, lon))
        {
            loc.Latitude = loc.Longitude = null;
        }
        if (loc.Name is { Length: > 80 }) loc.Name = loc.Name[..80];
        if (s.LastTrack is { } lt)
        {
            lt.Title ??= "";
            lt.Artist ??= "";
            if (lt.Title.Length > 120) lt.Title = lt.Title[..120];
            if (lt.Artist.Length > 120) lt.Artist = lt.Artist[..120];
        }
    }

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_path)!);
            var tmp = _path + ".tmp";
            File.WriteAllText(tmp, JsonSerializer.Serialize(Current, Options));
            File.Move(tmp, _path, overwrite: true);
        }
        catch
        {
            // gagal menyimpan pengaturan tidak boleh menjatuhkan aplikasi
        }
    }
}
