using System.Globalization;
using System.Net.Http;
using System.Text.Json;
using Windows.Devices.Geolocation;

namespace PatPet.Services;

public sealed record GeoPlace(string Name, double Latitude, double Longitude);

public sealed record WeatherNow(double TemperatureC, int Code, bool IsDay);

/// <summary>
/// Cuaca dari Open-Meteo (tanpa API key) dan nama daerah dari lokasi Windows
/// (reverse geocode OpenStreetMap Nominatim) atau pencarian kota Open-Meteo.
/// Hanya HTTPS, timeout pendek, dan ukuran respons dibatasi.
/// </summary>
public sealed class WeatherService : IDisposable
{
    // kebijakan Nominatim mewajibkan User-Agent yang mengidentifikasi aplikasi (diatur di HttpClients)
    private readonly HttpClient _http = HttpClients.Create(TimeSpan.FromSeconds(20), 256 * 1024);

    // ---------- lokasi ----------

    public async Task<GeoPlace?> GetDeviceLocationAsync(CancellationToken ct = default)
    {
        try
        {
            var access = await Geolocator.RequestAccessAsync().AsTask(ct);
            if (access != GeolocationAccessStatus.Allowed) return null;

            var locator = new Geolocator { DesiredAccuracyInMeters = 3000 };
            var pos = await locator.GetGeopositionAsync(TimeSpan.FromMinutes(30), TimeSpan.FromSeconds(10)).AsTask(ct);
            var p = pos.Coordinate.Point.Position;
            if (!IsValid(p.Latitude, p.Longitude)) return null;

            // nama kosong = reverse geocode gagal (mis. jaringan belum siap); pemanggil akan mencoba lagi
            var name = await ReverseGeocodeAsync(p.Latitude, p.Longitude, ct) ?? "";
            return new GeoPlace(name, p.Latitude, p.Longitude);
        }
        catch
        {
            return null; // akses lokasi Windows mati, tidak ada sensor, timeout, dll.
        }
    }

    internal async Task<string?> ReverseGeocodeAsync(double lat, double lon, CancellationToken ct)
    {
        var url = string.Create(CultureInfo.InvariantCulture,
            $"https://nominatim.openstreetmap.org/reverse?format=jsonv2&zoom=10&accept-language=id&lat={lat:F4}&lon={lon:F4}");
        using var doc = await _http.GetJsonAsync(url, ct);
        if (doc is null || !doc.RootElement.TryGetProperty("address", out var a)) return null;

        var local = First(a, "city", "town", "municipality", "county", "village", "state_district");
        var region = First(a, "state");
        if (local is null) return region;
        return region is null || region == local ? local : $"{local}, {region}";
    }

    public async Task<IReadOnlyList<GeoPlace>> SearchCityAsync(string query, CancellationToken ct = default)
    {
        query = query.Trim();
        if (query.Length is < 2 or > 80) return Array.Empty<GeoPlace>();

        var url = $"https://geocoding-api.open-meteo.com/v1/search?count=5&language=id&format=json&name={Uri.EscapeDataString(query)}";
        using var doc = await _http.GetJsonAsync(url, ct);
        var list = new List<GeoPlace>();
        if (doc is null || !doc.RootElement.TryGetProperty("results", out var results) || results.ValueKind != JsonValueKind.Array)
            return list;

        foreach (var r in results.EnumerateArray())
        {
            if (!r.TryGetProperty("latitude", out var la) || !r.TryGetProperty("longitude", out var lo)) continue;
            double lat = la.GetDouble(), lon = lo.GetDouble();
            if (!IsValid(lat, lon)) continue;
            var name = First(r, "name") ?? "?";
            var admin = First(r, "admin1");
            var country = First(r, "country");
            var label = string.Join(", ", new[] { name, admin, country }.Where(s => !string.IsNullOrEmpty(s)).Distinct());
            list.Add(new GeoPlace(label, lat, lon));
        }
        return list;
    }

    // ---------- cuaca ----------

    public async Task<WeatherNow?> GetWeatherAsync(double lat, double lon, CancellationToken ct = default)
    {
        if (!IsValid(lat, lon)) return null;
        var url = string.Create(CultureInfo.InvariantCulture,
            $"https://api.open-meteo.com/v1/forecast?latitude={lat:F4}&longitude={lon:F4}&current=temperature_2m,weather_code,is_day&timezone=auto");
        using var doc = await _http.GetJsonAsync(url, ct);
        if (doc is null || !doc.RootElement.TryGetProperty("current", out var cur)) return null;
        try
        {
            return new WeatherNow(
                cur.GetProperty("temperature_2m").GetDouble(),
                cur.GetProperty("weather_code").GetInt32(),
                !cur.TryGetProperty("is_day", out var d) || d.GetInt32() == 1);
        }
        catch
        {
            return null;
        }
    }

    public static (string Text, string Glyph) Describe(int code, bool isDay) => code switch
    {
        0 => ("Cerah", isDay ? "☀" : "☾"),
        1 => ("Cerah berawan", isDay ? "⛅" : "☾"),
        2 => ("Berawan", "⛅"),
        3 => ("Mendung", "☁"),
        45 or 48 => ("Berkabut", "≋"),
        51 or 53 or 55 => ("Gerimis", "☂"),
        56 or 57 => ("Gerimis dingin", "☂"),
        61 => ("Hujan ringan", "☂"),
        63 => ("Hujan sedang", "☂"),
        65 => ("Hujan lebat", "☂"),
        66 or 67 => ("Hujan dingin", "☂"),
        71 or 73 or 75 or 77 => ("Salju", "❄"),
        80 or 81 => ("Hujan lokal", "☂"),
        82 => ("Hujan deras", "☂"),
        85 or 86 => ("Hujan salju", "❄"),
        95 => ("Badai petir", "⛈"),
        96 or 99 => ("Badai petir & es", "⛈"),
        _ => ("Tidak diketahui", "☁"),
    };

    // ---------- util ----------

    private static string? First(JsonElement e, params string[] names)
    {
        foreach (var n in names)
        {
            if (e.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.String)
            {
                var s = v.GetString();
                if (!string.IsNullOrWhiteSpace(s)) return s.Length > 60 ? s[..60] : s;
            }
        }
        return null;
    }

    internal static bool IsValid(double lat, double lon) =>
        double.IsFinite(lat) && double.IsFinite(lon) && lat is >= -90 and <= 90 && lon is >= -180 and <= 180;

    public void Dispose() => _http.Dispose();
}
