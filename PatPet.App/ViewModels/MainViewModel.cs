using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Models;
using PatPet.Services;

namespace PatPet.ViewModels;

public enum Page { Clock, Music, Focus, Notes, Settings }

public partial class MainViewModel : ObservableObject
{
    private readonly SettingsService _settings;
    private readonly ClockService _clock;
    private readonly ActivityService _activity;
    private readonly FocusTimerService _timer;
    private readonly PetBrain _brain;
    private int _lastMinute = -1;

    [ObservableProperty] private bool _isExpanded;
    [ObservableProperty] private bool _isHoverOpen;
    [ObservableProperty] private bool _alwaysOnTop;
    [ObservableProperty] private bool _hideInFullscreen;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsBarMode))]
    private DisplayMode _mode;
    [ObservableProperty] private Page _currentPage = Page.Clock;

    public MainViewModel(
        ClockViewModel clock,
        NowPlayingViewModel nowPlaying,
        NotesViewModel notes,
        TimerViewModel timerVm,
        PetViewModel pet,
        SettingsViewModel settingsVm,
        SystemStatusViewModel system,
        SettingsService settings,
        ClockService clockService,
        ActivityService activity,
        FocusTimerService timer,
        PetBrain brain)
    {
        Clock = clock;
        NowPlaying = nowPlaying;
        Notes = notes;
        Timer = timerVm;
        Pet = pet;
        Settings = settingsVm;
        System = system;
        _settings = settings;
        _clock = clockService;
        _activity = activity;
        _timer = timer;
        _brain = brain;

        _alwaysOnTop = settings.Current.AlwaysOnTop;
        _hideInFullscreen = settings.Current.HideInFullscreen;
        _mode = settings.Current.Mode;

        // sinyal → PetBrain
        NowPlaying.PlayingChanged += _brain.MusicChanged;
        _timer.Changed += c => _brain.FocusChanged(_timer.IsActive, c.Completed);
        Notes.NoteAdded += _brain.NoteAdded;
        _clock.Tick += OnTick;

        Clock.Update(DateTime.Now);
        System.Update(DateTime.Now);
    }

    public ClockViewModel Clock { get; }
    public NowPlayingViewModel NowPlaying { get; }
    public NotesViewModel Notes { get; }
    public TimerViewModel Timer { get; }
    public PetViewModel Pet { get; }
    public SettingsViewModel Settings { get; }
    public SystemStatusViewModel System { get; }

    public bool IsBarMode => Mode == DisplayMode.Bar;

    public async Task InitializeAsync()
    {
        _clock.Start();
        try { await Notes.LoadAsync(); } catch { /* DB bermasalah: tetap jalan */ }
        await NowPlaying.InitAsync();
        await Clock.RefreshAsync(forceLocation: false);
    }

    private void OnTick(DateTime now)
    {
        Clock.Update(now);
        _timer.Tick();
        if (_timer.IsActive || IsExpanded) Timer.Refresh(); // bar juga menampilkan sisa timer
        _brain.TimeTick(_activity.GetIdleTime());
        if (IsBarMode || IsExpanded) System.Update(now);    // baterai & suhu hanya dibaca bila terlihat

        if (IsExpanded || IsHoverOpen) NowPlaying.UpdateProgress();

        if (now.Minute != _lastMinute)
        {
            _lastMinute = now.Minute;
            _ = Notes.CheckDeadlinesAsync(now);
            Clock.MaybeRefreshWeather(now);
        }
    }

    partial void OnIsHoverOpenChanged(bool value)
    {
        if (value) NowPlaying.UpdateProgress();
    }

    partial void OnIsExpandedChanged(bool value)
    {
        if (!value) return;
        NowPlaying.UpdateProgress();
        Timer.Refresh();
    }

    partial void OnAlwaysOnTopChanged(bool value)
    {
        _settings.Current.AlwaysOnTop = value;
        _settings.Save();
    }

    partial void OnHideInFullscreenChanged(bool value)
    {
        _settings.Current.HideInFullscreen = value;
        _settings.Save();
    }

    partial void OnModeChanged(DisplayMode value)
    {
        _settings.Current.Mode = value;
        _settings.Save();
        if (value == DisplayMode.Bar) System.Update(DateTime.Now);
    }

    [RelayCommand]
    private void ToggleExpanded() => IsExpanded = !IsExpanded;

    [RelayCommand]
    private void Collapse() => IsExpanded = false;

    [RelayCommand]
    private void SetMode(string? mode)
    {
        if (Enum.TryParse<DisplayMode>(mode, out var m)) Mode = m;
    }

    public async Task ShutdownAsync()
    {
        _clock.Stop();
        try { await Notes.FlushAsync(); } catch { }
        _settings.Save();
    }
}
