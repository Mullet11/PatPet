using System.Net;
using System.Net.Http;
using System.Text.Json;

namespace PatPet.Services;

/// <summary>HttpClient yang aman dipakai bersama: hanya HTTPS, timeout, dan ukuran respons dibatasi.</summary>
internal static class HttpClients
{
    public static HttpClient Create(TimeSpan timeout, long maxResponseBytes)
    {
        var handler = new SocketsHttpHandler
        {
            AutomaticDecompression = DecompressionMethods.All,
            ConnectTimeout = TimeSpan.FromSeconds(Math.Min(timeout.TotalSeconds, 25)),
            PooledConnectionLifetime = TimeSpan.FromMinutes(5),
            PooledConnectionIdleTimeout = TimeSpan.FromSeconds(30), // lepas socket saat idle
            MaxConnectionsPerServer = 2,
        };
        var http = new HttpClient(handler)
        {
            Timeout = timeout,
            MaxResponseContentBufferSize = maxResponseBytes,
        };
        http.DefaultRequestHeaders.UserAgent.ParseAdd("PatPet/1.2 (Windows desktop widget)");
        http.DefaultRequestHeaders.AcceptLanguage.ParseAdd("id");
        return http;
    }

    /// <summary>GET JSON; null bila gagal, offline, bukan HTTPS, terlalu besar, atau JSON tidak valid.</summary>
    public static async Task<JsonDocument?> GetJsonAsync(this HttpClient http, string url, CancellationToken ct)
    {
        if (!url.StartsWith("https://", StringComparison.Ordinal)) return null;
        try
        {
            using var resp = await http.GetAsync(url, HttpCompletionOption.ResponseContentRead, ct);
            if (!resp.IsSuccessStatusCode) return null;
            await using var s = await resp.Content.ReadAsStreamAsync(ct);
            return await JsonDocument.ParseAsync(s, new JsonDocumentOptions { MaxDepth = 16 }, ct);
        }
        catch
        {
            return null;
        }
    }
}
