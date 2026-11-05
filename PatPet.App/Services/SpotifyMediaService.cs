using PatPet.Models;
using Windows.Media.Control;

namespace PatPet.Services;

/// <summary>Membaca sesi Spotify desktop lewat Windows Media Session (SMTC). Berbasis event, tanpa polling.</summary>
public sealed class SpotifyMediaService : IDisposable
{
    private readonly object _gate = new();
    private GlobalSystemMediaTransportControlsSessionManager? _mgr;
    private GlobalSystemMediaTransportControlsSession? _session;

    /// <summary>Dipanggil dari thread non-UI. null = Spotify tidak aktif.</summary>
    public event Action<TrackInfo?>? TrackChanged;

    public bool IsPlaying { get; private set; }

    public async Task InitAsync()
    {
        _mgr = await GlobalSystemMediaTransportControlsSessionManager.RequestAsync();
        _mgr.SessionsChanged += OnSessionsChanged;
        AttachSpotify();
    }

    private void OnSessionsChanged(GlobalSystemMediaTransportControlsSessionManager sender, SessionsChangedEventArgs args)
        => AttachSpotify();

    private void AttachSpotify()
    {
        GlobalSystemMediaTransportControlsSession? s;
        try
        {
            s = _mgr?.GetSessions().FirstOrDefault(x =>
                x.SourceAppUserModelId?.Contains("Spotify", StringComparison.OrdinalIgnoreCase) == true);
        }
        catch
        {
            s = null;
        }

        lock (_gate)
        {
            if (ReferenceEquals(s, _session)) return;
            Detach();
            _session = s;
            if (s is not null)
            {
                s.MediaPropertiesChanged += OnMediaPropertiesChanged;
                s.PlaybackInfoChanged += OnPlaybackInfoChanged;
                s.TimelinePropertiesChanged += OnTimelinePropertiesChanged;
            }
        }

        if (s is null)
        {
            IsPlaying = false;
            TrackChanged?.Invoke(null);
        }
        else
        {
            _ = PublishAsync();
        }
    }

    private void OnMediaPropertiesChanged(GlobalSystemMediaTransportControlsSession sender, MediaPropertiesChangedEventArgs args) => _ = PublishAsync();
    private void OnPlaybackInfoChanged(GlobalSystemMediaTransportControlsSession sender, PlaybackInfoChangedEventArgs args) => _ = PublishAsync();
    private void OnTimelinePropertiesChanged(GlobalSystemMediaTransportControlsSession sender, TimelinePropertiesChangedEventArgs args) => _ = PublishAsync();

    private async Task PublishAsync()
    {
        var session = _session;
        if (session is null) return;
        try
        {
            var props = await session.TryGetMediaPropertiesAsync();
            var play = session.GetPlaybackInfo();
            var time = session.GetTimelineProperties();
            if (!ReferenceEquals(session, _session)) return;

            IsPlaying = play?.PlaybackStatus == GlobalSystemMediaTransportControlsSessionPlaybackStatus.Playing;
            var updated = time.LastUpdatedTime == default ? DateTimeOffset.Now : time.LastUpdatedTime;
            TrackChanged?.Invoke(new TrackInfo(
                props?.Title ?? "",
                props?.Artist ?? "",
                props?.AlbumTitle ?? "",
                props?.Thumbnail,
                IsPlaying,
                time.Position,
                time.EndTime,
                updated));
        }
        catch
        {
            // sesi bisa tertutup di tengah jalan; abaikan, SessionsChanged akan menyusul
        }
    }

    /// <summary>Lompat ke posisi tertentu di lagu tanpa membuka Spotify.</summary>
    public async Task<bool> SeekAsync(TimeSpan position)
    {
        var s = _session;
        if (s is null || position < TimeSpan.Zero) return false;
        return await s.TryChangePlaybackPositionAsync(position.Ticks);
    }

    public bool CanSeek
    {
        get
        {
            try { return _session?.GetPlaybackInfo()?.Controls?.IsPlaybackPositionEnabled == true; }
            catch { return false; }
        }
    }

    public Task PlayPause() => _session?.TryTogglePlayPauseAsync().AsTask() ?? Task.CompletedTask;
    public Task Next() => _session?.TrySkipNextAsync().AsTask() ?? Task.CompletedTask;
    public Task Previous() => _session?.TrySkipPreviousAsync().AsTask() ?? Task.CompletedTask;

    private void Detach()
    {
        if (_session is null) return;
        _session.MediaPropertiesChanged -= OnMediaPropertiesChanged;
        _session.PlaybackInfoChanged -= OnPlaybackInfoChanged;
        _session.TimelinePropertiesChanged -= OnTimelinePropertiesChanged;
        _session = null;
    }

    public void Dispose()
    {
        lock (_gate) Detach();
        if (_mgr is not null) _mgr.SessionsChanged -= OnSessionsChanged;
    }
}
