using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Models;
using PatPet.Services;
using Windows.Storage.Streams;

namespace PatPet.ViewModels;

public partial class NowPlayingViewModel : ObservableObject
{
    private const int CoverCacheSize = 4;
    private const long MaxCoverBytes = 2 * 1024 * 1024;

    private readonly SpotifyMediaService _spotify;
    private readonly Dictionary<string, BitmapSource> _coverCache = new();
    private readonly Queue<string> _coverOrder = new();
    private TrackInfo? _track;
    private string? _coverKey;
    private string? _loadingKey;
    private DateTime _seekStartedAt;

    [ObservableProperty] private bool _isAvailable;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(PlayPauseGlyph))]
    private bool _isPlaying;
    [ObservableProperty] private string _title = "";
    [ObservableProperty] private string _artist = "";
    [ObservableProperty] private ImageSource? _cover;
    [ObservableProperty] private double _positionSeconds;
    [ObservableProperty] private double _durationSeconds = 1;
    [ObservableProperty] private string _positionText = "0:00";
    [ObservableProperty] private string _durationText = "0:00";
    [ObservableProperty] private bool _isSeeking;
    [ObservableProperty] private bool _canSeek;
    [ObservableProperty] private string _statusText = "";

    // ---------- lirik ----------
    private const int LyricsCacheSize = 6;
    private readonly LyricsService _lyrics;
    private readonly Dictionary<string, LyricsResult?> _lyricsCache = new();
    private readonly Queue<string> _lyricsOrder = new();
    private CancellationTokenSource? _lyricsCts;
    private string? _lyricsKey;
    private LyricsResult? _currentLyrics;
    private bool _lyricsActive;

    [ObservableProperty] private string _lyricsStatus = "";
    [ObservableProperty] private bool _hasLyrics;
    [ObservableProperty] private bool _lyricsSynced;
    [ObservableProperty] private int _currentLyricIndex = -1;

    public System.Collections.ObjectModel.ObservableCollection<LyricLineViewModel> LyricLines { get; } = new();

    // ---------- lagu terakhir (untuk hover bar) ----------
    private readonly SettingsService? _settings;
    [ObservableProperty] private string _lastTitle = "";
    [ObservableProperty] private string _lastArtist = "";
    [ObservableProperty] private bool _hasLastTrack;

    public NowPlayingViewModel(SpotifyMediaService spotify, LyricsService lyrics, SettingsService? settings = null)
    {
        _spotify = spotify;
        _lyrics = lyrics;
        _settings = settings;
        if (settings?.Current.LastTrack is { Title.Length: > 0 } last)
        {
            _lastTitle = last.Title;
            _lastArtist = last.Artist;
            _hasLastTrack = true;
        }
        _spotify.TrackChanged += OnTrackChanged;
    }

    private void RememberTrack(string title, string artist)
    {
        if (string.IsNullOrWhiteSpace(title) || (title == LastTitle && artist == LastArtist)) return;
        LastTitle = title.Length > 120 ? title[..120] : title;
        LastArtist = artist.Length > 120 ? artist[..120] : artist;
        HasLastTrack = true;
        if (_settings is null) return;
        _settings.Current.LastTrack = new Models.LastTrack { Title = LastTitle, Artist = LastArtist };
        _settings.Save();
    }

    /// <summary>Halaman Musik terlihat: lirik diambil hanya saat dibutuhkan.</summary>
    public bool LyricsActive
    {
        get => _lyricsActive;
        set
        {
            _lyricsActive = value;
            if (value) _ = EnsureLyricsAsync();
            else _lyricsCts?.Cancel();
        }
    }

    private async Task EnsureLyricsAsync()
    {
        var t = _track;
        if (!_lyricsActive || t is null) return;
        var key = t.Title + "|" + t.Artist;
        if (key == _lyricsKey) return;

        _lyricsKey = key;
        _lyricsCts?.Cancel();
        _lyricsCts?.Dispose();

        if (_lyricsCache.TryGetValue(key, out var cached))
        {
            ShowLyrics(cached);
            return;
        }

        ShowLyrics(null, "Mencari lirik…");
        var cts = _lyricsCts = new CancellationTokenSource(TimeSpan.FromSeconds(40));
        LyricsResult? result = null;
        var failed = false;
        try
        {
            result = await _lyrics.GetAsync(t.Title, t.Artist, t.Album, t.Duration, cts.Token);
        }
        catch (OperationCanceledException)
        {
            if (_lyricsKey == key) _lyricsKey = null; // dicoba lagi saat halaman dibuka lagi
            return;
        }
        catch
        {
            failed = true;
        }

        if (!failed)
        {
            _lyricsCache[key] = result;
            _lyricsOrder.Enqueue(key);
            while (_lyricsOrder.Count > LyricsCacheSize) _lyricsCache.Remove(_lyricsOrder.Dequeue());
        }
        if (_lyricsKey == key) ShowLyrics(result, failed ? "Lirik gagal dimuat. Cek koneksi internet." : null);
        if (failed && _lyricsKey == key) _lyricsKey = null;
    }

    private void ShowLyrics(LyricsResult? r, string? status = null)
    {
        _currentLyrics = r;
        LyricLines.Clear();
        CurrentLyricIndex = -1;
        if (r is not null)
        {
            foreach (var l in r.Lines) LyricLines.Add(new LyricLineViewModel(l.Text));
        }
        HasLyrics = LyricLines.Count > 0;
        LyricsSynced = r?.Synced == true;
        LyricsStatus = status
                       ?? (r is { Instrumental: true } ? "Lagu instrumental, tanpa lirik"
                           : HasLyrics ? (LyricsSynced ? "" : "Lirik ini tidak mengikuti lagu")
                           : "Lirik tidak ditemukan");
        UpdateLyricPosition();
    }

    /// <summary>Sorot baris yang sedang dinyanyikan (dipanggil ±4×/detik oleh halaman Musik).</summary>
    public void UpdateLyricPosition()
    {
        if (_track is null || _currentLyrics is not { Synced: true } lyr || LyricLines.Count == 0) return;
        var pos = IsSeeking ? TimeSpan.FromSeconds(PositionSeconds) : CurrentPosition(_track);
        var idx = LyricsService.IndexAt(lyr.Lines, pos + TimeSpan.FromMilliseconds(250));
        if (idx == CurrentLyricIndex) return;
        if (CurrentLyricIndex >= 0 && CurrentLyricIndex < LyricLines.Count) LyricLines[CurrentLyricIndex].IsCurrent = false;
        if (idx >= 0 && idx < LyricLines.Count) LyricLines[idx].IsCurrent = true;
        CurrentLyricIndex = idx;
    }

    public string PlayPauseGlyph => IsPlaying ? "" : "";

    public event Action<bool>? PlayingChanged;

    partial void OnIsPlayingChanged(bool value) => PlayingChanged?.Invoke(value);

    partial void OnPositionSecondsChanged(double value)
    {
        if (IsSeeking) PositionText = Format(TimeSpan.FromSeconds(value)); // pratinjau saat menyeret
    }

    public async Task InitAsync()
    {
        try { await _spotify.InitAsync(); }
        catch { IsAvailable = false; }
    }

    private void OnTrackChanged(TrackInfo? track) =>
        Application.Current?.Dispatcher.InvokeAsync(() => Apply(track)); // event SMTC dari thread non-UI

    private void Apply(TrackInfo? t)
    {
        _track = t;
        if (t is null)
        {
            IsAvailable = false;
            IsPlaying = false;
            Title = Artist = "";
            Cover = null;
            _coverKey = null;
            PositionSeconds = 0;
            _lyricsCts?.Cancel();
            _lyricsKey = null;
            ShowLyrics(null, "");
            return;
        }

        IsAvailable = true;
        CanSeek = _spotify.CanSeek;
        Title = string.IsNullOrWhiteSpace(t.Title) ? "Tanpa judul" : t.Title;
        Artist = string.IsNullOrWhiteSpace(t.Artist) ? "Spotify" : t.Artist;
        IsPlaying = t.IsPlaying;
        if (!string.IsNullOrWhiteSpace(t.Title)) RememberTrack(Title, Artist);

        var key = t.Title + "|" + t.Artist;
        if (key != _coverKey)
        {
            _coverKey = key;
            Cover = _coverCache.TryGetValue(key, out var cached) ? cached : null;
        }
        if (Cover is null && t.Thumbnail is not null && _loadingKey != key)
            _ = LoadCoverAsync(key, t.Thumbnail);

        UpdateProgress();
        if (_lyricsActive && key != _lyricsKey) _ = EnsureLyricsAsync();
        else UpdateLyricPosition();
    }

    private async Task LoadCoverAsync(string key, IRandomAccessStreamReference thumb)
    {
        _loadingKey = key;
        BitmapSource? img = null;
        try
        {
            img = await Task.Run(async () =>
            {
                using var ras = await thumb.OpenReadAsync();
                if (ras.Size > MaxCoverBytes) return null; // tolak gambar tidak wajar
                using var src = ras.AsStreamForRead();
                var ms = new MemoryStream((int)ras.Size);
                await src.CopyToAsync(ms);
                ms.Position = 0;
                var bmp = new BitmapImage();
                bmp.BeginInit();
                bmp.CacheOption = BitmapCacheOption.OnLoad; // stream boleh dibuang setelah decode
                bmp.DecodePixelWidth = 128;
                bmp.StreamSource = ms;
                bmp.EndInit();
                bmp.Freeze();
                ms.Dispose();
                return (BitmapSource)bmp;
            });
        }
        catch
        {
            // lagu lokal tanpa cover, iklan, dll.
        }
        finally
        {
            if (_loadingKey == key) _loadingKey = null;
        }

        if (img is null) return;
        if (!_coverCache.ContainsKey(key))
        {
            _coverCache[key] = img;
            _coverOrder.Enqueue(key);
            while (_coverOrder.Count > CoverCacheSize) _coverCache.Remove(_coverOrder.Dequeue());
        }
        if (_coverKey == key) Cover = img;
    }

    /// <summary>SMTC tidak mengirim posisi tiap detik; dihitung dari tick jam.</summary>
    public void UpdateProgress()
    {
        if (_track is null) return;

        // jangan timpa slider saat pengguna menyeret (dengan batas aman bila mouse-up hilang)
        if (IsSeeking)
        {
            if (DateTime.Now - _seekStartedAt < TimeSpan.FromSeconds(15)) return;
            IsSeeking = false;
        }

        var pos = CurrentPosition(_track);
        var dur = _track.Duration;
        DurationSeconds = Math.Max(1, dur.TotalSeconds);
        PositionSeconds = Math.Min(pos.TotalSeconds, DurationSeconds);
        PositionText = Format(pos);
        DurationText = Format(dur);
    }

    private TimeSpan CurrentPosition(TrackInfo t)
    {
        var pos = t.Position;
        if (IsPlaying) pos += DateTimeOffset.Now - t.LastUpdated;
        if (pos < TimeSpan.Zero) pos = TimeSpan.Zero;
        if (t.Duration > TimeSpan.Zero && pos > t.Duration) pos = t.Duration;
        return pos;
    }

    public void BeginSeek()
    {
        if (_track is null) return;
        _seekStartedAt = DateTime.Now;
        IsSeeking = true;
    }

    public async Task EndSeekAsync()
    {
        if (!IsSeeking || _track is null) return;
        var target = TimeSpan.FromSeconds(Math.Clamp(PositionSeconds, 0, DurationSeconds));
        IsSeeking = false;

        // optimis: langsung tampilkan posisi baru, SMTC akan mengonfirmasi lewat event
        _track = _track with { Position = target, LastUpdated = DateTimeOffset.Now };
        UpdateProgress();

        bool ok;
        try { ok = await _spotify.SeekAsync(target); }
        catch { ok = false; }
        StatusText = ok ? "" : "Spotify tidak mau pindah posisi";
    }

    private static string Format(TimeSpan t) =>
        t.TotalHours >= 1 ? $"{(int)t.TotalHours}:{t.Minutes:00}:{t.Seconds:00}" : $"{t.Minutes}:{t.Seconds:00}";

    [RelayCommand]
    private async Task PlayPause() { try { await _spotify.PlayPause(); } catch { } }

    [RelayCommand]
    private async Task Next() { try { await _spotify.Next(); } catch { } }

    [RelayCommand]
    private async Task Previous() { try { await _spotify.Previous(); } catch { } }
}
