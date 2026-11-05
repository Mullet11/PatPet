using System.Globalization;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Services;

namespace PatPet.ViewModels;

public partial class ClockViewModel : ObservableObject
{
    private static readonly CultureInfo Indonesian = new("id-ID");
    private static readonly TimeSpan WeatherInterval = TimeSpan.FromMinutes(30);
    private static readonly TimeSpan DeviceLocationInterval = TimeSpan.FromHours(6);

    private readonly WeatherService _weather;
    private readonly SettingsService _settings;
    private DateTime _nextWeatherAttempt = DateTime.MinValue;
    private bool _busy;
    private int _failures;
    private static readonly int[] RetrySeconds = { 30, 60, 120, 300 };

    [ObservableProperty] private string _timeText = "";
    [ObservableProperty] private string _dateText = "";
    [ObservableProperty] private string _greeting = "";
    [ObservableProperty] private string _zoneLabel = "";
    [ObservableProperty] private string _locationText = "Mencari lokasi…";
    [ObservableProperty] private string _weatherGlyph = "";
    [ObservableProperty] private string _weatherText = "";
    [ObservableProperty] private string _temperatureText = "";
    [ObservableProperty] private bool _hasWeather;
    [ObservableProperty] private bool _isWeatherLoading;
    [ObservableProperty] private string _tooltipText = "PatPet";

    public ClockViewModel(WeatherService weather, SettingsService settings)
    {
        _weather = weather;
        _settings = settings;
    }

    public void Update(DateTime now)
    {
        // tanpa detik: properti hanya berubah tiap menit, jadi UI tidak dirender ulang tiap detik
        TimeText = now.ToString("HH:mm", CultureInfo.InvariantCulture);
        DateText = now.ToString("dddd, d MMMM yyyy", Indonesian);
        Greeting = GetGreeting(now.Hour);
        ZoneLabel = GetZoneLabel(TimeZoneInfo.Local.GetUtcOffset(now));
        UpdateTooltip();
    }

    private void UpdateTooltip()
    {
        var t = $"{TimeText} {ZoneLabel} · {DateText}";
        if (HasWeather) t += $"\n{LocationText} · {TemperatureText} {WeatherText}";
        TooltipText = t + "\nKlik untuk membuka, seret untuk memindah";
    }

    public static string GetGreeting(int hour) => hour switch
    {
        >= 4 and < 10 => "Selamat pagi",
        >= 10 and < 15 => "Selamat siang",
        >= 15 and < 18 => "Selamat sore",
        _ => "Selamat malam",
    };

    public static string GetZoneLabel(TimeSpan offset)
    {
        if (offset == TimeSpan.FromHours(7)) return "WIB";
        if (offset == TimeSpan.FromHours(8)) return "WITA";
        if (offset == TimeSpan.FromHours(9)) return "WIT";
        var sign = offset < TimeSpan.Zero ? "-" : "+";
        var abs = offset.Duration();
        return abs.Minutes == 0 ? $"UTC{sign}{abs.Hours}" : $"UTC{sign}{abs.Hours}:{abs.Minutes:00}";
    }

    /// <summary>Dipanggil tiap menit; hanya mengambil data bila sudah waktunya.</summary>
    public void MaybeRefreshWeather(DateTime now)
    {
        if (now >= _nextWeatherAttempt) _ = RefreshAsync(forceLocation: false);
    }

    [RelayCommand]
    private Task RefreshWeather() => RefreshAsync(forceLocation: true);

    public async Task RefreshAsync(bool forceLocation)
    {
        if (_busy) return;
        _busy = true;
        IsWeatherLoading = true;
        try
        {
            var place = await ResolvePlaceAsync(forceLocation);
            if (place is null)
            {
                LocationText = _settings.Current.Location.UseDeviceLocation
                    ? "Izinkan lokasi Windows atau pilih kota di tab Atur"
                    : "Pilih kota di tab Atur";
                HasWeather = false;
                _nextWeatherAttempt = DateTime.Now.AddMinutes(10);
                return;
            }

            LocationText = place.Name;
            var w = await _weather.GetWeatherAsync(place.Latitude, place.Longitude);
            if (w is null)
            {
                // jaringan lambat/putus: pertahankan cuaca terakhir, coba lagi lebih cepat dulu
                if (!HasWeather) WeatherText = "Memuat cuaca…";
                _nextWeatherAttempt = DateTime.Now.AddSeconds(RetrySeconds[Math.Min(_failures, RetrySeconds.Length - 1)]);
                _failures++;
                return;
            }
            _failures = 0;

            var (text, glyph) = WeatherService.Describe(w.Code, w.IsDay);
            WeatherText = text;
            WeatherGlyph = glyph;
            TemperatureText = $"{Math.Round(w.TemperatureC):0}°C";
            HasWeather = true;
            _nextWeatherAttempt = DateTime.Now + WeatherInterval;
        }
        finally
        {
            IsWeatherLoading = false;
            _busy = false;
            UpdateTooltip();
        }
    }

    private async Task<GeoPlace?> ResolvePlaceAsync(bool force)
    {
        var loc = _settings.Current.Location;
        var stale = loc.ResolvedAt is null || DateTime.Now - loc.ResolvedAt > DeviceLocationInterval
                    || loc.Latitude is null || string.IsNullOrEmpty(loc.Name) || LooksLikeCoordinates(loc.Name);
        if (loc.UseDeviceLocation && (force || stale))
        {
            var device = await _weather.GetDeviceLocationAsync();
            if (device is not null)
            {
                loc.Latitude = device.Latitude;
                loc.Longitude = device.Longitude;
                if (device.Name.Length > 0)
                {
                    loc.Name = device.Name;
                    loc.ResolvedAt = DateTime.Now;
                }
                else if (LooksLikeCoordinates(loc.Name))
                {
                    loc.Name = null; // jangan simpan koordinat sebagai nama; dicoba lagi berikutnya
                }
                _settings.Save();
            }
        }

        return loc.Latitude is { } lat && loc.Longitude is { } lon
            ? new GeoPlace(string.IsNullOrEmpty(loc.Name) ? "Lokasi saat ini" : loc.Name, lat, lon)
            : null;
    }

    private static bool LooksLikeCoordinates(string? s) =>
        !string.IsNullOrEmpty(s) && s.All(c => char.IsDigit(c) || c is ',' or '.' or '-' or ' ');
}
