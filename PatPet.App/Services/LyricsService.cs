using System.Globalization;
using System.Net.Http;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace PatPet.Services;

public sealed record LyricLine(TimeSpan Time, string Text);

public sealed record LyricsResult(IReadOnlyList<LyricLine> Lines, bool Synced, bool Instrumental);

/// <summary>
/// Lirik dari LRCLIB (lrclib.net, gratis tanpa API key). Lirik tersinkron (LRC) diutamakan
/// agar baris yang sedang dinyanyikan bisa disorot. Hasil hanya disimpan di memori.
/// </summary>
public sealed partial class LyricsService : IDisposable
{
    private const int MaxLines = 400;
    private const int MaxLineLength = 200;

    private readonly HttpClient _http = HttpClients.Create(TimeSpan.FromSeconds(30), 512 * 1024);

    public async Task<LyricsResult?> GetAsync(string title, string artist, string album, TimeSpan duration, CancellationToken ct)
    {
        title = Clean(title);
        artist = Clean(artist);
        if (title.Length == 0) return null;

        // 1) pencocokan tepat (judul + artis + album + durasi)
        var url = "https://lrclib.net/api/get?track_name=" + Uri.EscapeDataString(title)
                  + "&artist_name=" + Uri.EscapeDataString(artist);
        if (!string.IsNullOrWhiteSpace(album)) url += "&album_name=" + Uri.EscapeDataString(Clean(album));
        if (duration > TimeSpan.Zero) url += "&duration=" + ((int)Math.Round(duration.TotalSeconds)).ToString(CultureInfo.InvariantCulture);

        using (var doc = await _http.GetJsonAsync(url, ct))
        {
            if (doc is not null && Parse(doc.RootElement) is { } exact) return exact;
        }
        ct.ThrowIfCancellationRequested();

        // 2) pencarian longgar (judul + artis), ambil hasil pertama yang punya lirik
        var search = "https://lrclib.net/api/search?track_name=" + Uri.EscapeDataString(title)
                     + "&artist_name=" + Uri.EscapeDataString(artist);
        using var list = await _http.GetJsonAsync(search, ct);
        if (list is null || list.RootElement.ValueKind != JsonValueKind.Array) return null;
        foreach (var item in list.RootElement.EnumerateArray().Take(10))
        {
            if (Parse(item) is { } r) return r;
        }
        return null;
    }

    internal static LyricsResult? Parse(JsonElement e)
    {
        if (e.ValueKind != JsonValueKind.Object) return null;
        if (e.TryGetProperty("instrumental", out var ins) && ins.ValueKind == JsonValueKind.True)
            return new LyricsResult(Array.Empty<LyricLine>(), false, true);

        if (e.TryGetProperty("syncedLyrics", out var syn) && syn.ValueKind == JsonValueKind.String)
        {
            var lines = ParseLrc(syn.GetString() ?? "");
            if (lines.Count > 0) return new LyricsResult(lines, true, false);
        }
        if (e.TryGetProperty("plainLyrics", out var plain) && plain.ValueKind == JsonValueKind.String)
        {
            var lines = (plain.GetString() ?? "")
                .Split('\n')
                .Select(l => Limit(l.Trim()))
                .Take(MaxLines)
                .Select(l => new LyricLine(TimeSpan.Zero, l))
                .ToList();
            if (lines.Any(l => l.Text.Length > 0)) return new LyricsResult(lines, false, false);
        }
        return null;
    }

    [GeneratedRegex(@"\[(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)\]")]
    private static partial Regex TimeTag();

    /// <summary>Format LRC: "[mm:ss.xx] teks"; satu baris bisa punya beberapa tag waktu.</summary>
    internal static List<LyricLine> ParseLrc(string lrc)
    {
        var result = new List<LyricLine>();
        foreach (var raw in lrc.Split('\n'))
        {
            var matches = TimeTag().Matches(raw);
            if (matches.Count == 0) continue;
            var text = Limit(raw[(matches[^1].Index + matches[^1].Length)..].Trim());
            foreach (Match m in matches)
            {
                var min = int.Parse(m.Groups[1].Value, CultureInfo.InvariantCulture);
                var sec = double.Parse(m.Groups[2].Value.Replace(':', '.'), CultureInfo.InvariantCulture);
                result.Add(new LyricLine(TimeSpan.FromMinutes(min) + TimeSpan.FromSeconds(sec), text));
            }
            if (result.Count >= MaxLines) break;
        }
        result.Sort((a, b) => a.Time.CompareTo(b.Time));
        return result;
    }

    /// <summary>Indeks baris terakhir yang waktunya ≤ posisi; -1 bila belum ada.</summary>
    public static int IndexAt(IReadOnlyList<LyricLine> lines, TimeSpan position)
    {
        int lo = 0, hi = lines.Count - 1, found = -1;
        while (lo <= hi)
        {
            var mid = (lo + hi) / 2;
            if (lines[mid].Time <= position) { found = mid; lo = mid + 1; }
            else hi = mid - 1;
        }
        return found;
    }

    private static string Clean(string? s)
    {
        var t = (s ?? "").Trim();
        return t.Length > 200 ? t[..200] : t;
    }
    private static string Limit(string s) => s.Length > MaxLineLength ? s[..MaxLineLength] : s;

    public void Dispose() => _http.Dispose();
}
